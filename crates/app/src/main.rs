//! 应用入口与 Windows 生命周期集成。

// Windows GUI 子系统：双击 exe 不再被默认终端托管（不弹控制台窗口）。
// 代价：stdout/stderr 不再挂控制台，启动期错误只能靠 UI 状态呈现。
#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod bootstrap;
mod browser;
mod command;
mod logging;
mod net;
mod platform;
mod python_env;
mod settings;
#[cfg(windows)]
mod shortcut;
#[cfg(windows)]
mod single;
mod state;
mod store;
mod supervisor;
#[cfg(windows)]
mod tray;
mod ui;

use std::path::{Path, PathBuf};
const APP_NAME: &str = "camoforge";

/// 数据目录：
/// 1. 环境变量 `CAMOFORGE_DATA_DIR` 显式指定时优先；
/// 2. 默认使用 exe 同目录的 `{APP_NAME}_data/`，避免在 C 盘写数据；
/// 3. 回退到系统标准应用数据目录（Windows `%APPDATA%/...`，其他 `~/.config/...`）。
fn data_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("CAMOFORGE_DATA_DIR") {
        return PathBuf::from(dir);
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            return parent.join(format!("{}_data", APP_NAME));
        }
    }

    if cfg!(target_os = "windows") {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata).join(APP_NAME);
        }
    }
    if let Ok(home) = std::env::var("HOME") {
        if cfg!(target_os = "macos") {
            return PathBuf::from(home)
                .join("Library/Application Support")
                .join(APP_NAME);
        }
        return PathBuf::from(home).join(".config").join(APP_NAME);
    }
    PathBuf::from(".")
}

fn parse_launch_profile_arg() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--profile" {
            return args.next();
        }
        if let Some(id) = arg.strip_prefix("--profile=") {
            return Some(id.to_string());
        }
    }
    None
}

fn fatal_startup_error(msg: &str) -> ! {
    eprintln!("{msg}");
    #[cfg(windows)]
    platform::windows::fatal_message_box("CamouForge 启动失败", msg);
    std::process::exit(1);
}

/// 安装 panic hook：windows_subsystem 下 UI 线程 panic 会静默消失，
/// 这里把 panic 信息追加写入数据目录 logs/panic.log。
fn install_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("[camoforge panic] {}", info);
        eprintln!("{msg}");
        let logs = data_dir().join("logs");
        if std::fs::create_dir_all(&logs).is_err() {
            return;
        }
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(logs.join("panic.log"))
        {
            let ts = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            let _ = writeln!(f, "[{ts}] {msg}");
        }
    }));
}

fn main() {
    install_panic_hook();
    let launch_profile_id = parse_launch_profile_arg();

    let dir = data_dir();

    // 单实例保护：已有实例在跑时，把启动请求转交给它，本进程立即退出，
    // 不再重复启动 worker / 浏览器（避免双击 exe 或快捷方式堆出无限个进程）。
    #[cfg(target_os = "windows")]
    if !single::acquire() {
        single::forward_launch_request(launch_profile_id.as_deref(), &dir);
        return;
    }

    #[cfg(windows)]
    platform::windows::assign_process_job();

    if let Err(e) = std::fs::create_dir_all(&dir) {
        fatal_startup_error(&format!("无法创建数据目录 {}: {e}", dir.display()));
    }

    let log_sink = logging::LogSink::init(&dir);
    log_sink.write("info", "CamouForge 启动");

    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()));
    let runtime_layout = python_env::RuntimeLayout::resolve(
        exe_dir.as_deref().unwrap_or_else(|| Path::new(".")),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    )
    .unwrap_or_else(|e| fatal_startup_error(&format!("{e:#}")));

    bootstrap::run(dir, log_sink, runtime_layout, launch_profile_id);
}
