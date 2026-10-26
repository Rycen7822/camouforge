//! camoufox 官方 release 的发现与下载。
//!
//! 版本目录布局：`<root>/<version-build>/`（目录名即版本全串），内含 camoufox.exe
//! 与 version.json（与 SDK multiversion 同 schema）。release 列表来自 GitHub API
//! `repos/daijro/camoufox/releases`，资产 pattern 与 SDK repos.yml 一致。

use anyhow::{anyhow, Context, Result};
use serde_json::{json, Value};
use std::io::{BufWriter, Read, Seek, SeekFrom, Write as _};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};

const RELEASES_API: &str = "https://api.github.com/repos/daijro/camoufox/releases?per_page=100";
/// 进度节流：变化 ≥1MB 才发一条，避免无意义的高频 UI 更新。
const PROGRESS_CHUNK: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub struct Release {
    pub full: String,
    pub version: String,
    pub build: String,
    pub asset_url: String,
    pub size: u64,
    pub prerelease: bool,
}

#[derive(Debug, Clone)]
pub enum DownloadMsg {
    Progress {
        full: String,
        downloaded: u64,
        total: u64,
    },
    Extracting {
        full: String,
    },
    Done {
        full: String,
    },
    Failed {
        full: String,
        error: String,
    },
    Cancelled {
        full: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum DownloadCommand {
    #[default]
    Running,
    Paused,
    Cancelled,
}

#[derive(Debug, Default)]
pub struct DownloadControl {
    command: Mutex<DownloadCommand>,
    changed: Condvar,
}

impl DownloadControl {
    pub fn toggle_pause(&self) {
        let mut command = self.command.lock().unwrap();
        *command = match *command {
            DownloadCommand::Running => DownloadCommand::Paused,
            DownloadCommand::Paused => DownloadCommand::Running,
            DownloadCommand::Cancelled => DownloadCommand::Cancelled,
        };
        self.changed.notify_all();
    }

    pub fn cancel(&self) {
        *self.command.lock().unwrap() = DownloadCommand::Cancelled;
        self.changed.notify_all();
    }

    pub fn is_paused(&self) -> bool {
        *self.command.lock().unwrap() == DownloadCommand::Paused
    }

    pub fn is_cancelled(&self) -> bool {
        *self.command.lock().unwrap() == DownloadCommand::Cancelled
    }

    fn checkpoint(&self) -> Result<()> {
        let mut command = self.command.lock().unwrap();
        while *command == DownloadCommand::Paused {
            command = self.changed.wait(command).unwrap();
        }
        anyhow::ensure!(*command != DownloadCommand::Cancelled, "下载已取消");
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadPhase {
    Downloading,
    Extracting,
}

#[derive(Debug, Clone)]
pub struct DownloadState {
    pub downloaded: u64,
    pub total: u64,
    pub phase: DownloadPhase,
    pub control: Arc<DownloadControl>,
}

impl DownloadState {
    pub fn percent(&self) -> f32 {
        if self.total == 0 {
            0.0
        } else {
            (self.downloaded as f64 / self.total as f64 * 100.0).min(100.0) as f32
        }
    }
}

/// 浏览器版本根目录：exe 同级 `camoufox/`，跟 exe 走的便携布局。
pub fn releases_root() -> PathBuf {
    let base = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("camoufox")
}

pub fn exe_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "camoufox.exe"
    } else {
        "camoufox"
    }
}

/// 资产文件名 → (version, build)。官方 pattern：`camoufox-{version}-{build}-win.x86_64.zip`。
fn parse_asset(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix("camoufox-")?;
    let rest = rest.strip_suffix("-win.x86_64.zip")?;
    let (version, build) = rest.split_once('-')?;
    if version.is_empty() || build.is_empty() {
        return None;
    }
    Some((version.to_string(), build.to_string()))
}

/// 版本排序键（降序用）：数值化版本段 + build 号。
pub fn version_key(full: &str) -> (Vec<u64>, u64) {
    let (v, b) = full.split_once('-').unwrap_or((full, ""));
    let nums: Vec<u64> = v.split('.').filter_map(|p| p.parse().ok()).collect();
    let build_num: u64 = b
        .rsplit('.')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0);
    (nums, build_num)
}

/// 拉取官方 release 列表（适用于 win.x86_64），新版本在前、同版本去重。
pub fn fetch_releases() -> Result<Vec<Release>> {
    let agent = crate::net::http_agent()?;
    let resp = agent
        .get(RELEASES_API)
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| anyhow!("GitHub API 请求失败: {e}"))?;
    let releases: Value =
        serde_json::from_reader(resp.into_reader()).context("GitHub API 响应解析失败")?;
    let arr = releases
        .as_array()
        .ok_or_else(|| anyhow!("GitHub API 返回非列表（可能被限流）"))?;

