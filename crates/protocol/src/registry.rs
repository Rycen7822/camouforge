//! 指纹注入键注册表：camoufox `config` 参数全部可配置属性的元数据。
//!
//! 来源：camoufox.com/fingerprint 文档 14 个子页 + camoufox 0.5.5 源码交叉核对。
//! UI 由该表驱动渲染表单；未登记的键在"原始 JSON"面板仍可编辑（向前兼容）。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Multiline,
    Int,
    SInt,
    Float,
    Bool,
    TextList,
    Select(&'static [&'static str]),
    SelectInt(&'static [&'static str]),
    SelectMulti(&'static [&'static str]),
    Json,
}

#[derive(Debug, Clone)]
pub struct FieldSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub kind: FieldKind,
    pub hint: &'static str,
    pub placeholder: &'static str,
}

pub struct FieldGroup {
    pub id: &'static str,
    pub title: &'static str,
    pub fields: &'static [FieldSpec],
}

const fn f(
    key: &'static str,
    label: &'static str,
    kind: FieldKind,
    hint: &'static str,
    placeholder: &'static str,
) -> FieldSpec {
    FieldSpec {
        key,
        label,
        kind,
        hint,
        placeholder,
    }
}

/// 常见浏览器首选语言（BCP-47 locale），供 language / languages 下拉选择。
pub const LANGUAGES: &[&str] = &[
    "en-US", "en-GB", "zh-CN", "zh-TW", "ja-JP", "ko-KR", "de-DE", "fr-FR", "es-ES", "pt-BR",
    "pt-PT", "it-IT", "nl-NL", "pl-PL", "ru-RU", "uk-UA", "tr-TR", "ar-SA", "he-IL", "hi-IN",
    "th-TH", "vi-VN", "id-ID", "ms-MY", "sv-SE", "no-NO", "da-DK", "fi-FI", "cs-CZ", "el-GR",
    "hu-HU", "ro-RO",
];

/// 常见 BCP-47 语言码（`locale:language` 裸语言 subtag，供 Intl API）。
pub const LANGUAGES_BARE: &[&str] = &[
    "en", "zh", "ja", "ko", "de", "fr", "es", "pt", "it", "nl", "ru", "uk", "ar", "he", "hi", "th",
    "vi", "id", "ms", "sv", "no", "da", "fi", "cs", "el", "hu", "ro", "pl", "tr", "bg", "hr", "sk",
    "sl", "lt", "lv", "et", "sr", "fa", "bn", "ta", "te", "mr", "gu", "kn", "ml", "pa", "ur", "sw",
    "af", "ca",
];

/// 常见 BCP-47 地区码（`locale:region`，供 Intl API）。
pub const REGIONS: &[&str] = &[
    "US", "GB", "CN", "TW", "HK", "JP", "KR", "DE", "FR", "ES", "PT", "BR", "IT", "NL", "PL", "RU",
    "UA", "AR", "SA", "AE", "EG", "IL", "IN", "TH", "VN", "ID", "MY", "SG", "PH", "SE", "NO", "DK",
    "FI", "CZ", "GR", "HU", "RO", "TR", "AT", "CH", "BE", "CA", "MX", "AU", "NZ", "ZA", "IE", "PK",
    "BD", "KZ",
];

/// 常见 ISO 15924 文字系统码（`locale:script`）。
pub const SCRIPTS: &[&str] = &[
    "Latn", "Cyrl", "Arab", "Hans", "Hant", "Jpan", "Kore", "Grek", "Hebr", "Deva", "Thai", "Geor",
    "Armn", "Hang", "Beng", "Guru", "Gujr", "Taml", "Telu", "Knda", "Mlym", "Sinh",
];

// IANA 时区名（`timezone`，zone1970.tab 权威列表，312 个规范区域）。
include!("timezones.inc");

/// 屏幕颜色/像素位深（BrowserForge 网络真实闭合值：24/30/32）。
pub const COLOR_DEPTH: &[&str] = &["24", "30", "32"];

/// navigator.maxTouchPoints（BrowserForge 网络 12 值全量）。
pub const MAX_TOUCH_POINTS: &[&str] = &[
    "0", "1", "2", "3", "5", "10", "15", "16", "20", "40", "80", "256",
];

