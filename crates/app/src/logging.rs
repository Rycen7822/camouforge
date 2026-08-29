//! 应用日志落盘：UI 操作日志与 worker 的 stderr 统一追加到 exe 同目录 `logs/` 下按天滚动的文件。
//!
//! - 默认目录：`<exe 所在目录>/logs`；不可写时回退数据目录 `logs/`，再回退系统临时目录。
//! - 文件名按天：`2026-08-24.log`，追加写入，不截断。
//! - 线程安全：UI 线程（state.log）与 supervisor 读线程（worker 杂散输出）共用。

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LogSink {
    inner: Arc<LogSinkInner>,
}

/// 日志文件保留天数；超出的 `.log` 在建 sink 时删除，logs/ 不无限增长。
const RETENTION_DAYS: u64 = 14;

struct LogSinkInner {
    dir: PathBuf,
    /// 串行化同一文件内的追加写，避免跨线程行交错。
    lock: Mutex<()>,
}

impl LogSink {
    pub fn init(fallback_dir: &Path) -> LogSink {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                if let Ok(dir) = Self::mkdir(parent.join("logs")) {
                    return Self::new(dir);
                }
            }
        }
        if let Ok(dir) = Self::mkdir(fallback_dir.join("logs")) {
            return Self::new(dir);
        }
        Self::new(std::env::temp_dir().join("camoforge-logs"))
    }

    pub fn new(dir: PathBuf) -> LogSink {
        let _ = std::fs::create_dir_all(&dir);
        Self::prune_old_logs(&dir);
        LogSink {
            inner: Arc::new(LogSinkInner {
                dir,
                lock: Mutex::new(()),
            }),
        }
    }

    fn prune_old_logs(dir: &Path) {
        let Some(cutoff) = std::time::SystemTime::now()
            .checked_sub(std::time::Duration::from_secs(RETENTION_DAYS * 86_400))
        else {
            return;
        };
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("log") {
                continue;
            }
            let old = entry
                .metadata()
                .and_then(|m| m.modified())
                .map(|t| t < cutoff)
                .unwrap_or(false);
            if old {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    fn mkdir(dir: PathBuf) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    pub fn write(&self, level: &str, text: &str) {
        let _guard = self.inner.lock.lock().unwrap();
        let (date, clock) = now_local();
        let path = self.inner.dir.join(format!("{date}.log"));
        let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) else {
            return;
        };
        let _ = writeln!(f, "[{clock}] [{level}] {text}");
    }

    pub fn stderr_file(&self) -> Option<File> {
        let _guard = self.inner.lock.lock().unwrap();
        let (date, _) = now_local();
        OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.inner.dir.join(format!("{date}.log")))
            .ok()
    }
}

/// 本地时间（Windows 用 GetLocalTime；其他平台退化为 UTC）。
fn now_local() -> (String, String) {
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        struct SystemTime {
            year: u16,
            month: u16,
            day_of_week: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            milliseconds: u16,
        }
        extern "system" {
            fn GetLocalTime(lp_system_time: *mut SystemTime);
        }
        let mut t = SystemTime {
            year: 0,
            month: 0,
            day_of_week: 0,
            day: 0,
            hour: 0,
            minute: 0,
            second: 0,
            milliseconds: 0,
        };
        unsafe { GetLocalTime(&mut t) };
        (
            format!("{:04}-{:02}-{:02}", t.year, t.month, t.day),
            format!("{:02}:{:02}:{:02}", t.hour, t.minute, t.second),
        )
    }
    #[cfg(not(target_os = "windows"))]
    {
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let days = secs.div_euclid(86_400);
        let sod = secs.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        (
            format!("{y:04}-{m:02}-{d:02}"),
            format!("{:02}:{:02}:{:02}", sod / 3600, (sod % 3600) / 60, sod % 60),
        )
    }
}

/// UNIX 纪元天数 → (年, 月, 日)。Howard Hinnant 的 civil_from_days。
#[cfg(not(target_os = "windows"))]
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m as u32, d as u32)
}
