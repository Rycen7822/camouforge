//! 应用级设置：数据目录 settings.json，原子写。
//!
//! 目前设置：本地 camoufox 浏览器目录（替代 pip 缓存 / 自动探测）；
//! 关闭窗口时是否弹窗询问（退出 / 最小化到托盘）。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// 关闭窗口行为的进程内镜像：`on_window_should_close` 回调没有 view/state 访问权，
/// 只能读全局；设置变更时经 [`AppSettings::sync_globals`] 同步到这里。
pub static CLOSE_ACTION: AtomicU8 = AtomicU8::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum CloseAction {
    #[default]
    Confirm,
    Exit,
    MinimizeToTray,
}

impl CloseAction {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => CloseAction::Exit,
            2 => CloseAction::MinimizeToTray,
            _ => CloseAction::Confirm,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            CloseAction::Confirm => 0,
            CloseAction::Exit => 1,
            CloseAction::MinimizeToTray => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "AppSettingsSerde")]
pub struct AppSettings {
    pub camoufox_dir: Option<String>,
    /// 点右上角关闭窗口时的行为。
    /// 默认弹窗询问；旧 settings.json 里 `confirm_on_close=false` 会映射为 Exit。
    #[serde(default)]
    pub close_action: CloseAction,
}

#[derive(Debug, Deserialize)]
struct AppSettingsSerde {
    camoufox_dir: Option<String>,
    confirm_on_close: Option<bool>,
    close_action: Option<CloseAction>,
}

impl From<AppSettingsSerde> for AppSettings {
    fn from(raw: AppSettingsSerde) -> Self {
        let close_action = raw.close_action.unwrap_or_else(|| {
            // 兼容旧 settings.json 里的 confirm_on_close 布尔字段。
            if raw.confirm_on_close == Some(false) {
                CloseAction::Exit
            } else {
                CloseAction::Confirm
            }
        });
        Self {
            camoufox_dir: raw.camoufox_dir,
            close_action,
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            camoufox_dir: None,
            close_action: CloseAction::Confirm,
        }
    }
}

impl AppSettings {
    pub fn load(dir: &Path) -> Self {
        let path = dir.join("settings.json");
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str::<AppSettings>(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        let path = dir.join("settings.json");
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_string_pretty(self).context("serialize settings")?;
        std::fs::write(&tmp, json.as_bytes()).context("write settings tmp")?;
        std::fs::rename(&tmp, &path).context("rename settings")?;
        Ok(())
    }

    pub fn executable_path(&self) -> Option<PathBuf> {
        let dir = self.camoufox_dir.as_ref()?;
        let p = PathBuf::from(dir);
        if p.is_dir() {
            let exe = if cfg!(target_os = "windows") {
                "camoufox.exe"
            } else {
                "camoufox"
            };
            Some(p.join(exe))
        } else {
            Some(p)
        }
    }

    pub fn sync_globals(&self) {
        CLOSE_ACTION.store(self.close_action.to_u8(), Ordering::Relaxed);
    }
}

pub fn close_action() -> CloseAction {
    CloseAction::from_u8(CLOSE_ACTION.load(Ordering::Relaxed))
}
