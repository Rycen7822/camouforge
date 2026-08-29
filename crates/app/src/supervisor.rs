//! Worker 监管器：spawn Python worker 子进程，stdio 行协议请求/响应关联 + 事件流。
//!
//! 生命周期：
//! - `WorkerSupervisor::start()`：spawn worker（同 data 目录），后台线程读 stdout
//! - 请求走 `request()`：id 关联等待响应（带超时）
//! - worker 单行事件（instance_exited 等）转成 `SupervisorEvent` 广播给 UI
//! - worker 意外退出 → 自动重启（最多 3 次指数退避），实例状态标记为 exited

use crate::logging::LogSink;
use anyhow::{anyhow, Context, Result};
use camoforge_protocol::{
    GenerateFingerprintParams, GenerateFingerprintResult, InstanceListResult, LaunchParams,
    LaunchResult, ListFontsParams, ListFontsResult, ListVersionsResult, ListVoicesParams,
    ListVoicesResult, ListWebglParams, ListWebglResult, Request, Response, ValidateParams,
    ValidateResult,
};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone)]
pub enum SupervisorEvent {
    WorkerReady {
        camoufox: Option<String>,
        browser: Option<String>,
    },
    WorkerRestarted {
        attempt: u32,
    },
    WorkerFailed {
        reason: String,
    },
    InstanceExited {
        profile_id: String,
        reason: String,
    },
}

pub struct WorkerSupervisor {
    state: Arc<Shared>,
}

struct Shared {
    python: String,
    worker_script: PathBuf,
    data_dir: PathBuf,
    log_sink: LogSink,
    child: Mutex<Option<Child>>,
    pending: Mutex<HashMap<String, (String, std::sync::mpsc::Sender<Response>)>>,
    events_tx: std::sync::mpsc::Sender<SupervisorEvent>,
    next_id: AtomicU64,
    restarts: Mutex<u32>,
    shutting_down: AtomicBool,
}

impl WorkerSupervisor {
    pub fn new(
        python: String,
        worker_script: PathBuf,
        data_dir: PathBuf,
        log_sink: LogSink,
    ) -> (Self, mpsc::Receiver<SupervisorEvent>) {
        let (tx, rx) = mpsc::channel();
        let sup = Self {
            state: Arc::new(Shared {
                python,
                worker_script,
                data_dir,
                log_sink,
                child: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                events_tx: tx,
                next_id: AtomicU64::new(1),
                restarts: Mutex::new(0),
                shutting_down: AtomicBool::new(false),
            }),
        };
        (sup, rx)
    }

    pub fn start(&self) -> Result<()> {
        // shutdown 竞态：环境自愈完成后才调用 start，此时应用可能已在退出
        if self.state.shutting_down.load(Ordering::Relaxed) {
            return Err(anyhow!("supervisor 正在关闭，拒绝启动 worker"));
        }
        {
            let guard = self.state.child.lock().unwrap();
            if guard.is_some() {
                return Ok(());
            }
        }
        Self::spawn_worker(&self.state)
    }

    pub fn is_shutting_down(&self) -> bool {
        self.state.shutting_down.load(Ordering::Relaxed)
    }

    /// 最多自动重启次数。
    const MAX_RESTARTS: u32 = 3;

