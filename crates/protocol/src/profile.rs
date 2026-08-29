//! Profile 数据模型 —— 对 camoufox 0.5.5 `launch_options()` 参数面的完整结构化表达。
//!
//! 设计原则：
//! - 所有 SDK 参数一一对应，不增不减；`Option` 的语义 = "未设置则由 SDK 默认行为接管"
//! - 语义模式（枚举 tag="mode"）让 UI 三态选择直接序列化，Python 端展开为 SDK kwargs
//! - `config` 为扁平点分键字典（87 个指纹注入键），由 `registry` 驱动 UI，天然向前兼容

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type JsonMap = BTreeMap<String, serde_json::Value>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub launch: LaunchOptions,
    /// 指纹注入键（扁平点分字符串 → 值），等价于 SDK `config=` 参数。
    #[serde(default)]
    pub config: JsonMap,
}

impl Profile {
    pub fn new(name: impl Into<String>) -> Self {
        let now = unix_ts();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            notes: String::new(),
            created_at: now,
            updated_at: now,
            launch: LaunchOptions::default(),
            config: JsonMap::new(),
        }
    }

    pub fn touch(&mut self) {
        self.updated_at = unix_ts();
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shortcut {
    pub name: String,
    pub url: String,
}

/// camoufox `launch_options()` 的完整参数面（config 除外，见 `Profile::config`）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchOptions {
    /// 目标 OS 指纹（windows/macos/linux）。None = SDK 默认三系统加权随机；多项 = 随机挑选。
    pub os: Option<Vec<String>>,
    pub humanize: Option<HumanizeMode>,
    pub headless: HeadlessMode,
    /// prefers-color-scheme 模拟：None = 驱动默认（light）；"dark"/"light" 显式指定。
    /// 驱动未显式指定时会把 context colorScheme 默认成 "light"，导致支持暗色的站点
    /// （如 ippure.com）即使浏览器 chrome 是暗色也渲染成白色。设 "dark" 可修正。
    pub color_scheme: Option<String>,
    pub persistent_context: Option<bool>,
    /// persistent_context=true 时的用户数据目录（空 = 按 profile id 自动生成）。
    pub user_data_dir: Option<String>,
    /// 下载保存目录。None = 系统 Downloads 目录（Windows 上 %USERPROFILE%\Downloads）。
    pub downloads_dir: Option<String>,
    /// 启动页/主页快捷方式（如 Chrome 新标签页的快捷图标，名称 + 网址）。
    pub shortcuts: Vec<Shortcut>,

    pub block_images: Option<bool>,
    pub block_webrtc: Option<bool>,
    pub block_webgl: Option<bool>,
    pub disable_coop: Option<bool>,
    pub main_world_eval: Option<bool>,
    pub enable_cache: Option<bool>,
    pub debug: Option<bool>,
    pub i_know_what_im_doing: Option<bool>,

    pub webgl_config: Option<WebglConfig>,
    /// browserforge 完整 Fingerprint 对象（serde_json 透传）。
    pub fingerprint: Option<serde_json::Value>,
    pub fingerprint_preset: Option<FingerprintPreset>,
    pub ff_version: Option<u32>,

    pub geoip: Option<GeoipMode>,
    pub geoip_db: Option<String>,
    pub proxy: Option<Proxy>,
    /// Playwright proxy server 字符串拆解为 server/username/password/port。
    /// locale 列表（首个用于 Intl API）。
    pub locale: Option<Vec<String>>,

    pub fonts: Option<Vec<String>>,
    pub custom_fonts_only: Option<bool>,
    pub addons: Vec<String>,
    pub exclude_addons: Vec<String>,

    pub screen: Option<ScreenSpec>,
    pub window: Option<(u32, u32)>,

    pub executable_path: Option<String>,
    /// 已安装的浏览器版本选择（如 "beta.20" / "official/beta.20"）。
    pub browser: Option<String>,
    /// Firefox user prefs（键值对）。
    pub firefox_user_prefs: JsonMap,
    /// 附加浏览器命令行参数。
    pub args: Vec<String>,
    /// 环境变量。
    pub env: JsonMap,
    /// 透传给 Playwright launch 的其余参数（viewport、timeout 等）。
    pub extra_launch_options: JsonMap,
}

/// humanize 参数：off / true / 自定义最大时长（秒）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum HumanizeMode {
    Off,
    On,
    Custom { max_time: f32 },
}

/// geoip 参数：关闭 / 自动探测出口 IP / 指定 IP。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum GeoipMode {
    Off,
    Auto,
    Ip { ip: String },
}

/// 显示模式。`Virtual` 仅 Linux（Xvfb 虚拟显示），Windows 上等价 Headed。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadlessMode {
    #[default]
    Headed,
    Headless,
    Virtual,
}

/// screen 参数：精确尺寸 / 范围随机。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum ScreenSpec {
    Exact {
        width: u32,
        height: u32,
    },
    Range {
        min_width: u32,
        max_width: u32,
        min_height: u32,
        max_height: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WebglConfig {
    pub vendor: String,
    pub renderer: String,
}

/// fingerprint_preset 参数：不用 / 随机捆绑预设 / 指定预设对象。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum FingerprintPreset {
    Off,
    Random,
    Value { value: serde_json::Value },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Proxy {
    pub server: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// HTTP 代理端口伪装值（config `port` 键的便捷入口，None = 不伪装）。
    pub port: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceInfo {
    pub profile_id: String,
    pub profile_name: String,
    pub pid: Option<u32>,
    pub headless: bool,
    pub started_at: i64,
    /// persistent_context 模式下的 user_data_dir。
    pub user_data_dir: Option<String>,
}

pub fn unix_ts() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