/// navigator.hardwareConcurrency（BrowserForge 网络 39 值闭合池）。
pub const HARDWARE_CONCURRENCY: &[&str] = &[
    "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16", "17", "18",
    "19", "20", "21", "22", "23", "24", "25", "26", "27", "28", "30", "32", "36", "40", "44", "48",
    "56", "64", "96", "128", "384", "640",
];

/// 媒体设备数量（mediaDevices:* 计数，常见 0-4）。
pub const DEVICE_COUNTS: &[&str] = &["0", "1", "2", "3", "4"];

/// navigator.appVersion 模板（与 platform 一致的 Firefox 形态）。
pub const APP_VERSIONS: &[&str] = &[
    "5.0 (Macintosh)",
    "5.0 (Windows)",
    "5.0 (X11; Linux x86_64)",
    "5.0 (X11; Linux i686)",
];

/// navigator.buildID 模板（YYYYMMDDHHMMSS，随 ff_version 变化）。
pub const BUILD_IDS: &[&str] = &[
    "20250801000000",
    "20250701000000",
    "20250601000000",
    "20250501000000",
    "20250401000000",
    "20250301000000",
];

pub static NAVIGATOR_FIELDS: &[FieldSpec] = &[
    f(
        "navigator.userAgent",
        "User Agent",
        FieldKind::Multiline,
        "浏览器与系统标识字符串。留空则按目标 OS 自动生成一致 UA。",
        "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:152.0) Gecko/20100101 Firefox/152.0",
    ),
    f(
        "navigator.appVersion",
        "appVersion",
        FieldKind::Select(APP_VERSIONS),
        "浏览器版本信息（与 platform 对应，如 Macintosh/Windows/Linux）。",
        "5.0 (Macintosh)",
    ),
    f(
        "navigator.platform",
        "platform",
        FieldKind::Select(&["Win32", "MacIntel", "Linux x86_64", "Linux i686"]),
        "浏览器运行平台标识，需与 UA 系统一致。",
        "MacIntel",
    ),
    f(
        "navigator.oscpu",
        "oscpu",
        FieldKind::Select(&[
            "Intel Mac OS X 10.15",
            "Windows NT 10.0; Win64; x64",
            "Linux x86_64",
            "Linux i686",
        ]),
        "操作系统与 CPU 信息（Firefox 特有属性），需与 platform 一致。",
        "Intel Mac OS X 10.15",
    ),
    f(
        "navigator.appCodeName",
        "appCodeName",
        FieldKind::Select(&["Mozilla"]),
        "浏览器代码名（Firefox 固定为 Mozilla）。",
        "Mozilla",
    ),
    f(
        "navigator.appName",
        "appName",
        FieldKind::Select(&["Netscape"]),
        "浏览器名称（Firefox 固定为 Netscape）。",
        "Netscape",
    ),
    f(
        "navigator.product",
        "product",
        FieldKind::Select(&["Gecko"]),
        "产品名（Firefox 固定为 Gecko）。",
        "Gecko",
    ),
    f(
        "navigator.productSub",
        "productSub",
        FieldKind::Select(&["20100101", "20030107"]),
        "构建号（现代 Firefox 为 20100101，旧版为 20030107）。",
        "20100101",
    ),
    f(
        "navigator.buildID",
        "buildID",
        FieldKind::Select(BUILD_IDS),
        "浏览器构建标识符（YYYYMMDDHHMMSS）。",
        "20250801000000",
    ),
    f(
        "navigator.language",
        "language",
        FieldKind::Select(LANGUAGES),
        "首选语言（首个 Intl API 语言）。",
        "en-US",
    ),
    f(
        "navigator.languages",
        "languages",
        FieldKind::SelectMulti(LANGUAGES),
        "语言列表（通过下拉添加，标签右侧 × 删除）。",
        "en-US",
    ),
    f(
        "navigator.hardwareConcurrency",
        "逻辑处理器数",
        FieldKind::SelectInt(HARDWARE_CONCURRENCY),
        "navigator.hardwareConcurrency 返回值（BrowserForge 39 值闭合池）。",
        "12",
    ),
    f(
        "navigator.maxTouchPoints",
        "最大触点数",
        FieldKind::SelectInt(MAX_TOUCH_POINTS),
        "同时触摸接触点最大数量（桌面通常为 0）。",
        "0",
    ),
    f(
        "navigator.doNotTrack",
        "doNotTrack",
        FieldKind::Select(&["0", "1", "null"]),
        "跟踪偏好（1=拒绝跟踪，0=允许，null=未设置）。",
        "1",
    ),
    f(
        "navigator.cookieEnabled",
        "cookieEnabled",
        FieldKind::Bool,
        "是否启用 Cookie。",
        "",
    ),
    f(
        "navigator.globalPrivacyControl",
        "globalPrivacyControl",
        FieldKind::Bool,
        "GPC 全局隐私控制信号。",
        "",
    ),
    f(
        "navigator.onLine",
        "onLine",
        FieldKind::Bool,
        "是否在线。",
        "",
    ),
];

