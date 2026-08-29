//! Python worker 运行环境自举：固定版 uv + 托管 CPython 3.12 + uv.lock 同步与校验修复。
//!
//! 布局（root = 便携 exe 同级目录，或开发仓库根）：
//! - `<root>/pyproject.toml` / `uv.lock` / `uv.toml`：依赖与同步配置真源，缺失即拒绝
//! - `<root>/.venv`：worker 环境；修复走 staging 原子替换，同步前绝不删除旧环境
//! - `<root>/.camouforge-runtime/`：uv 工具（tools/uv/<ver>）、托管 Python（python/）、缓存（cache/）

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const UV_VERSION: &str = "0.12.4";
const MANIFEST_SCHEMA: u32 = 1;
const MANIFEST_NAME: &str = ".camouforge-env.json";
const REQUIRED_PYTHON_MINOR: u32 = 12;
const REQUIRED_CAMOUFOX: &str = "0.5.5";
const REQUIRED_BROWSERFORGE: &str = "1.2.4";
pub(crate) const WORKER_FILES: &[&str] = &[
    "camoforge_worker.py",
    "browser_patches.py",
    "downloads.py",
    "manager.py",
    "profile_translate.py",
    "sdk_bridge.py",
    "win_process.py",
];
/// uv.zip ~20MB；Content-Length 缺失时的流式硬上限。
const MAX_UV_ZIP_BYTES: u64 = 128 * 1024 * 1024;
const PROBE_TIMEOUT: Duration = Duration::from_secs(120);
const UV_SYNC_TIMEOUT: Duration = Duration::from_secs(30 * 60);

#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const UV_ASSET_NAME: &str = "uv-x86_64-pc-windows-msvc.zip";
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const UV_ASSET_SHA256: &str = "4f3b7d63cd81fca0da5a655d973d20affca89ce6e5f9a29fd0183cc4204a7639";
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const UV_ASSET_NAME: &str = "uv-aarch64-pc-windows-msvc.zip";
#[cfg(all(target_os = "windows", target_arch = "aarch64"))]
const UV_ASSET_SHA256: &str = "3290abffee78c30e3113f5113e26684fd057287e89124a588dcdcdd6ceec0fea";

pub fn uv_asset_name() -> &'static str {
    #[cfg(all(
        target_os = "windows",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        UV_ASSET_NAME
    }
    #[cfg(not(all(
        target_os = "windows",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    {
        ""
    }
}

pub fn uv_asset_sha256() -> &'static str {
    #[cfg(all(
        target_os = "windows",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    {
        UV_ASSET_SHA256
    }
    #[cfg(not(all(
        target_os = "windows",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    {
        ""
    }
}

/// 下载源：releases.astral.sh 优先，GitHub release 回退；两个来源共用同一 SHA-256 校验。
pub fn uv_download_urls() -> Vec<String> {
    let asset = uv_asset_name();
    if asset.is_empty() {
        return Vec::new();
    }
    vec![
        format!("https://releases.astral.sh/github/uv/releases/download/{UV_VERSION}/{asset}"),
        format!("https://github.com/astral-sh/uv/releases/download/{UV_VERSION}/{asset}"),
    ]
}

pub fn uv_exe_name() -> &'static str {
    if cfg!(windows) {
        "uv.exe"
    } else {
        "uv"
    }
}

pub fn venv_python_path(venv: &Path) -> PathBuf {
    venv.join(if cfg!(windows) {
        "Scripts/python.exe"
    } else {
        "bin/python"
    })
}

/// 运行根目录布局的统一解析结果。
#[derive(Debug, Clone)]
pub struct RuntimeLayout {
    pub root: PathBuf,
    pub worker_script: PathBuf,
    pub pyproject: PathBuf,
    pub uv_lock: PathBuf,
    pub uv_config: PathBuf,
    pub venv_dir: PathBuf,
    pub venv_python: PathBuf,
    #[allow(dead_code)] // 布局完整性字段：runtime_dir 是 uv_dir/cache/python 的共同父目录
    pub runtime_dir: PathBuf,
    pub uv_dir: PathBuf,
    pub uv_exe: PathBuf,
    pub uv_cache_dir: PathBuf,
    pub managed_python_dir: PathBuf,
}