    let mut out: Vec<Release> = Vec::new();
    for rel in arr {
        let prerelease = rel
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let Some(assets) = rel.get("assets").and_then(Value::as_array) else {
            continue;
        };
        for asset in assets {
            let Some(name) = asset.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some((version, build)) = parse_asset(name) else {
                continue;
            };
            let full = format!("{version}-{build}");
            if out.iter().any(|r| r.full == full) {
                continue;
            }
            out.push(Release {
                full,
                version,
                build,
                asset_url: asset
                    .get("browser_download_url")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                size: asset.get("size").and_then(Value::as_u64).unwrap_or(0),
                prerelease,
            });
        }
    }
    out.retain(|r| !r.asset_url.is_empty());
    out.sort_by_key(|r| std::cmp::Reverse(version_key(&r.full)));
    Ok(out)
}

/// 清理旧进程遗留的 `.part-*` 目录，保留当前进程正在使用的目录。
pub fn cleanup_part_dirs(root: &Path) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    let active_suffix = format!("-{}", std::process::id());
    for e in entries.filter_map(|e| e.ok()) {
        let name = e.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(".part-") && !name.ends_with(&active_suffix) {
            let _ = std::fs::remove_dir_all(e.path());
        }
    }
}

/// 扫描已安装版本：`<root>/<目录>/camoufox.exe` 存在即算，版本号降序。
pub fn scan_installed(root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| !name.starts_with('.') && root.join(name).join(exe_name()).exists())
        .collect();
    out.sort_by_key(|b| std::cmp::Reverse(version_key(b)));
    out
}

fn installed_version_dir(root: &Path, full: &str) -> Result<PathBuf> {
    use std::path::Component;

    let mut parts = Path::new(full).components();
    anyhow::ensure!(
        root.is_absolute()
            && !full.starts_with('.')
            && matches!(parts.next(), Some(Component::Normal(_)))
            && parts.next().is_none(),
        "无效的浏览器版本目录"
    );
    let directory = root.join(full);
    let metadata = std::fs::symlink_metadata(&directory).context("读取浏览器目录")?;
    anyhow::ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "浏览器目录不能是链接"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        anyhow::ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "浏览器目录不能是目录联接"
        );
    }
    anyhow::ensure!(
        directory.canonicalize()?.parent() == Some(root.canonicalize()?.as_path())
            && directory.join(exe_name()).is_file(),
        "仅可删除浏览器管理目录中的已安装版本"
    );
    Ok(directory)
}

pub fn remove_installed(root: &Path, full: &str) -> Result<()> {
    let directory = installed_version_dir(root, full)?;
    std::fs::remove_dir_all(&directory)
        .with_context(|| format!("删除 {} 失败，请确认浏览器已关闭", directory.display()))
}

/// 下载并安装一个 release 到 `<root>/<full>/`；失败时已清理临时目录，
/// 不产生半成品版本目录。
pub fn download_release(
    rel: &Release,
    root: &Path,
    tx: &async_channel::Sender<DownloadMsg>,
    control: &DownloadControl,
) -> Result<()> {
    if rel.asset_url.is_empty() {
        return Err(anyhow!("该版本缺少下载地址"));
    }
    let final_dir = root.join(&rel.full);
    if final_dir.join(exe_name()).exists() {
        return Ok(());
    }
    std::fs::create_dir_all(root).with_context(|| format!("创建目录 {}", root.display()))?;

    let part = root.join(format!(".part-{}-{}", rel.full, std::process::id()));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).with_context(|| format!("创建临时目录 {}", part.display()))?;
    let result = download_and_extract(rel, root, &part, tx, control);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&part);
    }
    result
}