pub static WINDOW_FIELDS: &[FieldSpec] = &[
    f(
        "window.innerWidth",
        "视口宽 innerWidth",
        FieldKind::Int,
        "页面视口宽度（px）。",
        "1440",
    ),
    f(
        "window.innerHeight",
        "视口高 innerHeight",
        FieldKind::Int,
        "页面视口高度（px）。",
        "900",
    ),
    f(
        "window.outerWidth",
        "窗口外框宽",
        FieldKind::Int,
        "浏览器窗口外框宽度（px）。",
        "1440",
    ),
    f(
        "window.outerHeight",
        "窗口外框高",
        FieldKind::Int,
        "浏览器窗口外框高度（px，含标题栏/工具栏）。",
        "900",
    ),
    f(
        "window.screenX",
        "窗口 X 坐标",
        FieldKind::SInt,
        "窗口左上角屏幕 X 坐标。",
        "0",
    ),
    f(
        "window.screenY",
        "窗口 Y 坐标",
        FieldKind::SInt,
        "窗口左上角屏幕 Y 坐标。",
        "25",
    ),
    f(
        "window.devicePixelRatio",
        "设备像素比",
        FieldKind::Float,
        "CSS 像素与物理像素之比（Retina=2.0）。",
        "2.0",
    ),
    f(
        "window.history.length",
        "历史长度",
        FieldKind::Int,
        "会话历史条目数。",
        "1",
    ),
    f(
        "window.scrollMinX",
        "最小横向滚动",
        FieldKind::SInt,
        "横向滚动最小偏移。",
        "0",
    ),
    f(
        "window.scrollMinY",
        "最小纵向滚动",
        FieldKind::SInt,
        "纵向滚动最小偏移。",
        "0",
    ),
    f(
        "window.scrollMaxX",
        "最大横向滚动",
        FieldKind::Int,
        "横向滚动最大偏移。",
        "0",
    ),
    f(
        "window.scrollMaxY",
        "最大纵向滚动",
        FieldKind::Int,
        "纵向滚动最大偏移。",
        "0",
    ),
];

pub static SCREEN_FIELDS: &[FieldSpec] = &[
    f(
        "screen.width",
        "屏幕宽",
        FieldKind::Int,
        "屏幕总宽度（px）。",
        "2560",
    ),
    f(
        "screen.height",
        "屏幕高",
        FieldKind::Int,
        "屏幕总高度（px）。",
        "1440",
    ),
    f(
        "screen.availWidth",
        "可用宽",
        FieldKind::Int,
        "去掉系统任务栏后的可用宽度。",
        "2560",
    ),
    f(
        "screen.availHeight",
        "可用高",
        FieldKind::Int,
        "去掉系统任务栏后的可用高度。",
        "1415",
    ),
    f(
        "screen.availLeft",
        "可用区左边界",
        FieldKind::SInt,
        "可用区域左上角 X。",
        "0",
    ),
    f(
        "screen.availTop",
        "可用区上边界",
        FieldKind::SInt,
        "可用区域左上角 Y。",
        "25",
    ),
    f(
        "screen.colorDepth",
        "颜色深度",
        FieldKind::SelectInt(COLOR_DEPTH),
        "屏幕颜色位深（24/30/32）。",
        "24",
    ),
    f(
        "screen.pixelDepth",
        "像素深度",
        FieldKind::SelectInt(COLOR_DEPTH),
        "屏幕像素位深（通常同 colorDepth）。",
        "24",
    ),
    f(
        "screen.pageXOffset",
        "页面横向偏移",
        FieldKind::Float,
        "scrollX 别名。",
        "0",
    ),
    f(
        "screen.pageYOffset",
        "页面纵向偏移",
        FieldKind::Float,
        "scrollY 别名。",
        "0",
    ),
];