    fn spawn_worker(shared: &Arc<Shared>) -> Result<()> {
        let mut cmd = Command::new(&shared.python);
        cmd.arg("-u")
            .arg(&shared.worker_script)
            .current_dir(&shared.data_dir)
            .env("CAMOFORGE_DATA_DIR", &shared.data_dir)
            // worker 用 UTF-8 收发 stdio：Windows 上 Python 默认按 locale（如 GBK）
            // 编码 stdio，中文身份名/日志会在 Rust↔Python 之间乱码甚至解析失败。
            .env("PYTHONUTF8", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        // worker 的 stderr（Python traceback、camoufox SDK 告警）不进协议通道，
        // 直接重定向到当日日志文件，供启动失败排查。
        if let Some(f) = shared.log_sink.stderr_file() {
            cmd.stderr(Stdio::from(f));
        } else {
            cmd.stderr(Stdio::null());
        }
        crate::command::hide_console_window(&mut cmd);
        let mut child = cmd.spawn().context("spawn python worker")?;

        // Windows 进程树清理：main.rs 已在启动时把本进程（及此后派生的
        // python worker、firefox）绑进 KILL_ON_JOB_CLOSE 的 Job Object，
        // 应用退出（含崩溃）时内核自动回收整棵进程树，此处无需重复绑 Job。
        // 注意：子进程默认继承父进程的 Job，再单独 assign 会因
        // "进程已在 Job 中" 而失败，故不要在此再建 Job Object。

        let stdout = child.stdout.take().expect("piped stdout");
        {
            let mut guard = shared.child.lock().unwrap();
            *guard = Some(child);
        }

        let shared = shared.clone();
        std::thread::spawn(move || Self::reader_loop(shared, stdout));

        Ok(())
    }

    fn reader_loop(shared: Arc<Shared>, stdout: std::process::ChildStdout) {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
                shared.log_sink.write("worker", &line);
                continue;
            };
            Self::dispatch_response_or_event(&shared, message);
        }
        Self::handle_worker_exit(&shared);
    }

    fn dispatch_response_or_event(shared: &Arc<Shared>, message: serde_json::Value) {
        if let Some(id) = message
            .get("id")
            .and_then(|value| value.as_str())
            .map(String::from)
        {
            let Ok(response) = serde_json::from_value::<Response>(message) else {
                return;
            };
            if let Some((_method, sender)) = shared.pending.lock().unwrap().remove(&id) {
                let _ = sender.send(response);
            }
            return;
        }

        let Some(event) = message.get("event").and_then(|value| value.as_str()) else {
            return;
        };
        let data = message.get("data").cloned().unwrap_or_default();
        let event = match event {
            "ready" => {
                // ready 证明本次重启已恢复；旧崩溃不能永久占用后续重启额度。
                *shared.restarts.lock().unwrap() = 0;
                SupervisorEvent::WorkerReady {
                    camoufox: data
                        .get("camoufox")
                        .and_then(|value| value.as_str())
                        .map(String::from),
                    browser: data
                        .get("browser")
                        .and_then(|value| value.as_str())
                        .map(String::from),
                }
            }
            "instance_exited" => SupervisorEvent::InstanceExited {
                profile_id: data
                    .get("profile_id")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default()
                    .to_string(),
                reason: data
                    .get("reason")
                    .and_then(|value| value.as_str())
                    .unwrap_or("unknown")
                    .to_string(),
            },
            _ => return,
        };
        let _ = shared.events_tx.send(event);
    }

    fn handle_worker_exit(shared: &Arc<Shared>) {
        // stdout 关闭时丢弃 sender，让等待中的请求立即失败而不是耗尽超时。
        shared.pending.lock().unwrap().clear();
        shared.child.lock().unwrap().take();
        if shared.shutting_down.load(Ordering::Relaxed) {
            return;
        }

        let attempt = {
            let mut restarts = shared.restarts.lock().unwrap();
            *restarts += 1;
            *restarts
        };
        if attempt > Self::MAX_RESTARTS {
            let _ = shared.events_tx.send(SupervisorEvent::WorkerFailed {
                reason: format!("worker 反复崩溃（{attempt} 次），已停止自动重启"),
            });
            return;
        }

        let _ = shared
            .events_tx
            .send(SupervisorEvent::WorkerRestarted { attempt });
        std::thread::sleep(Duration::from_secs(1u64 << (attempt - 1)));
        if let Err(error) = Self::spawn_worker(shared) {
            let _ = shared.events_tx.send(SupervisorEvent::WorkerFailed {
                reason: format!("worker 重启失败: {error:#}"),
            });
        }
    }

    pub fn is_running(&self) -> bool {
        self.state.child.lock().unwrap().is_some()
    }

    pub fn request<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: &P,
    ) -> Result<R> {
        let id = self
            .state
            .next_id
            .fetch_add(1, Ordering::Relaxed)
            .to_string();
        let req = Request {
            id: id.clone(),
            method: method.to_string(),
            params: serde_json::to_value(params)?,
        };
        let (tx, rx) = mpsc::channel();
        // P1-5：launch 期间 worker 主线程阻塞在 manager.launch()（最长 180s），
        // 排队的 stop/stop_all 无法被读取。与其让调用方挂到 60s 超时再收到误导性
        // 的失败，直接返回显式原因；启动完成后（pending 清空）重试即可。
        if method == "stop" || method == "stop_all" {
            let launching = self
                .state
                .pending
                .lock()
                .unwrap()
                .values()
                .any(|(m, _)| m == "launch");
            if launching {
                return Err(anyhow!("浏览器仍在启动中，请稍后重试"));
            }
        }
        // 前置检查（worker 存活、请求可序列化）先于登记 pending；登记后唯一
        // 可能失败的是写管道，失败即清理，避免泄漏的 launch 条目让 stop/stop_all
        // 永远返回"仍在启动中"。
        let mut child_guard = self.state.child.lock().unwrap();
        let child = child_guard
            .as_mut()
            .ok_or_else(|| anyhow!("worker not running"))?;
        let stdin = child.stdin.as_mut().expect("piped stdin");
        let line = serde_json::to_string(&req)?;

        self.state
            .pending
            .lock()
            .unwrap()
            .insert(id.clone(), (method.to_string(), tx));
        if let Err(e) = writeln!(stdin, "{line}").and_then(|_| stdin.flush()) {
            self.state.pending.lock().unwrap().remove(&id);
            return Err(e.into());
        }
        drop(child_guard);

        let timeout = if method == "launch" { 200 } else { 60 };
        let resp = match rx.recv_timeout(Duration::from_secs(timeout)) {
            Ok(resp) => resp,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // reader 线程已在 worker 退出时清空 pending（sender 被 drop）。
                self.state.pending.lock().unwrap().remove(&id);
                return Err(anyhow!("worker 连接已中断: {method}"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // 超时：清理 pending 表项，避免每次超时泄漏一个 sender + id。
                self.state.pending.lock().unwrap().remove(&id);
                return Err(anyhow!("worker response timeout ({timeout}s): {method}"));
            }
        };
        self.state.pending.lock().unwrap().remove(&id);
        if resp.ok {
            let result = resp.result.unwrap_or(serde_json::Value::Null);
            Ok(serde_json::from_value(result)?)
        } else {
            Err(anyhow!(
                "{}",
                resp.error.unwrap_or_else(|| "unknown error".into())
            ))
        }
    }

    /// 优雅关闭：先发 shutdown RPC（不阻塞等响应）再短宽限后 kill。
    pub fn shutdown(&self) {
        // 标记正常退出，读线程检测 stdout 关闭后不再重启
        self.state.shutting_down.store(true, Ordering::Relaxed);
        // 直接写 shutdown 请求而不走 request()：launch 可能正阻塞 worker 的读循环，
        // 等响应会让 UI 线程卡住（最长 60s）。给 worker 极短宽限优雅 stop_all，超时
        // 即 kill；整棵进程树最终由 main.rs 的 Job Object 兜底回收。
        {
            let mut guard = self.state.child.lock().unwrap();
            if let Some(child) = guard.as_mut() {
                if let Some(stdin) = child.stdin.as_mut() {
                    let req = Request {
                        id: "shutdown".to_string(),
                        method: "shutdown".to_string(),
                        params: serde_json::Value::Null,
                    };
                    if let Ok(line) = serde_json::to_string(&req) {
                        let _ = writeln!(stdin, "{line}");
                        let _ = stdin.flush();
                    }
                }
            }
        }
        if let Some(mut child) = self.state.child.lock().unwrap().take() {
            // 给 worker 一个极短窗口优雅 stop_all；未退出即 kill，进程树由 Job Object 兜底。
            std::thread::sleep(Duration::from_millis(500));
            if matches!(child.try_wait(), Ok(None)) {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    /// worker 通信探测（supervisor 测试用；UI 就绪状态由 WorkerReady 事件驱动）。
    #[allow(dead_code)]
    pub fn ping(&self) -> Result<serde_json::Value> {
        self.request("ping", &serde_json::Value::Null)
    }

    pub fn launch(&self, profile: &camoforge_protocol::Profile) -> Result<LaunchResult> {
        self.request(
            "launch",
            &LaunchParams {
                profile_id: profile.id.clone(),
                profile: profile.clone(),
                user_data_dir: None,
            },
        )
    }

    pub fn stop(&self, profile_id: &str) -> Result<serde_json::Value> {
        self.request("stop", &serde_json::json!({ "profile_id": profile_id }))
    }

    pub fn stop_all(&self) -> Result<serde_json::Value> {
        self.request("stop_all", &serde_json::Value::Null)
    }

    /// 对应 worker `list` RPC（UI 实例状态由 `state.running` 维护，暂未接入）。
    #[allow(dead_code)]
    pub fn list_instances(&self) -> Result<InstanceListResult> {
        self.request("list", &serde_json::Value::Null)
    }

    pub fn validate(&self, profile: &camoforge_protocol::Profile) -> Result<ValidateResult> {
        self.request(
            "validate",
            &ValidateParams {
                profile: profile.clone(),
            },
        )
    }

    pub fn generate_fingerprint(
        &self,
        os: Option<Vec<String>>,
        ff_version: Option<u32>,
        locale: Option<String>,
    ) -> Result<GenerateFingerprintResult> {
        self.request(
            "generate_fingerprint",
            &GenerateFingerprintParams {
                os,
                ff_version,
                locale,
            },
        )
    }

    pub fn list_webgl(&self, os: Option<String>) -> Result<ListWebglResult> {
        self.request("list_webgl", &ListWebglParams { os })
    }

    pub fn list_versions(&self) -> Result<ListVersionsResult> {
        self.request("list_versions", &serde_json::Value::Null)
    }

    pub fn list_fonts(&self, os: Option<String>) -> Result<ListFontsResult> {
        self.request("list_fonts", &ListFontsParams { os })
    }

    pub fn list_voices(&self, os: Option<String>) -> Result<ListVoicesResult> {
        self.request("list_voices", &ListVoicesParams { os })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn worker_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("worker/camoforge_worker.py")
    }

    #[test]
    fn start_refused_after_shutdown() {
        let data_dir = tempfile::tempdir().unwrap();
        let (sup, _rx) = WorkerSupervisor::new(
            "python".into(),
            PathBuf::from("nonexistent-worker.py"),
            data_dir.path().to_path_buf(),
            LogSink::new(data_dir.path().join("logs")),
        );
        assert!(!sup.is_shutting_down());
        sup.shutdown();
        assert!(sup.is_shutting_down());
        let err = sup.start().unwrap_err();
        assert!(format!("{err:#}").contains("拒绝启动"), "{err:#}");
    }

    #[test]
    fn worker_ping_roundtrip() {
        if !worker_path().exists() {
            eprintln!("worker script not found, skip");
            return;
        }
        // 用项目 .venv 的 python（与应用运行环境一致）；缺失时跳过。
        // Windows 上裸 "python3" 可能命中 Store 存根进程（存活但不响应），不用。
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let python = if cfg!(target_os = "windows") {
            root.join(".venv/Scripts/python.exe")
        } else {
            root.join(".venv/bin/python")
        };
        if !python.exists() {
            eprintln!("venv python not found, skip");
            return;
        }
        let python = python.to_string_lossy().into_owned();
        let data_dir = tempfile::tempdir().unwrap();
        let (sup, _rx) = WorkerSupervisor::new(
            python,
            worker_path(),
            data_dir.path().to_path_buf(),
            LogSink::new(data_dir.path().join("logs")),
        );
        sup.start().unwrap();
        let pong: serde_json::Value = sup.ping().unwrap();
        assert!(pong.get("camoufox").is_some());
        sup.shutdown();
    }
}