impl RuntimeLayout {
    /// exe 同级存在 worker 入口即便携部署；否则回退开发仓库根。
    /// 便携目录缺少任一 worker 模块或自举输入时不得回退开发目录。
    pub fn resolve(exe_dir: &Path, manifest_dir: &Path) -> Result<Self> {
        let portable = exe_dir.join("worker").join("camoforge_worker.py").exists();
        let root = if portable {
            exe_dir.to_path_buf()
        } else {
            manifest_dir
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .ok_or_else(|| anyhow!("无法从 {} 推导仓库根目录", manifest_dir.display()))?
        };
        let worker_dir = root.join("worker");
        let worker_script = worker_dir.join("camoforge_worker.py");
        let pyproject = root.join("pyproject.toml");
        let uv_lock = root.join("uv.lock");
        let uv_config = root.join("uv.toml");
        let mut required: Vec<PathBuf> = WORKER_FILES
            .iter()
            .map(|name| worker_dir.join(name))
            .collect();
        required.extend([pyproject.clone(), uv_lock.clone(), uv_config.clone()]);
        let missing: Vec<String> = required
            .iter()
            .filter(|p| !p.exists())
            .map(|p| {
                p.strip_prefix(&root)
                    .unwrap_or(p)
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();
        if !missing.is_empty() {
            let kind = if portable {
                "便携目录"
            } else {
                "开发目录"
            };
            return Err(anyhow!(
                "发布包不完整：{kind} {} 缺少 {}",
                root.display(),
                missing.join("、")
            ));
        }
        let venv_dir = root.join(".venv");
        let runtime_dir = root.join(".camouforge-runtime");
        let uv_dir = runtime_dir.join("tools").join("uv");
        Ok(Self {
            root,
            worker_script,
            pyproject,
            uv_lock,
            uv_config,
            venv_python: venv_python_path(&venv_dir),
            venv_dir,
            uv_exe: uv_dir.join(UV_VERSION).join(uv_exe_name()),
            uv_dir,
            uv_cache_dir: runtime_dir.join("cache"),
            managed_python_dir: runtime_dir.join("python"),
            runtime_dir,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnsureMode {
    IfNeeded,
    ForceRepair,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PythonEnvPhase {
    Checking,
    DownloadingUv { downloaded: u64, total: Option<u64> },
    PreparingPython,
    SyncingDependencies,
    Verifying,
    Ready,
    Failed,
}

impl PythonEnvPhase {
    pub fn label(&self) -> String {
        match self {
            PythonEnvPhase::Checking => "正在检查 Python 环境…".into(),
            PythonEnvPhase::DownloadingUv { downloaded, total } => match total {
                Some(t) if *t > 0 => format!(
                    "正在下载 uv…（{:.1} / {:.1} MB）",
                    *downloaded as f64 / 1048576.0,
                    *t as f64 / 1048576.0
                ),
                _ => format!("正在下载 uv…（{:.1} MB）", *downloaded as f64 / 1048576.0),
            },
            PythonEnvPhase::PreparingPython => "正在准备 Python 3.12…".into(),
            PythonEnvPhase::SyncingDependencies => "正在同步 worker 依赖…".into(),
            PythonEnvPhase::Verifying => "正在验证 Python 环境…".into(),
            PythonEnvPhase::Ready => "Python 环境就绪".into(),
            PythonEnvPhase::Failed => "Python 环境配置失败".into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PythonEnvInfo {
    pub python: PathBuf,
    pub python_version: String,
    pub uv_version: String,
    pub venv: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
struct EnvManifest {
    schema: u32,
    lock_sha256: String,
    python_minor: u32,
    uv_version: String,
    created_at: u64,
}

/// 隔离探测脚本：版本/发行版断言失败或 import 失败都以非零退出结束。
pub fn probe_script() -> String {
    format!(
        r#"
import importlib.metadata as m, sys
assert sys.version_info[:2] == (3, {REQUIRED_PYTHON_MINOR}), f"python {{sys.version}}"
assert m.version("camoufox") == "{REQUIRED_CAMOUFOX}", f"camoufox {{m.version('camoufox')}}"
assert m.version("browserforge") == "{REQUIRED_BROWSERFORGE}", f"browserforge {{m.version('browserforge')}}"
import camoufox  # noqa
import browserforge  # noqa
print(".".join(map(str, sys.version_info[:3])))
"#
    )
}

/// 进程/网络注入点：生产用 [`RealBackend`]，测试注入假实现。
pub trait Backend {
    fn download_uv_zip(
        &self,
        urls: &[String],
        dest: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<()>;

    fn uv_version(&self, uv_exe: &Path) -> Result<String>;

    fn probe_python(&self, python: &Path) -> Result<String>;

    fn uv_sync(&self, plan: &UvSyncPlan) -> Result<()>;
}

/// uv sync 的完整命令规格（纯数据，测试直接断言）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UvSyncPlan {
    pub uv_exe: PathBuf,
    pub args: Vec<String>,
    pub env_remove: Vec<String>,
    pub env_set: Vec<(String, String)>,
    pub staging_venv: PathBuf,
}

pub fn build_sync_plan(layout: &RuntimeLayout, staging_venv: &Path) -> UvSyncPlan {
    UvSyncPlan {
        uv_exe: layout.uv_exe.clone(),
        args: vec![
            "sync".into(),
            "--project".into(),
            layout.root.to_string_lossy().into_owned(),
            "--config-file".into(),
            layout.uv_config.to_string_lossy().into_owned(),
            "--frozen".into(),
            "--no-dev".into(),
            "--no-install-project".into(),
            "--managed-python".into(),
            "--python".into(),
            "3.12".into(),
            "--link-mode".into(),
            "copy".into(),
            "--system-certs".into(),
            "--no-progress".into(),
        ],
        env_remove: [
            "VIRTUAL_ENV",
            "CONDA_PREFIX",
            "UV_PROJECT",
            "UV_PROJECT_ENVIRONMENT",
            "UV_PYTHON",
            "UV_PYTHON_INSTALL_DIR",
            "UV_CACHE_DIR",
            "UV_OFFLINE",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect(),
        env_set: vec![
            (
                "UV_PROJECT_ENVIRONMENT".into(),
                staging_venv.to_string_lossy().into_owned(),
            ),
            (
                "UV_PYTHON_INSTALL_DIR".into(),
                layout.managed_python_dir.to_string_lossy().into_owned(),
            ),
            (
                "UV_CACHE_DIR".into(),
                layout.uv_cache_dir.to_string_lossy().into_owned(),
            ),
        ],
        staging_venv: staging_venv.to_path_buf(),
    }
}

pub struct RealBackend;

impl Backend for RealBackend {
    fn download_uv_zip(
        &self,
        urls: &[String],
        dest: &Path,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<()> {
        if urls.is_empty() {
            bail!("当前平台没有预配置的 uv 下载资产");
        }
        let agent = crate::net::http_agent()?;
        let mut last_err: Option<anyhow::Error> = None;
        for url in urls {
            match download_to_file(&agent, url, dest, progress) {
                Ok(()) => return Ok(()),
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow!("无可用下载源")))
    }

    fn uv_version(&self, uv_exe: &Path) -> Result<String> {
        let mut cmd = Command::new(uv_exe);
        cmd.arg("--version");
        let (ok, out) = run_captured(&mut cmd, Duration::from_secs(30))
            .map_err(|e| anyhow!("执行 {} --version 失败: {e:#}", uv_exe.display()))?;
        if !ok {
            bail!("{} --version 退出异常", uv_exe.display());
        }
        Ok(out.trim().to_string())
    }

    fn probe_python(&self, python: &Path) -> Result<String> {
        let mut cmd = Command::new(python);
        cmd.arg("-I").arg("-c").arg(probe_script());
        let (ok, out) = run_captured(&mut cmd, PROBE_TIMEOUT)
            .map_err(|e| anyhow!("运行探测命令失败: {e:#}"))?;
        if !ok {
            bail!("环境探测未通过:\n{}", tail(&out, 2000));
        }
        let v = out.lines().next().unwrap_or("").trim().to_string();
        if v.is_empty() {
            bail!("环境探测无输出");
        }
        Ok(v)
    }

    fn uv_sync(&self, plan: &UvSyncPlan) -> Result<()> {
        let mut cmd = Command::new(&plan.uv_exe);
        cmd.args(&plan.args);
        for k in &plan.env_remove {
            cmd.env_remove(k);
        }
        for (k, v) in &plan.env_set {
            cmd.env(k, v);
        }
        let (ok, out) = run_captured(&mut cmd, UV_SYNC_TIMEOUT)?;
        if !ok {
            bail!("uv sync 失败:\n{}", tail(&out, 4000));
        }
        Ok(())
    }
}

fn download_to_file(
    agent: &ureq::Agent,
    url: &str,
    dest: &Path,
    progress: &mut dyn FnMut(u64, Option<u64>),
) -> Result<()> {
    let resp = agent
        .get(url)
        .call()
        .with_context(|| format!("下载请求失败: {url}"))?;
    let total = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());
    if let Some(t) = total {
        if t > MAX_UV_ZIP_BYTES {
            bail!("uv 压缩包超过大小上限（{t} 字节）");
        }
    }
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(dest).with_context(|| format!("创建 {}", dest.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_report: u64 = 0;
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| anyhow!("下载中断: {e}"))?;
        if n == 0 {
            break;
        }
        downloaded += n as u64;
        if downloaded > MAX_UV_ZIP_BYTES {
            bail!("下载超过大小上限（{MAX_UV_ZIP_BYTES} 字节），已中止");
        }
        writer
            .write_all(&buf[..n])
            .with_context(|| format!("写入 {}", dest.display()))?;
        if downloaded - last_report >= 512 * 1024 {
            progress(downloaded, total);
            last_report = downloaded;
        }
    }
    writer.flush()?;
    progress(downloaded, total);
    Ok(())
}

/// 带 CREATE_NO_WINDOW 与硬超时的子进程执行；返回 (成功与否, stdout+stderr 合并)。
fn run_captured(cmd: &mut Command, timeout: Duration) -> Result<(bool, String)> {
    crate::command::hide_console_window(cmd);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().context("spawn 子进程")?;
    let mut out_pipe = child.stdout.take().expect("piped stdout");
    let mut err_pipe = child.stderr.take().expect("piped stderr");
    let t_out = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = out_pipe.read_to_end(&mut s);
        s
    });
    let t_err = std::thread::spawn(move || {
        let mut s = Vec::new();
        let _ = err_pipe.read_to_end(&mut s);
        s
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(st)) => break Some(st),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(e) => return Err(e.into()),
        }
    };
    let out_bytes = t_out.join().unwrap_or_default();
    let err_bytes = t_err.join().unwrap_or_default();
    let mut text = String::from_utf8_lossy(&out_bytes).into_owned();
    let err_text = String::from_utf8_lossy(&err_bytes);
    if !err_text.trim().is_empty() {
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&err_text);
    }
    match status {
        Some(st) => Ok((st.success(), text)),
        None => Err(anyhow!("子进程超时（{:?}），已终止", timeout)),
    }
}

fn tail(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        s.to_string()
    } else {
        s.chars().skip(count - max_chars).collect()
    }
}

pub(crate) fn sha256_file(path: &Path) -> Result<String> {
    let mut f = std::fs::File::open(path).with_context(|| format!("打开 {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn unix_ts() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn read_manifest(venv: &Path) -> Result<EnvManifest> {
    let path = venv.join(MANIFEST_NAME);
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("读取环境清单 {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("解析环境清单 {}", path.display()))
}

fn write_manifest(venv: &Path, lock_sha: &str) -> Result<()> {
    let manifest = EnvManifest {
        schema: MANIFEST_SCHEMA,
        lock_sha256: lock_sha.to_string(),
        python_minor: REQUIRED_PYTHON_MINOR,
        uv_version: UV_VERSION.to_string(),
        created_at: unix_ts(),
    };
    let path = venv.join(MANIFEST_NAME);
    std::fs::write(
        &path,
        serde_json::to_vec_pretty(&manifest).context("序列化环境清单")?,
    )
    .with_context(|| format!("写入环境清单 {}", path.display()))
}

/// 完整入口：快速校验通过即 Ready（不碰网络）；否则走 staging 修复。
pub fn ensure_python_env(
    layout: &RuntimeLayout,
    mode: EnsureMode,
    progress: impl Fn(PythonEnvPhase),
) -> Result<PythonEnvInfo> {
    ensure_python_env_with(layout, mode, progress, &RealBackend)
}

pub fn ensure_python_env_with(
    layout: &RuntimeLayout,
    mode: EnsureMode,
    progress: impl Fn(PythonEnvPhase),
    backend: &dyn Backend,
) -> Result<PythonEnvInfo> {
    let result = match mode {
        EnsureMode::IfNeeded => match fast_check(layout, backend, &progress) {
            Ok(info) => {
                cleanup_backup_dirs(&layout.root);
                Ok(info)
            }
            Err(reason) => {
                repair(layout, backend, &progress).map_err(|e| anyhow!("{reason}；修复失败: {e:#}"))
            }
        },
        EnsureMode::ForceRepair => repair(layout, backend, &progress),
    };
    match result {
        Ok(info) => {
            progress(PythonEnvPhase::Ready);
            Ok(info)
        }
        Err(e) => {
            progress(PythonEnvPhase::Failed);
            Err(e)
        }
    }
}

/// 离线快速路径：解释器存在 + 清单匹配 + 探测通过；全程不运行 uv、不访问网络。
fn fast_check(
    layout: &RuntimeLayout,
    backend: &dyn Backend,
    progress: &impl Fn(PythonEnvPhase),
) -> Result<PythonEnvInfo> {
    if !layout.venv_python.exists() {
        bail!(".venv 解释器缺失（{}）", layout.venv_python.display());
    }
    let manifest = read_manifest(&layout.venv_dir)?;
    if manifest.schema != MANIFEST_SCHEMA {
        bail!(
            "环境清单 schema 不匹配（{} ≠ {MANIFEST_SCHEMA}）",
            manifest.schema
        );
    }
    if manifest.python_minor != REQUIRED_PYTHON_MINOR {
        bail!(
            "Python 版本不符（3.{} ≠ 3.{REQUIRED_PYTHON_MINOR}）",
            manifest.python_minor
        );
    }
    if manifest.uv_version != UV_VERSION {
        bail!("uv 版本不符（{} ≠ {UV_VERSION}）", manifest.uv_version);
    }
    let lock_sha = sha256_file(&layout.uv_lock)?;
    if manifest.lock_sha256 != lock_sha {
        bail!(
            "uv.lock 已变化（清单 {} ≠ 当前 {lock_sha}）",
            manifest.lock_sha256
        );
    }
    progress(PythonEnvPhase::Verifying);
    let python_version = backend
        .probe_python(&layout.venv_python)
        .map_err(|e| anyhow!("环境探测失败: {e:#}"))?;
    Ok(PythonEnvInfo {
        python: layout.venv_python.clone(),
        python_version,
        uv_version: manifest.uv_version,
        venv: layout.venv_dir.clone(),
    })
}

/// staging 修复：旧 .venv 全程保留到新环境通过最终探测；最终探测失败时尽量还原。
fn repair(
    layout: &RuntimeLayout,
    backend: &dyn Backend,
    progress: &impl Fn(PythonEnvPhase),
) -> Result<PythonEnvInfo> {
    cleanup_stale_artifacts(layout);
    ensure_uv(layout, backend, progress)?;
    let staging = layout
        .root
        .join(format!(".venv.part-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    let lock_sha = sha256_file(&layout.uv_lock)?;

    let backup = available_backup_path(&layout.root);
    let mut promoted = false;
    let built = (|| -> Result<()> {
        progress(PythonEnvPhase::PreparingPython);
        progress(PythonEnvPhase::SyncingDependencies);
        let plan = build_sync_plan(layout, &staging);
        backend.uv_sync(&plan)?;
        let staging_python = venv_python_path(&staging);
        if !staging_python.exists() {
            bail!("staging 环境缺少解释器（{}）", staging_python.display());
        }
        progress(PythonEnvPhase::Verifying);
        backend
            .probe_python(&staging_python)
            .map_err(|e| anyhow!("staging 环境探测失败: {e:#}"))?;
        write_manifest(&staging, &lock_sha)?;
        swap_install(&layout.venv_dir, &staging, &backup)?;
        promoted = true;
        Ok(())
    })();

    match built {
        Ok(()) => {
            progress(PythonEnvPhase::Verifying);
            match backend.probe_python(&layout.venv_python) {
                Ok(python_version) => {
                    // 新环境完整验证成功，backup（含历史残留）才可清理
                    cleanup_backup_dirs(&layout.root);
                    Ok(PythonEnvInfo {
                        python: layout.venv_python.clone(),
                        python_version,
                        uv_version: UV_VERSION.to_string(),
                        venv: layout.venv_dir.clone(),
                    })
                }
                Err(probe_err) => {
                    // 最终探测失败：新环境不可信，尽量还原旧环境
                    let _ = std::fs::remove_dir_all(&staging);
                    let restored = if promoted {
                        let _ = std::fs::remove_dir_all(&layout.venv_dir);
                        std::fs::rename(&backup, &layout.venv_dir).is_ok()
                    } else {
                        false
                    };
                    if restored {
                        Err(anyhow!("最终环境探测失败（旧环境已还原）: {probe_err:#}"))
                    } else {
                        Err(anyhow!(
                            "最终环境探测失败，旧环境还原失败，backup 保留于 {}: {probe_err:#}",
                            backup.display()
                        ))
                    }
                }
            }
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

/// 旧环境 → backup，staging → 原位；staging 落位失败时回滚并消费 backup。
/// 成功时 backup 留给调用方，在最终探测通过后清理。
fn swap_install(venv_dir: &Path, staging: &Path, backup: &Path) -> Result<()> {
    let had_old = venv_dir.exists();
    if had_old {
        std::fs::rename(venv_dir, backup).with_context(|| {
            format!(
                "旧 .venv 重命名失败（可能被运行中的进程占用）: {} → {}",
                venv_dir.display(),
                backup.display()
            )
        })?;
    }
    if let Err(e) = std::fs::rename(staging, venv_dir) {
        if had_old {
            match std::fs::rename(backup, venv_dir) {
                Ok(()) => return Err(anyhow!(e)).context("staging 落位失败（已回滚旧 .venv）"),
                Err(rb) => {
                    return Err(anyhow!(e)).context(format!(
                        "staging 落位失败且回滚失败，backup 保留于 {}: {rb:#}",
                        backup.display()
                    ));
                }
            }
        }
        return Err(anyhow!(e)).context("staging 落位失败");
    }
    Ok(())
}

/// 为本次交换选择不会覆盖历史恢复点的 backup 路径。
///
/// 进程可能在最终探测前退出，或在回滚时因文件占用留下 backup；Windows PID 也会复用，
/// 因此不能假定 `.venv.backup-<pid>` 一定空闲。
fn available_backup_path(root: &Path) -> PathBuf {
    let base = root.join(format!(".venv.backup-{}", std::process::id()));
    if !base.exists() {
        return base;
    }
    for suffix in 1u64.. {
        let candidate = root.join(format!(".venv.backup-{}-{suffix}", std::process::id()));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("u64 backup suffix space exhausted")
}

/// 获取（必要时下载并校验安装）固定版 uv，返回其可执行文件路径。
fn ensure_uv(
    layout: &RuntimeLayout,
    backend: &dyn Backend,
    progress: &impl Fn(PythonEnvPhase),
) -> Result<PathBuf> {
    if layout.uv_exe.exists() {
        if let Ok(v) = backend.uv_version(&layout.uv_exe) {
            if uv_version_matches(&v, UV_VERSION) {
                return Ok(layout.uv_exe.clone());
            }
        }
        // 版本不符或不可执行：按版本命名的目录整体重建
        let _ = std::fs::remove_dir_all(layout.uv_dir.join(UV_VERSION));
    }
    let urls = uv_download_urls();
    if urls.is_empty() {
        bail!("当前平台没有预配置的 uv 下载资产");
    }
    std::fs::create_dir_all(&layout.uv_dir)
        .with_context(|| format!("创建 {}", layout.uv_dir.display()))?;
    let part = layout.uv_dir.join(format!(".part-{}", std::process::id()));
    progress(PythonEnvPhase::DownloadingUv {
        downloaded: 0,
        total: None,
    });
    let result = backend
        .download_uv_zip(&urls, &part, &mut |downloaded, total| {
            progress(PythonEnvPhase::DownloadingUv { downloaded, total });
        })
        .and_then(|()| {
            install_uv_from_zip(
                &part,
                &layout.uv_dir,
                uv_asset_sha256(),
                UV_VERSION,
                &|exe| backend.uv_version(exe),
            )
        });
    let _ = std::fs::remove_file(&part);
    result?;
    Ok(layout.uv_exe.clone())
}

/// 校验 zip SHA-256 → 仅提取 uv.exe（拒绝路径穿越）→ 验证版本 → 原子落位。
pub(crate) fn install_uv_from_zip(
    zip_path: &Path,
    tools_dir: &Path,
    expected_sha256: &str,
    uv_version: &str,
    run_version: &dyn Fn(&Path) -> Result<String>,
) -> Result<PathBuf> {
    let actual = sha256_file(zip_path)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        bail!("uv 压缩包 SHA-256 校验失败：期望 {expected_sha256}，实际 {actual}");
    }
    let file =
        std::fs::File::open(zip_path).with_context(|| format!("打开 {}", zip_path.display()))?;
    let mut zf = zip::ZipArchive::new(std::io::BufReader::new(file)).context("读取 uv 压缩包")?;
    let staging = tools_dir.join(format!(".stage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).with_context(|| format!("创建 {}", staging.display()))?;
    let extract = (|| -> Result<()> {
        let mut found = false;
        for i in 0..zf.len() {
            let mut entry = zf.by_index(i).with_context(|| format!("读取条目 {i}"))?;
            let name = entry.name().to_string();
            if !is_safe_zip_name(&name) {
                bail!("压缩包含异常路径条目: {name}");
            }
            if entry.is_dir() || name != uv_exe_name() {
                continue;
            }
            let out = staging.join(uv_exe_name());
            let mut of =
                std::fs::File::create(&out).with_context(|| format!("创建 {}", out.display()))?;
            std::io::copy(&mut entry, &mut of).with_context(|| format!("解压 {name}"))?;
            found = true;
        }
        if !found {
            bail!("压缩包中未找到 {}", uv_exe_name());
        }
        let staged_exe = staging.join(uv_exe_name());
        let v = run_version(&staged_exe)?;
        if !uv_version_matches(&v, uv_version) {
            bail!("uv 版本验证失败：期望 {uv_version}，得到 {v}");
        }
        Ok(())
    })();
    match extract {
        Ok(()) => {
            let final_dir = tools_dir.join(uv_version);
            let _ = std::fs::remove_dir_all(&final_dir);
            std::fs::rename(&staging, &final_dir)
                .with_context(|| format!("落位 {}", final_dir.display()))?;
            Ok(final_dir.join(uv_exe_name()))
        }
        Err(e) => {
            let _ = std::fs::remove_dir_all(&staging);
            Err(e)
        }
    }
}

/// 拒绝绝对路径、盘符与 `..` 穿越。
fn is_safe_zip_name(name: &str) -> bool {
    !name.starts_with('/')
        && !name.starts_with('\\')
        && !name.contains(':')
        && !name.split(['/', '\\']).any(|seg| seg == "..")
}

/// `uv --version` 输出形如 "uv 0.12.4 (...)"，第二个字段须精确等于期望版本。
fn uv_version_matches(output: &str, expected: &str) -> bool {
    output.split_whitespace().nth(1) == Some(expected)
}

/// 清理 root 直属的 `.venv.part-*` 与 uv 下载 `.part-*`。
///
/// 这里绝不删除 `.venv.backup-*`：进程可能在新 `.venv` 落位后、最终探测前退出，
/// 此时 `.venv` 虽然存在却不可信，backup 仍是唯一恢复点。backup 只允许在当前环境
/// 通过快速或最终探测后由 [`cleanup_backup_dirs`] 清理。
pub(crate) fn cleanup_stale_artifacts(layout: &RuntimeLayout) {
    let Ok(entries) = std::fs::read_dir(&layout.root) else {
        return;
    };
    for e in entries.filter_map(|e| e.ok()) {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with(".venv.part-") && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
    if let Ok(entries) = std::fs::read_dir(&layout.uv_dir) {
        for e in entries.filter_map(|e| e.ok()) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(".part-") && e.path().is_file() {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// 新环境完整验证成功后调用：清掉全部 `.venv.backup-*`（此时均为过时残留）。
fn cleanup_backup_dirs(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for e in entries.filter_map(|e| e.ok()) {
        if e.file_name().to_string_lossy().starts_with(".venv.backup-") && e.path().is_dir() {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// 环境任务重入护栏：同一时间只允许一次 ensure/repair。
#[derive(Default)]
pub struct EnsureGuard(AtomicBool);

impl EnsureGuard {
    pub fn try_enter(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
    pub fn exit(&self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::fs::File;
    use std::sync::Mutex;

    type DownloadFn = Box<dyn FnMut(&Path, &mut dyn FnMut(u64, Option<u64>)) -> Result<()> + Send>;

    struct TestBackend {
        probe_results: Mutex<VecDeque<Result<String, String>>>,
        uv_version_result: Mutex<Result<String, String>>,
        sync_calls: Mutex<Vec<UvSyncPlan>>,
        sync_results: Mutex<VecDeque<Result<(), String>>>,
        create_venv_on_sync: AtomicBool,
        download: Mutex<Option<DownloadFn>>,
    }

    impl TestBackend {
        fn new() -> Self {
            Self {
                probe_results: Mutex::new(VecDeque::new()),
                uv_version_result: Mutex::new(Ok("uv 0.12.4 (test)".into())),
                sync_calls: Mutex::new(Vec::new()),
                sync_results: Mutex::new(VecDeque::new()),
                create_venv_on_sync: AtomicBool::new(false),
                download: Mutex::new(None),
            }
        }

        fn push_probe_ok(&self, version: &str) {
            self.probe_results
                .lock()
                .unwrap()
                .push_back(Ok(version.into()));
        }
        fn push_probe_err(&self, msg: &str) {
            self.probe_results
                .lock()
                .unwrap()
                .push_back(Err(msg.into()));
        }
        fn sync_creates_venv(&self) {
            self.sync_results.lock().unwrap().push_back(Ok(()));
            self.create_venv_on_sync.store(true, Ordering::SeqCst);
        }
        fn preinstall_uv(&self, layout: &RuntimeLayout) {
            std::fs::create_dir_all(layout.uv_dir.join(UV_VERSION)).unwrap();
            File::create(&layout.uv_exe).unwrap();
        }
    }

    impl Default for TestBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Backend for TestBackend {
        fn download_uv_zip(
            &self,
            _urls: &[String],
            dest: &Path,
            progress: &mut dyn FnMut(u64, Option<u64>),
        ) -> Result<()> {
            let mut dl = self.download.lock().unwrap();
            match dl.as_mut() {
                Some(f) => f(dest, progress),
                None => bail!("unexpected download"),
            }
        }

        fn uv_version(&self, _uv_exe: &Path) -> Result<String> {
            self.uv_version_result
                .lock()
                .unwrap()
                .clone()
                .map_err(|e| anyhow!("{e}"))
        }

        fn probe_python(&self, python: &Path) -> Result<String> {
            assert!(
                python.exists(),
                "probe target must exist: {}",
                python.display()
            );
            self.probe_results
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected probe call")
                .map_err(|e| anyhow!("{e}"))
        }

        fn uv_sync(&self, plan: &UvSyncPlan) -> Result<()> {
            self.sync_calls.lock().unwrap().push(plan.clone());
            let result = self
                .sync_results
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected uv sync call");
            if result.is_ok() && self.create_venv_on_sync.load(Ordering::SeqCst) {
                let py = venv_python_path(&plan.staging_venv);
                std::fs::create_dir_all(py.parent().unwrap()).unwrap();
                File::create(&py).unwrap();
            }
            result.map_err(|e| anyhow!("{e}"))
        }
    }

    struct Fixture {
        _dir: tempfile::TempDir,
        layout: RuntimeLayout,
    }

    fn write_worker_files(root: &Path) {
        std::fs::create_dir_all(root.join("worker")).unwrap();
        for name in WORKER_FILES {
            File::create(root.join("worker").join(name)).unwrap();
        }
    }

    fn fixture_named(name: &str) -> Fixture {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join(name);
        write_worker_files(&root);
        File::create(root.join("pyproject.toml")).unwrap();
        File::create(root.join("uv.lock")).unwrap();
        File::create(root.join("uv.toml")).unwrap();
        let layout =
            RuntimeLayout::resolve(&root, Path::new("D:\\dev\\camouforge\\crates\\app")).unwrap();
        assert_eq!(layout.root, root);
        Fixture { _dir: dir, layout }
    }

    fn fixture() -> Fixture {
        fixture_named("root")
    }

    fn make_installed_env(layout: &RuntimeLayout) {
        std::fs::create_dir_all(venv_python_path(&layout.venv_dir).parent().unwrap()).unwrap();
        File::create(venv_python_path(&layout.venv_dir)).unwrap();
        let lock_sha = sha256_file(&layout.uv_lock).unwrap();
        write_manifest(layout.venv_dir.as_path(), &lock_sha).unwrap();
    }

    #[test]
    fn layout_selects_portable_and_dev_roots() {
        let dir = tempfile::TempDir::new().unwrap();
        let portable_root = dir.path().join("portable");
        write_worker_files(&portable_root);
        File::create(portable_root.join("pyproject.toml")).unwrap();
        File::create(portable_root.join("uv.lock")).unwrap();
        File::create(portable_root.join("uv.toml")).unwrap();
        let layout =
            RuntimeLayout::resolve(&portable_root, Path::new("X:\\elsewhere\\crates\\app"))
                .unwrap();
        assert_eq!(layout.root, portable_root);
        assert_eq!(layout.venv_dir, portable_root.join(".venv"));
        assert_eq!(
            layout.runtime_dir,
            portable_root.join(".camouforge-runtime")
        );
        assert_eq!(
            layout.uv_exe,
            portable_root
                .join(".camouforge-runtime/tools/uv")
                .join(UV_VERSION)
                .join(uv_exe_name())
        );

        let dev_root = dir.path().join("repo");
        write_worker_files(&dev_root);
        File::create(dev_root.join("pyproject.toml")).unwrap();
        File::create(dev_root.join("uv.lock")).unwrap();
        File::create(dev_root.join("uv.toml")).unwrap();
        let manifest_dir = dev_root.join("crates").join("app");
        std::fs::create_dir_all(&manifest_dir).unwrap();
        let layout = RuntimeLayout::resolve(&dir.path().join("exe-dir"), &manifest_dir).unwrap();
        assert_eq!(layout.root, dev_root);
        assert_eq!(
            layout.worker_script,
            dev_root.join("worker").join("camoforge_worker.py")
        );
    }

    #[test]
    fn portable_missing_manifest_never_falls_back_to_dev() {
        let dir = tempfile::TempDir::new().unwrap();
        let portable_root = dir.path().join("portable");
        std::fs::create_dir_all(portable_root.join("worker")).unwrap();
        File::create(portable_root.join("worker").join("camoforge_worker.py")).unwrap();
        let err = RuntimeLayout::resolve(
            &portable_root,
            Path::new("D:\\dev\\camouforge\\crates\\app"),
        )
        .unwrap_err();
        let msg = format!("{err:#}");
        assert!(msg.contains("发布包不完整"), "{msg}");
        assert!(msg.contains("browser_patches.py"), "{msg}");
        assert!(msg.contains("uv.lock"), "{msg}");
        assert!(msg.contains("便携目录"), "{msg}");
        assert!(!msg.contains("开发目录"), "{msg}");
        assert!(!msg.contains("crates/app"), "{msg}");
    }

    #[test]
    fn valid_env_fully_skips_uv_and_network() {
        let f = fixture();
        make_installed_env(&f.layout);
        let historical_backup = f.layout.root.join(".venv.backup-old");
        std::fs::create_dir_all(&historical_backup).unwrap();
        let b = TestBackend::new();
        b.push_probe_ok("3.12.7");
        let saw_verifying = std::sync::atomic::AtomicBool::new(false);
        let info = ensure_python_env_with(
            &f.layout,
            EnsureMode::IfNeeded,
            |p| {
                if p == PythonEnvPhase::Verifying {
                    saw_verifying.store(true, std::sync::atomic::Ordering::SeqCst);
                }
            },
            &b,
        )
        .unwrap();
        assert!(saw_verifying.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(info.python_version, "3.12.7");
        assert_eq!(info.python, f.layout.venv_python);
        assert!(
            b.sync_calls.lock().unwrap().is_empty(),
            "fast path must not run uv sync"
        );
        assert!(
            b.download.lock().unwrap().is_none(),
            "fast path must not download"
        );
        assert!(
            !historical_backup.exists(),
            "verified fast path may clean historical backups"
        );
    }

    /// 锁文件 SHA 变化 / Python 不可执行 / 发行版版本错误 / import 失败：
    /// 四种失效都必须触发 uv sync 重建，且重建后 .venv 可用、无 staging 残留。
    #[test]
    fn invalid_envs_trigger_rebuild() {
        let cases = [
            ("lock-sha-changed", "uv.lock 内容变化"),
            ("python-not-executable", "探测进程崩溃"),
            ("dist-version-wrong", "camoufox 1.2.4 != 0.5.5"),
            ("import-failure", "ImportError: No module named camoufox"),
        ];
        for (case, probe_err) in cases {
            let f = fixture_named(&format!("root-{case}"));
            make_installed_env(&f.layout);
            let b = TestBackend::new();
            b.preinstall_uv(&f.layout);
            match case {
                "lock-sha-changed" => {
                    let mut l = File::options()
                        .append(true)
                        .open(&f.layout.uv_lock)
                        .unwrap();
                    writeln!(l, "# changed").unwrap();
                }
                _ => b.push_probe_err(probe_err),
            }
            b.sync_creates_venv();
            b.push_probe_ok("3.12.8"); // staging 探测
            b.push_probe_ok("3.12.8"); // 最终探测
            let info = ensure_python_env_with(&f.layout, EnsureMode::IfNeeded, |_| {}, &b)
                .unwrap_or_else(|e| panic!("{case}: {e:#}"));
            assert_eq!(info.python_version, "3.12.8", "{case}");
            let calls = b.sync_calls.lock().unwrap();
            assert_eq!(calls.len(), 1, "{case}");
            assert!(
                calls[0]
                    .staging_venv
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".venv.part-"),
                "{case}"
            );
            assert!(
                !calls[0].staging_venv.exists(),
                "{case}: staging must be promoted away"
            );
            drop(calls);
            assert!(f.layout.venv_python.exists(), "{case}");
            assert!(f.layout.venv_dir.join(MANIFEST_NAME).exists(), "{case}");
        }
    }

    #[test]
    fn rebuild_uses_exact_sync_plan() {
        let f = fixture();
        make_installed_env(&f.layout);
        let b = TestBackend::new();
        b.preinstall_uv(&f.layout);
        b.push_probe_err("python dead");
        b.sync_creates_venv();
        b.push_probe_ok("3.12.8");
        b.push_probe_ok("3.12.8");
        ensure_python_env_with(&f.layout, EnsureMode::IfNeeded, |_| {}, &b).unwrap();
        let calls = b.sync_calls.lock().unwrap();
        let plan = &calls[0];
        assert_eq!(plan.uv_exe, f.layout.uv_exe);
        assert_eq!(plan.args[0], "sync");
        assert_eq!(plan.args[1], "--project");
        assert_eq!(plan.args[2], f.layout.root.to_string_lossy());
        assert_eq!(plan.args[3], "--config-file");
        assert_eq!(plan.args[4], f.layout.uv_config.to_string_lossy());
        let expected_tail = [
            "--frozen",
            "--no-dev",
            "--no-install-project",
            "--managed-python",
            "--python",
            "3.12",
            "--link-mode",
            "copy",
            "--system-certs",
            "--no-progress",
        ];
        let tail_starts = plan.args.len() - expected_tail.len();
        assert_eq!(&plan.args[tail_starts..], expected_tail, "{:?}", plan.args);
        for key in [
            "VIRTUAL_ENV",
            "CONDA_PREFIX",
            "UV_PROJECT",
            "UV_PROJECT_ENVIRONMENT",
            "UV_PYTHON",
            "UV_PYTHON_INSTALL_DIR",
            "UV_CACHE_DIR",
            "UV_OFFLINE",
        ] {
            assert!(
                plan.env_remove.iter().any(|k| k == key),
                "missing env_remove {key}"
            );
        }
        let env: std::collections::HashMap<String, String> = plan.env_set.iter().cloned().collect();
        assert_eq!(
            env["UV_PROJECT_ENVIRONMENT"].as_str(),
            plan.staging_venv.to_string_lossy()
        );
        assert_eq!(
            env["UV_PYTHON_INSTALL_DIR"].as_str(),
            f.layout.managed_python_dir.to_string_lossy()
        );
        assert_eq!(
            env["UV_CACHE_DIR"].as_str(),
            f.layout.uv_cache_dir.to_string_lossy()
        );
    }

    #[test]
    fn uv_zip_sha_mismatch_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let tools = dir.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let zip = dir.path().join("uv.zip");
        File::create(&zip).unwrap().write_all(b"not a zip").unwrap();
        let err = install_uv_from_zip(&zip, &tools, "deadbeef", UV_VERSION, &|_| {
            Ok("uv 0.12.4 (x)".into())
        })
        .unwrap_err();
        assert!(format!("{err:#}").contains("SHA-256"), "{err:#}");
        assert!(
            tools.read_dir().unwrap().next().is_none(),
            "nothing may be installed"
        );
    }

    #[test]
    fn uv_zip_path_traversal_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let tools = dir.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let zip_path = dir.path().join("evil.zip");
        {
            let f = File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file("../evil.txt", opts).unwrap();
            w.write_all(b"evil").unwrap();
            w.start_file("uv.exe", opts).unwrap();
            w.write_all(b"uv").unwrap();
            w.finish().unwrap();
        }
        let sha = sha256_file(&zip_path).unwrap();
        let err = install_uv_from_zip(&zip_path, &tools, &sha, UV_VERSION, &|_| {
            Ok(format!("uv {UV_VERSION} (x)"))
        })
        .unwrap_err();
        assert!(format!("{err:#}").contains("异常路径"), "{err:#}");
        assert!(!dir.path().join("evil.txt").exists());
        assert!(!tools.join(UV_VERSION).exists());
    }

    #[test]
    fn uv_zip_without_exe_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        let tools = dir.path().join("tools");
        std::fs::create_dir_all(&tools).unwrap();
        let zip_path = dir.path().join("noexe.zip");
        {
            let f = File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            w.start_file("readme.txt", zip::write::SimpleFileOptions::default())
                .unwrap();
            w.write_all(b"hi").unwrap();
            w.finish().unwrap();
        }
        let sha = sha256_file(&zip_path).unwrap();
        let err = install_uv_from_zip(&zip_path, &tools, &sha, UV_VERSION, &|_| {
            Ok(format!("uv {UV_VERSION}"))
        })
        .unwrap_err();
        assert!(format!("{err:#}").contains("未找到"), "{err:#}");
    }

    #[test]
    fn interrupted_download_leaves_cleanable_part_only() {
        let f = fixture();
        let b = TestBackend::new();
        *b.download.lock().unwrap() = Some(Box::new(
            |dest: &Path, progress: &mut dyn FnMut(u64, Option<u64>)| {
                std::fs::create_dir_all(dest.parent().unwrap()).unwrap();
                let mut file = File::create(dest).unwrap();
                file.write_all(&vec![0u8; 1024]).unwrap();
                progress(1024, None);
                bail!("connection reset");
            },
        ));
        let err =
            ensure_python_env_with(&f.layout, EnsureMode::ForceRepair, |_| {}, &b).unwrap_err();
        assert!(format!("{err:#}").contains("connection reset"), "{err:#}");
        assert!(
            !f.layout.uv_dir.join(UV_VERSION).exists(),
            "no uv may be installed"
        );
        std::fs::create_dir_all(&f.layout.uv_dir).unwrap();
        let leftover = f.layout.uv_dir.join(".part-424242");
        File::create(&leftover).unwrap();
        cleanup_stale_artifacts(&f.layout);
        assert!(!leftover.exists());
    }

    #[test]
    fn cleanup_removes_only_parts_and_preserves_backups() {
        let f = fixture();
        let part = f.layout.root.join(".venv.part-1");
        let backup = f.layout.root.join(".venv.backup-2");
        let keep = f.layout.root.join(".venv");
        let outside = tempfile::TempDir::new().unwrap();
        let outside_part = outside.path().join(".venv.part-3");
        for d in [&part, &backup, &keep, &outside_part] {
            std::fs::create_dir_all(d).unwrap();
        }
        File::create(keep.join("marker")).unwrap();
        File::create(outside_part.join("marker")).unwrap();
        cleanup_stale_artifacts(&f.layout);
        assert!(!part.exists());
        assert!(
            backup.exists(),
            "pre-repair cleanup must never delete a recovery point"
        );
        assert!(
            keep.exists() && keep.join("marker").exists(),
            "real .venv must be untouched"
        );
        assert!(outside_part.exists(), "outside root must be untouched");
    }

    #[test]
    fn cleanup_keeps_sole_backup_when_venv_missing() {
        let f = fixture_named("root-no-venv");
        let backup = f.layout.root.join(".venv.backup-3");
        let part = f.layout.root.join(".venv.part-3");
        std::fs::create_dir_all(backup.join("Scripts")).unwrap();
        std::fs::create_dir_all(&part).unwrap();
        File::create(backup.join("Scripts").join("python.exe")).unwrap();

        cleanup_stale_artifacts(&f.layout);

        assert!(!part.exists(), "staging parts always cleaned");
        assert!(
            backup.join("Scripts").join("python.exe").exists(),
            ".venv 缺失时 backup 是唯一恢复点，必须保留"
        );
    }

    #[test]
    fn cleanup_keeps_backup_when_unverified_venv_exists() {
        let f = fixture();
        let backup = f.layout.root.join(".venv.backup-crash");
        let part = f.layout.root.join(".venv.part-crash");
        std::fs::create_dir_all(&f.layout.venv_dir).unwrap();
        std::fs::create_dir_all(&backup).unwrap();
        std::fs::create_dir_all(&part).unwrap();
        File::create(f.layout.venv_dir.join("new-but-unverified")).unwrap();
        File::create(backup.join("known-old")).unwrap();

        cleanup_stale_artifacts(&f.layout);

        assert!(!part.exists(), "staging parts are never recovery points");
        assert!(f.layout.venv_dir.join("new-but-unverified").exists());
        assert!(
            backup.join("known-old").exists(),
            "an existing .venv may be the unverified result of an interrupted promotion"
        );
    }

    #[test]
    fn backup_path_never_reuses_an_existing_recovery_point() {
        let f = fixture();
        let base = f
            .layout
            .root
            .join(format!(".venv.backup-{}", std::process::id()));
        let first_suffix = f
            .layout
            .root
            .join(format!(".venv.backup-{}-1", std::process::id()));
        std::fs::create_dir_all(&base).unwrap();
        std::fs::create_dir_all(&first_suffix).unwrap();

        assert_eq!(
            available_backup_path(&f.layout.root),
            f.layout
                .root
                .join(format!(".venv.backup-{}-2", std::process::id()))
        );
        assert!(base.exists() && first_suffix.exists());
    }

    /// 最终探测失败：新环境不可信，必须还原旧环境并消费 backup。
    #[test]
    fn final_probe_failure_restores_old_venv() {
        let f = fixture();
        make_installed_env(&f.layout);
        File::create(f.layout.venv_dir.join("marker-old")).unwrap();
        let b = TestBackend::new();
        b.preinstall_uv(&f.layout);
        b.push_probe_err("python dead"); // 快速校验失败 → 修复
        b.sync_creates_venv();
        b.push_probe_ok("3.12.8"); // staging 探测
        b.push_probe_err("final import broken"); // 最终探测失败
        let err = ensure_python_env_with(&f.layout, EnsureMode::IfNeeded, |_| {}, &b).unwrap_err();
        assert!(format!("{err:#}").contains("旧环境已还原"), "{err:#}");
        assert!(
            f.layout.venv_dir.join("marker-old").exists(),
            "old .venv restored after final probe failure"
        );
        assert!(
            !f.layout
                .root
                .join(format!(".venv.backup-{}", std::process::id()))
                .exists(),
            "backup consumed by restore"
        );
    }

    #[test]
    fn sync_failure_keeps_old_venv() {
        let f = fixture();
        make_installed_env(&f.layout);
        File::create(f.layout.venv_dir.join("marker-old")).unwrap();
        let b = TestBackend::new();
        b.preinstall_uv(&f.layout);
        b.push_probe_err("python dead"); // 快速校验失败 → 修复
        b.sync_results
            .lock()
            .unwrap()
            .push_back(Err("disk full".into()));
        let err = ensure_python_env_with(&f.layout, EnsureMode::IfNeeded, |_| {}, &b).unwrap_err();
        assert!(format!("{err:#}").contains("disk full"), "{err:#}");
        assert!(
            f.layout.venv_dir.join("marker-old").exists(),
            "old .venv must survive"
        );
        assert!(
            f.layout.venv_dir.join(MANIFEST_NAME).exists(),
            "old manifest intact"
        );
        assert!(
            std::fs::read_dir(&f.layout.root)
                .unwrap()
                .filter_map(|e| e.ok())
                .all(|e| !e.file_name().to_string_lossy().starts_with(".venv.part-")),
            "no staging leftover"
        );
    }

    #[test]
    fn failed_final_rename_rolls_back_backup() {
        let dir = tempfile::TempDir::new().unwrap();
        let venv = dir.path().join(".venv");
        let staging = dir.path().join(".venv.part-9");
        let backup = dir.path().join(".venv.backup-9");
        std::fs::create_dir_all(&venv).unwrap();
        File::create(venv.join("old-marker")).unwrap();
        let err = swap_install(&venv, &staging, &backup).unwrap_err();
        assert!(format!("{err:#}").contains("staging 落位失败"), "{err:#}");
        assert!(venv.join("old-marker").exists(), "old .venv restored");
        assert!(!backup.exists(), "backup consumed by rollback");
    }

    #[test]
    fn ensure_guard_blocks_reentry() {
        let guard = EnsureGuard::default();
        assert!(guard.try_enter());
        assert!(
            !guard.try_enter(),
            "second concurrent entry must be refused"
        );
        guard.exit();
        assert!(guard.try_enter());
    }

    #[test]
    fn sync_plan_paths_survive_spaces_and_unicode() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("my app 目录");
        write_worker_files(&root);
        File::create(root.join("pyproject.toml")).unwrap();
        File::create(root.join("uv.lock")).unwrap();
        File::create(root.join("uv.toml")).unwrap();
        let layout =
            RuntimeLayout::resolve(&root, Path::new("D:\\dev\\camouforge\\crates\\app")).unwrap();
        let staging = root.join(".venv.part-7");
        let plan = build_sync_plan(&layout, &staging);
        assert_eq!(plan.args[2], root.to_string_lossy());
        assert_eq!(plan.env_set[0].1, staging.to_string_lossy());
    }

    /// 真实网络全链路：固定版 uv 下载 + 托管 Python + uv sync + 探测 + worker ping。
    /// 只在 TempDir 中搭建运行目录，绝不触碰仓库现有 .venv。
    #[test]
    #[ignore]
    fn bootstrap_live() {
        let dir = tempfile::TempDir::new().unwrap();
        let root = dir.path().join("portable-root");
        std::fs::create_dir_all(root.join("worker")).unwrap();
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        for name in WORKER_FILES {
            std::fs::copy(
                repo.join("worker").join(name),
                root.join("worker").join(name),
            )
            .unwrap();
        }
        std::fs::copy(repo.join("pyproject.toml"), root.join("pyproject.toml")).unwrap();
        std::fs::copy(repo.join("uv.lock"), root.join("uv.lock")).unwrap();
        std::fs::copy(repo.join("uv.toml"), root.join("uv.toml")).unwrap();
        let layout = RuntimeLayout::resolve(&root, Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
        let t0 = Instant::now();
        let info = ensure_python_env(&layout, EnsureMode::IfNeeded, |p| {
            println!("[{:>6.1}s] {}", t0.elapsed().as_secs_f32(), p.label());
        })
        .expect("bootstrap failed");
        println!("python: {}", info.python.display());
        println!("python_version: {}", info.python_version);
        assert!(info.python_version.starts_with("3.12."));
        let info2 =
            ensure_python_env(&layout, EnsureMode::IfNeeded, |_| {}).expect("second ensure failed");
        assert_eq!(info2.python_version, info.python_version);
        let data_dir = dir.path().join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let (sup, _rx) = crate::supervisor::WorkerSupervisor::new(
            info.python.to_string_lossy().into_owned(),
            layout.worker_script.clone(),
            data_dir.clone(),
            crate::logging::LogSink::new(data_dir.join("logs")),
        );
        sup.start().expect("worker start");
        let pong = sup.ping().expect("worker ping");
        println!("worker ping: {pong}");
        assert!(pong.get("worker").is_some() || pong.get("camoufox").is_some());
        sup.shutdown();
    }
}
