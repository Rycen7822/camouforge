//! 单实例保护（仅 Windows 编译，main.rs 里 `#[cfg(windows)] mod single`）。
//!
//! 无保护时每次双击 exe/快捷方式都会再起一个完整进程（各自带 worker 和浏览器）。
//! 这里用命名 Mutex 判断是否已有实例：
//! - 新实例发现已有实例在跑时，把启动请求（`--profile <id>`，可为空）写入
//!   `data_dir/pending_launch/req_<pid>.txt` 后立即退出，不启动任何东西；
//! - 新实例写完后触发命名事件；老实例的等待线程收到事件才读取请求目录。

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject, EVENT_MODIFY_STATE,
    INFINITE,
};

const MUTEX_NAME: &str = "Local\\camoforge.single";
const EVENT_NAME: &str = "Local\\camoforge.launch-request";

/// 尝试获取单实例锁。
/// 返回 `true` = 本进程是唯一实例（锁持有至进程退出）；
/// 返回 `false` = 已有实例在运行，调用方应转交请求后退出。
pub fn acquire() -> bool {
    let name = windows::core::HSTRING::from(MUTEX_NAME);
    match unsafe { CreateMutexW(None, true, windows::core::PCWSTR(name.as_ptr())) } {
        Ok(handle) => {
            let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
            if already_exists {
                // HANDLE 是 Copy、无 Drop：显式关闭刚打开的句柄，锁仍归已有实例持有。
                unsafe {
                    let _ = CloseHandle(handle);
                }
                false
            } else {
                // 唯一实例：不关闭句柄，锁持有到进程退出（进程退出时内核自动释放）。
                // HANDLE 是 Copy 且无 Drop，这里借用保留即可，无需 mem::forget。
                let _ = handle;
                true
            }
        }
        // 创建失败（极端情况）按唯一实例处理，避免误判导致程序无法启动。
        Err(_) => true,
    }
}

pub fn forward_launch_request(profile_id: Option<&str>, data_dir: &Path) {
    let dir = data_dir.join("pending_launch");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let file = dir.join(format!("req_{}.txt", std::process::id()));
    let content = profile_id.unwrap_or("");
    if std::fs::write(file, content).is_err() {
        return;
    }
    let name = windows::core::HSTRING::from(EVENT_NAME);
    if let Ok(event) = unsafe {
        OpenEventW(
            EVENT_MODIFY_STATE,
            false,
            windows::core::PCWSTR(name.as_ptr()),
        )
    } {
        unsafe {
            let _ = SetEvent(event);
            let _ = CloseHandle(event);
        }
    }
}

pub fn take_launch_requests(data_dir: &Path) -> Vec<Option<String>> {
    let dir = data_dir.join("pending_launch");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("txt"))
        .map(|e| {
            let path = e.path();
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            let _ = std::fs::remove_file(&path);
            if content.is_empty() {
                None
            } else {
                Some(content)
            }
        })
        .collect()
}

pub fn launch_request_events(data_dir: PathBuf) -> async_channel::Receiver<Option<String>> {
    let (tx, rx) = async_channel::unbounded();
    let _ = std::thread::Builder::new()
        .name("camoforge-single-instance".into())
        .spawn(move || {
            let name = windows::core::HSTRING::from(EVENT_NAME);
            let Ok(event) =
                (unsafe { CreateEventW(None, false, false, windows::core::PCWSTR(name.as_ptr())) })
            else {
                return;
            };

            'events: loop {
                for request in take_launch_requests(&data_dir) {
                    if tx.send_blocking(request).is_err() {
                        break 'events;
                    }
                }
                if unsafe { WaitForSingleObject(event, INFINITE) } != WAIT_OBJECT_0 {
                    break;
                }
            }
            unsafe {
                let _ = CloseHandle(event);
            }
        });
    rx
}