fn download_and_extract(
    rel: &Release,
    root: &Path,
    part: &Path,
    tx: &async_channel::Sender<DownloadMsg>,
    control: &DownloadControl,
) -> Result<()> {
    let agent = crate::net::http_agent()?;
    let report_progress = |downloaded, total| {
        let _ = tx.send_blocking(DownloadMsg::Progress {
            full: rel.full.clone(),
            downloaded,
            total,
        });
    };
    let zip_path = part.join("camoufox.zip");
    let file =
        std::fs::File::create(&zip_path).with_context(|| format!("创建 {}", zip_path.display()))?;
    let mut writer = BufWriter::new(file);
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_sent: u64 = 0;
    let mut total = rel.size;
    'transfer: loop {
        control.checkpoint()?;
        if downloaded > 0 && total > 0 && downloaded == total {
            break;
        }
        let mut request = agent.get(&rel.asset_url);
        if downloaded > 0 {
            request = request.set("Range", &format!("bytes={downloaded}-"));
        }
        let resp = request.call().map_err(|e| anyhow!("下载请求失败: {e}"))?;
        if downloaded > 0 {
            if resp.status() == 206 {
                anyhow::ensure!(
                    resp.header("Content-Range")
                        .is_some_and(|range| range.starts_with(&format!("bytes {downloaded}-"))),
                    "服务器返回的续传位置不正确"
                );
            } else {
                // 不支持 Range 时重新下载，不能把整包追加到已有内容。
                writer.get_mut().set_len(0)?;
                writer.seek(SeekFrom::Start(0))?;
                downloaded = 0;
                last_sent = 0;
            }
        }
        total = if resp.status() == 206 {
            resp.header("Content-Range")
                .and_then(|value| value.rsplit('/').next())
                .and_then(|value| value.parse().ok())
        } else {
            resp.header("Content-Length")
                .and_then(|value| value.parse().ok())
        }
        .unwrap_or(rel.size);
        report_progress(downloaded, total);
        let mut reader = resp.into_reader();
        loop {
            if control.is_paused() {
                writer.flush()?;
                break;
            }
            anyhow::ensure!(!control.is_cancelled(), "下载已取消");
            let n = reader
                .read(&mut buf)
                .map_err(|e| anyhow!("下载中断: {e}"))?;
            if n == 0 {
                anyhow::ensure!(total == 0 || downloaded == total, "下载文件不完整");
                break 'transfer;
            }
            writer
                .write_all(&buf[..n])
                .with_context(|| format!("写入 {}", zip_path.display()))?;
            downloaded += n as u64;
            if downloaded - last_sent >= PROGRESS_CHUNK {
                report_progress(downloaded, total);
                last_sent = downloaded;
            }
        }
    }
    writer.flush().context("flush 下载文件")?;
    drop(writer);
    report_progress(downloaded, total);

    let unpacked = part.join("unpacked");
    std::fs::create_dir_all(&unpacked).context("创建解压目录")?;
    let _ = tx.send_blocking(DownloadMsg::Extracting {
        full: rel.full.clone(),
    });
    extract_zip(&zip_path, &unpacked, control)?;

    if !unpacked.join(exe_name()).exists() {
        return Err(anyhow!("解压结果缺少 {}，资产结构与预期不符", exe_name()));
    }

    let meta = json!({
        "version": rel.version,
        "build": rel.build,
        "prerelease": rel.prerelease,
        "installed_by": "camoforge",
    });
    std::fs::write(
        unpacked.join("version.json"),
        serde_json::to_vec_pretty(&meta).context("序列化 version.json")?,
    )
    .context("写入 version.json")?;

    control.checkpoint()?;
    let final_dir = root.join(&rel.full);
    if final_dir.exists() {
        if final_dir.join(exe_name()).exists() {
            let _ = std::fs::remove_dir_all(part);
            return Ok(());
        }
        std::fs::remove_dir_all(&final_dir)
            .with_context(|| format!("移除不完整目录 {}", final_dir.display()))?;
    }
    std::fs::rename(&unpacked, &final_dir)
        .with_context(|| format!("移动 {} → {}", unpacked.display(), final_dir.display()))?;
    let _ = std::fs::remove_dir_all(part);
    Ok(())
}

/// 解压 zip 到 dest。若全部条目位于同一顶层目录（部分打包方式），剥离该目录层级。
fn extract_zip(zip_path: &Path, dest: &Path, control: &DownloadControl) -> Result<()> {
    let file =
        std::fs::File::open(zip_path).with_context(|| format!("打开 {}", zip_path.display()))?;
    let mut zf = zip::ZipArchive::new(std::io::BufReader::new(file))
        .with_context(|| format!("读取 zip {}", zip_path.display()))?;

    let names: Vec<String> = (0..zf.len())
        .filter_map(|i| zf.by_index_raw(i).ok().map(|f| f.name().to_string()))
        .collect();
    let skip_components = common_root_depth(&names);

    for i in 0..zf.len() {
        control.checkpoint()?;
        let mut entry = zf.by_index(i).with_context(|| format!("读取条目 {i}"))?;
        // enclosed_name 拒绝绝对路径与 .. 穿越（zip slip）
        let Some(enc) = entry.enclosed_name() else {
            continue;
        };
        let rel: PathBuf = enc.components().skip(skip_components).collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).context("创建解压子目录")?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).context("创建解压父目录")?;
        }
        let mut of =
            std::fs::File::create(&out).with_context(|| format!("创建 {}", out.display()))?;
        let mut buf = [0u8; 64 * 1024];
        loop {
            control.checkpoint()?;
            let count = entry
                .read(&mut buf)
                .with_context(|| format!("解压 {}", entry.name()))?;
            if count == 0 {
                break;
            }
            of.write_all(&buf[..count])?;
        }
    }
    Ok(())
}