pub static DOCUMENT_FIELDS: &[FieldSpec] = &[
    f(
        "document.body.clientWidth",
        "body 宽",
        FieldKind::Int,
        "document.body.clientWidth。",
        "1440",
    ),
    f(
        "document.body.clientHeight",
        "body 高",
        FieldKind::Int,
        "document.body.clientHeight。",
        "900",
    ),
    f(
        "document.body.clientLeft",
        "body 左边框",
        FieldKind::Int,
        "document.body.clientLeft。",
        "0",
    ),
    f(
        "document.body.clientTop",
        "body 上边框",
        FieldKind::Int,
        "document.body.clientTop。",
        "0",
    ),
];

pub static WEBGL_FIELDS: &[FieldSpec] = &[
    f(
        "webGl:vendor",
        "WebGL vendor",
        FieldKind::Text,
        "UNMASKED_VENDOR_WEBGL（显卡厂商）。",
        "Apple",
    ),
    f(
        "webGl:renderer",
        "WebGL renderer",
        FieldKind::Text,
        "UNMASKED_RENDERER_WEBGL（显卡型号）。",
        "Apple M2",
    ),
    f(
        "webGl:contextAttributes",
        "WebGL 上下文属性",
        FieldKind::Json,
        "getContextAttributes() 返回对象。",
        "{\"alpha\":true,\"antialias\":true}",
    ),
    f(
        "webGl:parameters",
        "WebGL 参数",
        FieldKind::Json,
        "各 getParameter 枚举值映射对象。",
        "{}",
    ),
    f(
        "webGl:shaderPrecisionFormats",
        "着色器精度格式",
        FieldKind::Json,
        "getShaderPrecisionFormat 各类型精度。",
        "{}",
    ),
    f(
        "webGl:supportedExtensions",
        "WebGL 扩展列表",
        FieldKind::TextList,
        "逗号分隔的受支持扩展名。",
        "ANGLE_instanced_arrays, EXT_blend_minmax",
    ),
    f(
        "webGl:parameters:blockIfNotDefined",
        "未定义参数屏蔽",
        FieldKind::Bool,
        "未在 parameters 中定义的 getParameter 调用返回 null。",
        "",
    ),
    f(
        "webGl:shaderPrecisionFormats:blockIfNotDefined",
        "未定义精度屏蔽",
        FieldKind::Bool,
        "未定义的精度格式查询返回 null。",
        "",
    ),
    f(
        "webGl2:contextAttributes",
        "WebGL2 上下文属性",
        FieldKind::Json,
        "WebGL2 getContextAttributes()。",
        "{}",
    ),
    f(
        "webGl2:parameters",
        "WebGL2 参数",
        FieldKind::Json,
        "WebGL2 各 getParameter 值（含 VERSION/SHADING_LANGUAGE_VERSION）。",
        "{}",
    ),
    f(
        "webGl2:shaderPrecisionFormats",
        "WebGL2 精度格式",
        FieldKind::Json,
        "WebGL2 着色器精度。",
        "{}",
    ),
    f(
        "webGl2:supportedExtensions",
        "WebGL2 扩展列表",
        FieldKind::TextList,
        "逗号分隔 WebGL2 扩展名。",
        "",
    ),
    f(
        "webGl2:parameters:blockIfNotDefined",
        "WebGL2 未定义参数屏蔽",
        FieldKind::Bool,
        "同 WebGL1 屏蔽开关。",
        "",
    ),
    f(
        "webGl2:shaderPrecisionFormats:blockIfNotDefined",
        "WebGL2 未定义精度屏蔽",
        FieldKind::Bool,
        "同 WebGL1 屏蔽开关。",
        "",
    ),
];

pub static MEDIA_AUDIO_FIELDS: &[FieldSpec] = &[
    f(
        "AudioContext:sampleRate",
        "采样率",
        FieldKind::SelectInt(&["44100", "48000", "96000"]),
        "AudioContext.sampleRate（常见 44100/48000）。",
        "48000",
    ),
    f(
        "AudioContext:maxChannelCount",
        "最大声道数",
        FieldKind::SelectInt(&["2", "6", "8"]),
        "AudioContext.destination.maxChannelCount（2=立体声，6=5.1，8=7.1）。",
        "2",
    ),
    f(
        "AudioContext:outputLatency",
        "输出延迟",
        FieldKind::Float,
        "AudioContext.outputLatency（毫秒）。",
        "0.0232",
    ),
    f(
        "mediaDevices:enabled",
        "媒体设备可用",
        FieldKind::Bool,
        "enumerateDevices 是否返回设备。",
        "",
    ),
    f(
        "mediaDevices:micros",
        "麦克风数量",
        FieldKind::SelectInt(DEVICE_COUNTS),
        "enumerateDevices 暴露的音频输入设备数量（0-4）。",
        "1",
    ),
    f(
        "mediaDevices:speakers",
        "扬声器数量",
        FieldKind::SelectInt(DEVICE_COUNTS),
        "enumerateDevices 暴露的音频输出设备数量（0-4）。",
        "0",
    ),
    f(
        "mediaDevices:webcams",
        "摄像头数量",
        FieldKind::SelectInt(DEVICE_COUNTS),
        "enumerateDevices 暴露的视频输入设备数量（0-4）。",
        "1",
    ),
];

pub static WEBRTC_FIELDS: &[FieldSpec] = &[
    f(
        "webrtc:ipv4",
        "WebRTC IPv4",
        FieldKind::Text,
        "RTCPeerConnection 报告的本机 IPv4 地址。",
        "192.168.1.42",
    ),
    f(
        "webrtc:ipv6",
        "WebRTC IPv6",
        FieldKind::Text,
        "RTCPeerConnection 报告的本机 IPv6 地址。",
        "",
    ),
];

pub static GEO_INTL_FIELDS: &[FieldSpec] = &[
    f(
        "geolocation:latitude",
        "纬度",
        FieldKind::Float,
        "地理位置纬度。开启 GeoIP 时自动计算。",
        "37.7749",
    ),
    f(
        "geolocation:longitude",
        "经度",
        FieldKind::Float,
        "地理位置经度。开启 GeoIP 时自动计算。",
        "-122.4194",
    ),
    f(
        "geolocation:accuracy",
        "定位精度(米)",
        FieldKind::Float,
        "不填则按经纬度小数位自动推算。",
        "100",
    ),
    f(
        "timezone",
        "时区",
        FieldKind::Select(TIMEZONES),
        "IANA 时区名，同时影响 Date() 本地时间。",
        "America/Los_Angeles",
    ),
    f(
        "locale:language",
        "Intl 语言",
        FieldKind::Select(LANGUAGES_BARE),
        "Intl API 语言代码（裸 subtag，如 en/zh）。",
        "en",
    ),
    f(
        "locale:region",
        "Intl 地区",
        FieldKind::Select(REGIONS),
        "Intl API 地区代码（如 US/CN）。",
        "US",
    ),
    f(
        "locale:script",
        "Intl 文字系统",
        FieldKind::Select(SCRIPTS),
        "书写系统（如 Latn），留空自动。",
        "Latn",
    ),
    f(
        "locale:all",
        "接受语言列表",
        FieldKind::Text,
        "逗号分隔全部 locale，首项应与 Intl 一致。",
        "en-US, en",
    ),
];

/// 常见 Accept-Encoding 组合（现代浏览器固定值池）。
pub const ACCEPT_ENCODINGS: &[&str] = &[
    "gzip, deflate, br, zstd",
    "gzip, deflate, br",
    "gzip, deflate",
    "gzip, deflate, zstd",
    "gzip",
    "gzip, br",
    "gzip, deflate, br, zstd, q=1.0, *;q=0.5",
];