/// 全部条目共享同一顶层目录时返回 1（解压时跳过），否则 0。
fn common_root_depth(names: &[String]) -> usize {
    let mut first: Option<&str> = None;
    for n in names {
        let seg = n.split('/').next().unwrap_or("");
        if seg.is_empty() {
            return 0;
        }
        match first {
            None => first = Some(seg),
            Some(f) if f != seg => return 0,
            _ => {}
        }
    }
    match first {
        Some(f) if names.iter().all(|n| n.starts_with(&format!("{f}/"))) => 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_pause_resume_and_cancel() {
        use std::io::{BufRead, Cursor};
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::time::Duration;

        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file(
            exe_name(),
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .unwrap();
        zip.write_all(&vec![b'x'; 2 * PROGRESS_CHUNK as usize])
            .unwrap();
        let data = zip.finish().unwrap().into_inner();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let (pause_tx, pause_rx) = mpsc::channel();
        let (closed_tx, closed_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            for round in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut headers = String::new();
                let mut reader = std::io::BufReader::new(&mut stream);
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" {
                        break;
                    }
                    headers.push_str(&line);
                }
                let offset = if round == 0 {
                    0
                } else {
                    PROGRESS_CHUNK as usize + 1
                };
                if round == 1 {
                    assert!(headers
                        .to_ascii_lowercase()
                        .contains(&format!("range: bytes={offset}-")));
                }
                let status = if round == 0 {
                    "200 OK"
                } else {
                    "206 Partial Content"
                };
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nContent-Range: bytes {offset}-{}/{}\r\nConnection: close\r\n\r\n", data.len() - offset, data.len() - 1, data.len()).unwrap();
                if round == 0 {
                    stream.write_all(&data[..PROGRESS_CHUNK as usize]).unwrap();
                    pause_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                    stream
                        .write_all(&data[PROGRESS_CHUNK as usize..PROGRESS_CHUNK as usize + 1])
                        .unwrap();
                    assert_eq!(stream.read(&mut [0u8; 1]).unwrap(), 0);
                    closed_tx.send(()).unwrap();
                } else {
                    stream.write_all(&data[offset..]).unwrap();
                }
            }
        });
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let release = Release {
            full: "158.0-beta.40".into(),
            version: "158.0".into(),
            build: "beta.40".into(),
            asset_url: format!("http://{address}/browser.zip"),
            size: 0,
            prerelease: true,
        };
        let control = Arc::new(DownloadControl::default());
        let worker_control = control.clone();
        let worker_root = root.clone();
        let worker_release = release.clone();
        let (worker_tx, rx) = async_channel::unbounded();
        let worker = std::thread::spawn(move || {
            download_release(&worker_release, &worker_root, &worker_tx, &worker_control).unwrap();
        });
        loop {
            if let DownloadMsg::Progress { downloaded, .. } = rx.recv_blocking().unwrap() {
                if downloaded >= PROGRESS_CHUNK {
                    break;
                }
            }
        }
        control.toggle_pause();
        assert!(control.is_paused());
        pause_tx.send(()).unwrap();
        closed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        control.toggle_pause();
        worker.join().unwrap();
        server.join().unwrap();
        assert_eq!(
            std::fs::metadata(root.join(&release.full).join(exe_name()))
                .unwrap()
                .len(),
            2 * PROGRESS_CHUNK
        );
        control.toggle_pause();
        control.cancel();
        assert!(control.checkpoint().is_err());
        control.toggle_pause();
        assert!(control.is_cancelled());
        let cancelled = Release {
            full: "158.0-beta.41".into(),
            ..release
        };
        let (tx, _rx) = async_channel::unbounded();
        assert!(download_release(&cancelled, &root, &tx, &control).is_err());
        assert!(!root
            .join(format!(".part-{}-{}", cancelled.full, std::process::id()))
            .exists());
        assert!(!root.join(&cancelled.full).exists());
    }

    #[test]
    fn browser_removal_is_limited_to_installed_child_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let version = tmp.path().join("158.0-beta.40");
        std::fs::create_dir(&version).unwrap();
        std::fs::write(version.join(exe_name()), b"browser").unwrap();
        assert_eq!(
            installed_version_dir(tmp.path(), "158.0-beta.40").unwrap(),
            version
        );
        assert!(installed_version_dir(tmp.path(), "..").is_err());
        assert!(installed_version_dir(tmp.path(), "../profiles").is_err());
        assert!(installed_version_dir(tmp.path(), "missing-version").is_err());
        assert!(version.join(exe_name()).is_file());
    }

    #[test]
    fn parse_asset_matches_official_pattern() {
        assert_eq!(
            parse_asset("camoufox-152.0.4-beta.29-win.x86_64.zip"),
            Some(("152.0.4".into(), "beta.29".into()))
        );
        assert_eq!(
            parse_asset("camoufox-135.0.1-stable-win.x86_64.zip"),
            Some(("135.0.1".into(), "stable".into()))
        );
        assert_eq!(parse_asset("camoufox-152.0.4-beta.29-lin.arm64.zip"), None);
        assert_eq!(
            parse_asset("camoufox-152.0.4-beta.29-win.x86_64.tar.bz2"),
            None
        );
        assert_eq!(parse_asset("other-1.0-beta.1-win.x86_64.zip"), None);
    }

    #[test]
    fn version_key_orders_newest_first() {
        assert!(version_key("152.0.4-beta.29") > version_key("152.0.4-beta.19"));
        assert!(version_key("152.0.4-beta.29") > version_key("135.0.1"));
        assert!(version_key("99.0") < version_key("135.0"));
    }

    #[test]
    fn common_root_detection() {
        let flat = vec!["camoufox.exe".to_string(), "fonts/x.ttf".to_string()];
        assert_eq!(common_root_depth(&flat), 0);
        let nested = vec![
            "camoufox/camoufox.exe".to_string(),
            "camoufox/fonts/x.ttf".to_string(),
        ];
        assert_eq!(common_root_depth(&nested), 1);
        let empty_name = vec!["/abs/path".to_string()];
        assert_eq!(common_root_depth(&empty_name), 0);
    }

    #[test]
    #[ignore]
    fn fetch_releases_live() {
        let releases = fetch_releases().expect("fetch_releases failed");
        assert!(!releases.is_empty(), "no releases parsed");
        let newest = &releases[0];
        assert!(
            newest.asset_url.contains("win.x86_64.zip"),
            "url: {}",
            newest.asset_url
        );
        println!(
            "live newest: {} ({} MB)",
            newest.full,
            newest.size / 1048576
        );
    }

    #[test]
    fn cleanup_part_dirs_removes_only_part_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join(".part-1.0-beta.1").join("unpacked")).unwrap();
        std::fs::write(root.join(".part-1.0-beta.1").join("camoufox.zip"), b"z").unwrap();
        let active = root.join(format!(".part-active-{}", std::process::id()));
        std::fs::create_dir_all(&active).unwrap();
        let keep = root.join("1.0-beta.1");
        std::fs::create_dir_all(&keep).unwrap();
        std::fs::write(keep.join(exe_name()), b"exe").unwrap();

        cleanup_part_dirs(root);

        assert!(!root.join(".part-1.0-beta.1").exists());
        assert!(active.exists());
        assert!(keep.join(exe_name()).exists());
    }

    #[test]
    fn extract_zip_handles_flat_and_nested_layouts() {
        use std::io::Write as _;

        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("t.zip");
        let make_zip = |path: &std::path::Path, prefix: &str| {
            let f = std::fs::File::create(path).unwrap();
            let mut w = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default();
            w.start_file(format!("{prefix}camoufox.exe"), opts).unwrap();
            w.write_all(b"exe").unwrap();
            w.start_file(format!("{prefix}fonts/a.ttf"), opts).unwrap();
            w.write_all(b"font").unwrap();
            w.finish().unwrap();
        };

        let dest = tmp.path().join("out-flat");
        std::fs::create_dir_all(&dest).unwrap();
        make_zip(&zip_path, "");
        extract_zip(&zip_path, &dest, &DownloadControl::default()).unwrap();
        assert!(dest.join("camoufox.exe").exists());
        assert!(dest.join("fonts").join("a.ttf").exists());

        let dest2 = tmp.path().join("out-nested");
        std::fs::create_dir_all(&dest2).unwrap();
        make_zip(&zip_path, "camoufox/");
        extract_zip(&zip_path, &dest2, &DownloadControl::default()).unwrap();
        assert!(dest2.join("camoufox.exe").exists());
        assert!(!dest2.join("camoufox").exists());
    }
}