/// 常见 Accept-Language 模板（`en-US,en;q=0.9` 形态，随 language 变化）。
pub const ACCEPT_LANGUAGES: &[&str] = &[
    "en-US,en;q=0.9",
    "zh-CN,zh;q=0.9,en;q=0.8",
    "zh-TW,zh;q=0.9,en;q=0.8",
    "ja-JP,ja;q=0.9,en;q=0.8",
    "ko-KR,ko;q=0.9,en;q=0.8",
    "de-DE,de;q=0.9,en;q=0.8",
    "fr-FR,fr;q=0.9,en;q=0.8",
    "es-ES,es;q=0.9,en;q=0.8",
    "en-GB,en;q=0.9",
];

pub static HEADER_FIELDS: &[FieldSpec] = &[
    f(
        "headers.User-Agent",
        "HTTP UA 头",
        FieldKind::Multiline,
        "HTTP 请求头中的 User-Agent（通常与 navigator.userAgent 联动）。",
        "",
    ),
    f(
        "headers.Accept-Language",
        "Accept-Language",
        FieldKind::Select(ACCEPT_LANGUAGES),
        "HTTP 接受语言头。",
        "en-US,en;q=0.9",
    ),
    f(
        "headers.Accept-Encoding",
        "Accept-Encoding",
        FieldKind::Select(ACCEPT_ENCODINGS),
        "HTTP 接受编码头。",
        "gzip, deflate, br, zstd",
    ),
];

pub static FONT_VOICE_FIELDS: &[FieldSpec] = &[
    f(
        "fonts",
        "字体列表",
        FieldKind::TextList,
        "逗号分隔字体名。留空则按目标 OS 生成随机子集。",
        "Helvetica, Georgia, Times New Roman",
    ),
    f(
        "fonts:spacing_seed",
        "字体间距种子",
        FieldKind::Int,
        "字体度量扰动的随机种子（1..2^32-1）。",
        "123456789",
    ),
    f(
        "voices",
        "语音列表",
        FieldKind::TextList,
        "逗号分隔 speechSynthesis 语音名。留空按 OS 随机生成。",
        "Alex, Daniel",
    ),
    f(
        "voices:blockIfNotDefined",
        "未定义语音屏蔽",
        FieldKind::Bool,
        "未列出的语音不出现在 getVoices()。",
        "",
    ),
    f(
        "voices:fakeCompletion",
        "语音合成假完成",
        FieldKind::Bool,
        "speak() 立即触发合成完成事件。",
        "",
    ),
    f(
        "voices:fakeCompletion:charsPerSecond",
        "假完成速率",
        FieldKind::Float,
        "按每秒字符数调度完成事件。",
        "15",
    ),
];

pub static BATTERY_FIELDS: &[FieldSpec] = &[
    f(
        "battery:charging",
        "充电中",
        FieldKind::Bool,
        "BatteryManager.charging。",
        "",
    ),
    f(
        "battery:chargingTime",
        "充满剩余(秒)",
        FieldKind::Float,
        "chargingTime（Infinity=不可充电）。",
        "3600",
    ),
    f(
        "battery:dischargingTime",
        "耗尽剩余(秒)",
        FieldKind::Float,
        "dischargingTime。",
        "14400",
    ),
    f(
        "battery:level",
        "电量",
        FieldKind::Float,
        "电量百分比（0.0-1.0）。",
        "0.85",
    ),
];

pub static HUMANIZE_FIELDS: &[FieldSpec] = &[
    f(
        "humanize:minTime",
        "光标最短时间(秒)",
        FieldKind::Float,
        "人性化光标移动最短时长。",
        "0.5",
    ),
    f(
        "humanize:maxTime",
        "光标最长时间(秒)",
        FieldKind::Float,
        "人性化光标移动最长时长。",
        "1.5",
    ),
    f(
        "showcursor",
        "显示光标",
        FieldKind::Bool,
        "是否显示浏览器光标。",
        "",
    ),
];

pub static MISC_FIELDS: &[FieldSpec] = &[f(
    "allowMainWorld",
    "主世界执行",
    FieldKind::Bool,
    "允许在页面主世界执行脚本（page.evaluate 加 mw: 前缀）。",
    "",
)];

/// 全部分组（UI Tab 顺序）。
pub static FIELD_GROUPS: &[FieldGroup] = &[
    FieldGroup {
        id: "navigator",
        title: "Navigator",
        fields: NAVIGATOR_FIELDS,
    },
    FieldGroup {
        id: "window",
        title: "Window",
        fields: WINDOW_FIELDS,
    },
    FieldGroup {
        id: "screen",
        title: "Screen",
        fields: SCREEN_FIELDS,
    },
    FieldGroup {
        id: "document",
        title: "Document",
        fields: DOCUMENT_FIELDS,
    },
    FieldGroup {
        id: "webgl",
        title: "WebGL",
        fields: WEBGL_FIELDS,
    },
    FieldGroup {
        id: "media_audio",
        title: "媒体与音频",
        fields: MEDIA_AUDIO_FIELDS,
    },
    FieldGroup {
        id: "webrtc",
        title: "WebRTC",
        fields: WEBRTC_FIELDS,
    },
    FieldGroup {
        id: "geo_intl",
        title: "地理与时区",
        fields: GEO_INTL_FIELDS,
    },
    FieldGroup {
        id: "headers",
        title: "HTTP 头",
        fields: HEADER_FIELDS,
    },
    FieldGroup {
        id: "fonts_voices",
        title: "字体与语音",
        fields: FONT_VOICE_FIELDS,
    },
    FieldGroup {
        id: "battery",
        title: "电池",
        fields: BATTERY_FIELDS,
    },
    FieldGroup {
        id: "humanize",
        title: "光标人性化",
        fields: HUMANIZE_FIELDS,
    },
    FieldGroup {
        id: "misc",
        title: "杂项",
        fields: MISC_FIELDS,
    },
];

impl FieldSpec {
    /// 是否为多值/复杂类型（JSON 面板编辑更方便）。
    pub fn is_complex(&self) -> bool {
        matches!(self.kind, FieldKind::Json)
    }
}

pub fn find_spec(key: &str) -> Option<&'static FieldSpec> {
    FIELD_GROUPS
        .iter()
        .flat_map(|g| g.fields.iter())
        .find(|f| f.key == key)
}

pub fn group_by_id(id: &str) -> Option<&'static FieldGroup> {
    FIELD_GROUPS.iter().find(|g| g.id == id)
}

pub fn registered_key_count() -> usize {
    FIELD_GROUPS.iter().map(|g| g.fields.len()).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_no_duplicate_keys() {
        let mut keys: Vec<&str> = FIELD_GROUPS
            .iter()
            .flat_map(|g| g.fields.iter().map(|f| f.key))
            .collect();
        keys.sort();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate keys exist");
    }

    #[test]
    fn select_options_have_no_duplicates() {
        for g in FIELD_GROUPS {
            for f in g.fields {
                let options = match f.kind {
                    FieldKind::Select(o) | FieldKind::SelectInt(o) | FieldKind::SelectMulti(o) => o,
                    _ => continue,
                };
                let mut seen = std::collections::HashSet::new();
                for v in options {
                    assert!(seen.insert(v), "duplicate option {v:?} in {}", f.key);
                }
            }
        }
    }

    #[test]
    fn registry_covers_documented_keys() {
        // 关键键抽样断言，防止重构时丢字段
        for k in [
            "navigator.userAgent",
            "navigator.platform",
            "navigator.oscpu",
            "navigator.hardwareConcurrency",
            "screen.width",
            "window.devicePixelRatio",
            "webGl:vendor",
            "webGl:renderer",
            "webGl2:parameters",
            "geolocation:latitude",
            "timezone",
            "locale:language",
            "headers.User-Agent",
            "fonts",
            "voices",
            "battery:level",
            "humanize:maxTime",
            "webrtc:ipv4",
            "AudioContext:sampleRate",
            "mediaDevices:webcams",
            "document.body.clientWidth",
        ] {
            assert!(find_spec(k).is_some(), "missing key: {k}");
        }
        assert!(registered_key_count() >= 85);
    }
}
