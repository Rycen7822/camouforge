use camoforge_protocol::Profile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum F {
    Os,
    Headless,
    ColorScheme,
    BlockImages,
    BlockWebrtc,
    BlockWebgl,
    DisableCoop,
    GeoipMode,
    GeoipIp,
    ProxyServer,
    ProxyUser,
    ProxyPass,
    ProxyPort,
    HumanizeMode,
    HumanizeMaxTime,
    WebglVendor,
    WebglRenderer,
    ScreenSpec,
    ScreenWH,
    ScreenMinMax,
    Window,
    Locale,
    Persistent,
    UserDataDir,
    DownloadsDir,
    Fingerprint,
    FfVersion,
    Addons,
    ExcludeAddons,
    Fonts,
    CustomFontsOnly,
    FpPreset,
    GeoipDb,
    MainWorldEval,
    EnableCache,
    DebugMode,
    IKnowWhat,
    ExecutablePath,
    Browser,
    FirefoxUserPrefs,
    Args,
    Env,
    ExtraLaunchOptions,
}

pub(super) struct Spec {
    pub(super) field: F,
    pub(super) label: &'static str,
    pub(super) hint: &'static str,
    pub(super) placeholder: &'static str,
    pub(super) enum_options: Option<&'static [&'static str]>,
    pub(super) is_bool: bool,
    pub(super) group: &'static str,
}

pub(super) const SPECS: &[Spec] = &[
    Spec { field: F::Os, label: "操作系统", hint: "(os) 伪装 OS，可多值随机：macos,windows,linux（留空=OS 指纹内决定）", placeholder: "macos", enum_options: None, is_bool: false, group: "系统" },
    Spec { field: F::Headless, label: "显示模式", hint: "(headless) headed=有界面 / headless=无界面 / virtual=虚拟显示(仅 Linux)", placeholder: "headed", enum_options: Some(&["headed", "headless", "virtual"]), is_bool: false, group: "系统" },
    Spec { field: F::ColorScheme, label: "颜色方案", hint: "(color_scheme) prefers-color-scheme：auto=驱动默认 / dark / light（暗色主题站点需 dark 才渲染深色）", placeholder: "auto", enum_options: Some(&["auto", "dark", "light"]), is_bool: false, group: "系统" },
    Spec { field: F::BlockImages, label: "拦截图片", hint: "(block_images)", placeholder: "false", enum_options: None, is_bool: true, group: "隐私拦截" },
    Spec { field: F::BlockWebrtc, label: "拦截 WebRTC", hint: "(block_webrtc) 等价 webrtc:ipv4 为空", placeholder: "false", enum_options: None, is_bool: true, group: "隐私拦截" },
    Spec { field: F::BlockWebgl, label: "拦截 WebGL", hint: "(block_webgl)", placeholder: "false", enum_options: None, is_bool: true, group: "隐私拦截" },
    Spec { field: F::DisableCoop, label: "禁用 COOP", hint: "(disable_coop) 允许跨窗口引用", placeholder: "false", enum_options: None, is_bool: true, group: "隐私拦截" },
    Spec { field: F::GeoipMode, label: "GeoIP 模式", hint: "(geoip) off=禁用 / auto=按代理 IP / ip=指定 IP", placeholder: "off", enum_options: Some(&["off", "auto", "ip"]), is_bool: false, group: "地理位置" },
    Spec { field: F::GeoipIp, label: "指定 IP", hint: "GeoIP 模式为 ip 时填写，如 123.45.67.89", placeholder: "", enum_options: None, is_bool: false, group: "地理位置" },
    Spec { field: F::ProxyServer, label: "代理服务器", hint: "如 http://host:port 或 socks5://host:port（清空=直连）", placeholder: "", enum_options: None, is_bool: false, group: "代理" },
    Spec { field: F::ProxyUser, label: "代理用户名", hint: "", placeholder: "", enum_options: None, is_bool: false, group: "代理" },
    Spec { field: F::ProxyPass, label: "代理密码", hint: "明文保存在本地 profiles/*.json，请勿复用重要账号密码", placeholder: "", enum_options: None, is_bool: false, group: "代理" },
    Spec { field: F::ProxyPort, label: "端口伪装", hint: "(port) 伪装本地端口，防代理端口泄漏，如 65432", placeholder: "", enum_options: None, is_bool: false, group: "代理" },
    Spec { field: F::HumanizeMode, label: "人类化模式", hint: "(humanize) off / on / custom（启用手动参数）", placeholder: "off", enum_options: Some(&["off", "on", "custom"]), is_bool: false, group: "人类化" },
    Spec { field: F::HumanizeMaxTime, label: "最大耗时(秒)", hint: "custom 模式下鼠标移动最大时间，如 2.5", placeholder: "", enum_options: None, is_bool: false, group: "人类化" },
    Spec { field: F::WebglVendor, label: "WebGL 厂商", hint: "(webgl vendor) 须为 SDK 数据库精确值（用下方 WebGL 组合列表）", placeholder: "Apple", enum_options: None, is_bool: false, group: "WebGL" },
    Spec { field: F::WebglRenderer, label: "WebGL 渲染器", hint: "(webgl renderer) 如 Apple M1", placeholder: "", enum_options: None, is_bool: false, group: "WebGL" },
    Spec { field: F::ScreenSpec, label: "屏幕约束模式", hint: "(screen) off / exact=精确 / range=范围", placeholder: "off", enum_options: Some(&["off", "exact", "range"]), is_bool: false, group: "屏幕与窗口" },
    Spec { field: F::ScreenWH, label: "屏幕尺寸 (exact)", hint: "exact 模式：宽,高 如 1920,1080", placeholder: "", enum_options: None, is_bool: false, group: "屏幕与窗口" },
    Spec { field: F::ScreenMinMax, label: "屏幕范围 (range)", hint: "range 模式：minW,minH,maxW,maxH", placeholder: "", enum_options: None, is_bool: false, group: "屏幕与窗口" },
    Spec { field: F::Window, label: "窗口尺寸", hint: "(window) 初始窗口 宽,高 如 1280,800", placeholder: "", enum_options: None, is_bool: false, group: "屏幕与窗口" },
    Spec { field: F::Locale, label: "语言", hint: "(locale) 逗号分隔，如 zh-CN,zh（影响 Accept-Language）", placeholder: "", enum_options: None, is_bool: false, group: "语言与持久化" },
    Spec { field: F::Persistent, label: "持久化上下文", hint: "(persistent) 保存历史/cookie 到数据目录", placeholder: "false", enum_options: None, is_bool: true, group: "语言与持久化" },
    Spec { field: F::UserDataDir, label: "数据目录", hint: "持久化目录（留空=自动）", placeholder: "", enum_options: None, is_bool: false, group: "语言与持久化" },
    Spec { field: F::DownloadsDir, label: "下载目录", hint: "(downloads_dir) 浏览器下载文件保存目录；留空=系统「下载」目录", placeholder: "", enum_options: None, is_bool: false, group: "下载" },
    Spec { field: F::Fingerprint, label: "指纹来源", hint: "(fingerprint) browserforge 完整 JSON（点「生成指纹」自动填充，最高优先级）", placeholder: "", enum_options: None, is_bool: false, group: "指纹" },
    Spec { field: F::FfVersion, label: "Firefox 版本", hint: "(ff_version) 固定 UA 版本号，如 135.0.1", placeholder: "", enum_options: None, is_bool: false, group: "指纹" },
];

pub(super) const ADVANCED_SPECS: &[Spec] = &[
    Spec {
        field: F::Addons,
        label: "加载 addons",
        hint: "(addons) 逗号分隔的 addon 名/路径",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "Addons",
    },
    Spec {
        field: F::ExcludeAddons,
        label: "排除 addons",
        hint: "(exclude_addons) 从默认 addons 中排除",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "Addons",
    },
    Spec {
        field: F::Fonts,
        label: "注入字体",
        hint: "(fonts) 逗号分隔字体名，如 Arial,Times New Roman",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "字体",
    },
    Spec {
        field: F::CustomFontsOnly,
        label: "仅自定义字体",
        hint: "(custom_fonts_only) 只加载 fonts 声明字体",
        placeholder: "false",
        enum_options: None,
        is_bool: true,
        group: "字体",
    },
    Spec {
        field: F::FpPreset,
        label: "指纹预设",
        hint: "(fingerprint_preset) off / random / browserforge JSON",
        placeholder: "off",
        enum_options: Some(&["off", "random"]),
        is_bool: false,
        group: "指纹预设",
    },
    Spec {
        field: F::GeoipDb,
        label: "GeoIP 数据库",
        hint: "(geoip_db) GeoIP 数据库名：MaxMind GeoLite2 / GeoIP AIO by daijro",
        placeholder: "MaxMind GeoLite2",
        enum_options: Some(&["MaxMind GeoLite2", "GeoIP AIO by daijro"]),
        is_bool: false,
        group: "调试与注入",
    },
    Spec {
        field: F::MainWorldEval,
        label: "主世界执行",
        hint: "(main_world_eval) 允许脚本主世界执行（安全风险）",
        placeholder: "false",
        enum_options: None,
        is_bool: true,
        group: "调试与注入",
    },
    Spec {
        field: F::EnableCache,
        label: "启用缓存",
        hint: "(enable_cache)",
        placeholder: "false",
        enum_options: None,
        is_bool: true,
        group: "调试与注入",
    },
    Spec {
        field: F::DebugMode,
        label: "调试模式",
        hint: "(debug) 输出详细调试日志",
        placeholder: "false",
        enum_options: None,
        is_bool: true,
        group: "调试与注入",
    },
    Spec {
        field: F::IKnowWhat,
        label: "确认风险",
        hint: "(i_know_what_im_doing) SDK 危险参数开关",
        placeholder: "false",
        enum_options: None,
        is_bool: true,
        group: "调试与注入",
    },
    Spec {
        field: F::ExecutablePath,
        label: "Firefox 路径",
        hint: "(executable_path) 自定义 Firefox 二进制（留空自动探测）",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "浏览器",
    },
    Spec {
        field: F::Browser,
        label: "浏览器版本",
        hint: "(browser) 已装版本：official/stable、beta.20、134.0.2-beta.20（留空=默认活动版本）",
        placeholder: "official/stable",
        enum_options: None,
        is_bool: false,
        group: "浏览器",
    },
    Spec {
        field: F::FirefoxUserPrefs,
        label: "用户首选项",
        hint: "(firefox_user_prefs) JSON，注入 about:config",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "高级注入",
    },
    Spec {
        field: F::Args,
        label: "启动参数",
        hint: "(args) 逗号分隔",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "高级注入",
    },
    Spec {
        field: F::Env,
        label: "环境变量",
        hint: "(env) JSON 对象，如 {\"FOO\":\"bar\"}",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "高级注入",
    },
    Spec {
        field: F::ExtraLaunchOptions,
        label: "额外选项",
        hint: "(extra_launch_options) JSON，合并进 launch kwargs",
        placeholder: "",
        enum_options: None,
        is_bool: false,
        group: "高级注入",
    },
];

pub(super) fn get_text(p: &Profile, f: F) -> String {
    use camoforge_protocol::*;
    let l = &p.launch;
    match f {
        F::Os => l.os.clone().map(|v| v.join(",")).unwrap_or_default(),
        F::Headless => match l.headless {
            HeadlessMode::Headed => "headed".into(),
            HeadlessMode::Headless => "headless".into(),
            HeadlessMode::Virtual => "virtual".into(),
        },
        F::ColorScheme => l.color_scheme.clone().unwrap_or_else(|| "auto".into()),
        F::BlockImages => l.block_images.map(|b| b.to_string()).unwrap_or_default(),
        F::BlockWebrtc => l.block_webrtc.map(|b| b.to_string()).unwrap_or_default(),
        F::BlockWebgl => l.block_webgl.map(|b| b.to_string()).unwrap_or_default(),
        F::DisableCoop => l.disable_coop.map(|b| b.to_string()).unwrap_or_default(),
        F::GeoipMode => match &l.geoip {
            Some(GeoipMode::Ip { .. }) => "ip".into(),
            Some(GeoipMode::Auto) => "auto".into(),
            Some(GeoipMode::Off) | None => "off".into(),
        },
        F::GeoipIp => match &l.geoip {
            Some(GeoipMode::Ip { ip }) => ip.clone(),
            _ => String::new(),
        },
        F::ProxyServer => l
            .proxy
            .as_ref()
            .map(|x| x.server.clone())
            .unwrap_or_default(),
        F::ProxyUser => l
            .proxy
            .as_ref()
            .and_then(|x| x.username.clone())
            .unwrap_or_default(),
        F::ProxyPass => l
            .proxy
            .as_ref()
            .and_then(|x| x.password.clone())
            .unwrap_or_default(),
        F::ProxyPort => l
            .proxy
            .as_ref()
            .and_then(|x| x.port)
            .map(|p| p.to_string())
            .unwrap_or_default(),
        F::HumanizeMode => match &l.humanize {
            Some(HumanizeMode::On) => "on".into(),
            Some(HumanizeMode::Custom { .. }) => "custom".into(),
            _ => "off".into(),
        },
        F::HumanizeMaxTime => match &l.humanize {
            Some(HumanizeMode::Custom { max_time }) => max_time.to_string(),
            _ => String::new(),
        },
        F::WebglVendor => l
            .webgl_config
            .as_ref()
            .map(|w| w.vendor.clone())
            .unwrap_or_default(),
        F::WebglRenderer => l
            .webgl_config
            .as_ref()
            .map(|w| w.renderer.clone())
            .unwrap_or_default(),
        F::ScreenSpec => match &l.screen {
            Some(ScreenSpec::Exact { .. }) => "exact".into(),
            Some(ScreenSpec::Range { .. }) => "range".into(),
            None => "off".into(),
        },
        F::ScreenWH => match &l.screen {
            Some(ScreenSpec::Exact { width, height }) => format!("{width},{height}"),
            _ => String::new(),
        },
        F::ScreenMinMax => match &l.screen {
            Some(ScreenSpec::Range {
                min_width,
                min_height,
                max_width,
                max_height,
            }) => {
                format!("{min_width},{min_height},{max_width},{max_height}")
            }
            _ => String::new(),
        },
        F::Window => l
            .window
            .map(|(w, h)| format!("{w},{h}"))
            .unwrap_or_default(),
        F::Locale => l.locale.clone().map(|v| v.join(",")).unwrap_or_default(),
        F::Persistent => l
            .persistent_context
            .map(|b| b.to_string())
            .unwrap_or_default(),
        F::UserDataDir => l.user_data_dir.clone().unwrap_or_default(),
        F::DownloadsDir => l.downloads_dir.clone().unwrap_or_default(),
        F::Fingerprint => l
            .fingerprint
            .as_ref()
            .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
            .unwrap_or_default(),
        F::FfVersion => l.ff_version.map(|v| v.to_string()).unwrap_or_default(),
        F::Addons => l.addons.join(","),
        F::ExcludeAddons => l.exclude_addons.join(","),
        F::Fonts => l.fonts.clone().map(|v| v.join(",")).unwrap_or_default(),
        F::CustomFontsOnly => l
            .custom_fonts_only
            .map(|b| b.to_string())
            .unwrap_or_default(),
        F::FpPreset => match &l.fingerprint_preset {
            Some(FingerprintPreset::Random) => "random".into(),
            Some(FingerprintPreset::Value { value }) => {
                serde_json::to_string_pretty(value).unwrap_or_default()
            }
            _ => "off".into(),
        },
        F::GeoipDb => l.geoip_db.clone().unwrap_or_default(),
        F::MainWorldEval => l.main_world_eval.map(|b| b.to_string()).unwrap_or_default(),
        F::EnableCache => l.enable_cache.map(|b| b.to_string()).unwrap_or_default(),
        F::DebugMode => l.debug.map(|b| b.to_string()).unwrap_or_default(),
        F::IKnowWhat => l
            .i_know_what_im_doing
            .map(|b| b.to_string())
            .unwrap_or_default(),
        F::ExecutablePath => l.executable_path.clone().unwrap_or_default(),
        F::Browser => l.browser.clone().unwrap_or_default(),
        F::FirefoxUserPrefs => json_map_text(&l.firefox_user_prefs),
        F::Args => l.args.join(","),
        F::Env => json_map_text(&l.env),
        F::ExtraLaunchOptions => json_map_text(&l.extra_launch_options),
    }
}

fn json_map_text(map: &camoforge_protocol::JsonMap) -> String {
    if map.is_empty() {
        String::new()
    } else {
        serde_json::to_string_pretty(map).unwrap_or_default()
    }
}

pub(super) fn set_text(p: &mut Profile, f: F, text: &str) -> Result<(), String> {
    use camoforge_protocol::*;
    let t = text.trim();
    let l = &mut p.launch;
    let parse_bool = |t: &str, name: &str| -> Result<bool, String> {
        match t {
            "" | "false" | "0" => Ok(false),
            "true" | "1" => Ok(true),
            other => Err(format!("{name}: 应为 true/false，得到 {other:?}")),
        }
    };
    let parse_list = |t: &str| -> Option<Vec<String>> {
        if t.is_empty() {
            None
        } else {
            Some(
                t.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            )
        }
    };
    match f {
        F::Os => {
            l.os = parse_list(t);
            if let Some(os) = &l.os {
                for o in os {
                    if !["windows", "macos", "linux"].contains(&o.as_str()) {
                        return Err(format!("os: {o:?} 无效（windows/macos/linux）"));
                    }
                }
            }
        }
        F::Headless => {
            l.headless = match t {
                "headless" => HeadlessMode::Headless,
                "virtual" => HeadlessMode::Virtual,
                _ => HeadlessMode::Headed,
            };
        }
        F::ColorScheme => {
            l.color_scheme = if t.is_empty() || t == "auto" {
                None
            } else {
                Some(t.to_string())
            };
        }
        F::BlockImages => {
            l.block_images = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "block_images")?)
            };
        }
        F::BlockWebrtc => {
            l.block_webrtc = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "block_webrtc")?)
            };
        }
        F::BlockWebgl => {
            l.block_webgl = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "block_webgl")?)
            };
        }
        F::DisableCoop => {
            l.disable_coop = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "disable_coop")?)
            }
        }
        F::GeoipMode => {
            l.geoip = match t {
                "auto" => Some(GeoipMode::Auto),
                "ip" => {
                    let ip = match &l.geoip {
                        Some(GeoipMode::Ip { ip }) => ip.clone(),
                        _ => String::new(),
                    };
                    Some(GeoipMode::Ip { ip })
                }
                _ => Some(GeoipMode::Off),
            };
        }
        F::GeoipIp => {
            if t.is_empty() {
                // 清空 IP = 退出 ip 模式（否则旧 IP 不可见却仍生效）；非 ip 模式不动。
                if matches!(l.geoip, Some(GeoipMode::Ip { .. })) {
                    l.geoip = Some(GeoipMode::Off);
                }
            } else {
                l.geoip = Some(GeoipMode::Ip { ip: t.to_string() });
            }
        }
        F::ProxyServer => {
            if t.is_empty() {
                l.proxy = None;
            } else {
                let scheme = t.split("://").next().unwrap_or("");
                if !["http", "https", "socks4", "socks5"].contains(&scheme) {
                    return Err(format!(
                        "代理服务器: 需要 http:// / https:// / socks5:// 前缀，得到 {t:?}"
                    ));
                }
                let proxy = l.proxy.take().unwrap_or_default();
                l.proxy = Some(Proxy {
                    server: t.to_string(),
                    ..proxy
                });
            }
        }
        F::ProxyUser => {
            let proxy = l.proxy.take().unwrap_or_default();
            l.proxy = Some(Proxy {
                username: if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                },
                ..proxy
            });
        }
        F::ProxyPass => {
            let proxy = l.proxy.take().unwrap_or_default();
            l.proxy = Some(Proxy {
                password: if t.is_empty() {
                    None
                } else {
                    Some(t.to_string())
                },
                ..proxy
            });
        }
        F::ProxyPort => {
            let proxy = l.proxy.take().unwrap_or_default();
            l.proxy = Some(Proxy {
                port: if t.is_empty() {
                    None
                } else {
                    Some(
                        t.parse::<u32>()
                            .map_err(|_| format!("代理端口: {t:?} 不是数字"))?,
                    )
                },
                ..proxy
            });
        }
        F::HumanizeMode => {
            l.humanize = match t {
                "custom" => {
                    let max = match &l.humanize {
                        Some(HumanizeMode::Custom { max_time }) => *max_time,
                        _ => 1.0,
                    };
                    Some(HumanizeMode::Custom { max_time: max })
                }
                "on" => Some(HumanizeMode::On),
                _ => None,
            };
        }
        F::HumanizeMaxTime => {
            let v: f32 = t.parse().map_err(|_| format!("耗时: {t:?} 不是数字"))?;
            l.humanize = Some(HumanizeMode::Custom { max_time: v });
        }
        F::WebglVendor => {
            if t.is_empty() {
                l.webgl_config = None;
            } else {
                let w = l.webgl_config.take().unwrap_or(WebglConfig {
                    vendor: String::new(),
                    renderer: String::new(),
                });
                l.webgl_config = Some(WebglConfig {
                    vendor: t.to_string(),
                    ..w
                });
            }
        }
        F::WebglRenderer => {
            if t.is_empty() {
                l.webgl_config = None;
            } else {
                let w = l.webgl_config.take().unwrap_or(WebglConfig {
                    vendor: String::new(),
                    renderer: String::new(),
                });
                l.webgl_config = Some(WebglConfig {
                    renderer: t.to_string(),
                    ..w
                });
            }
        }
        F::ScreenSpec => {
            l.screen = match t {
                "exact" => {
                    let (width, height) = match l.screen.take() {
                        Some(ScreenSpec::Exact { width, height }) => (width, height),
                        _ => (1920, 1080),
                    };
                    Some(ScreenSpec::Exact { width, height })
                }
                "range" => {
                    let (min_width, min_height, max_width, max_height) = match l.screen.take() {
                        Some(ScreenSpec::Range {
                            min_width,
                            min_height,
                            max_width,
                            max_height,
                        }) => (min_width, min_height, max_width, max_height),
                        _ => (1280, 720, 1920, 1080),
                    };
                    Some(ScreenSpec::Range {
                        min_width,
                        min_height,
                        max_width,
                        max_height,
                    })
                }
                _ => None,
            };
        }
        F::ScreenWH => {
            if !t.is_empty() {
                let parts: Vec<u32> = t
                    .split(',')
                    .map(|s| {
                        s.trim().parse::<u32>().map_err(|_| {
                            format!("屏幕尺寸 (exact): 需要「宽,高」两个数字，得到 {t:?}")
                        })
                    })
                    .collect::<Result<_, String>>()?;
                if parts.len() != 2 {
                    return Err(format!(
                        "屏幕尺寸 (exact): 需要「宽,高」两个数字，得到 {t:?}"
                    ));
                }
                l.screen = Some(ScreenSpec::Exact {
                    width: parts[0],
                    height: parts[1],
                });
            }
        }
        F::ScreenMinMax => {
            if !t.is_empty() {
                let parts: Vec<u32> = t
                    .split(',')
                    .map(|s| {
                        s.trim().parse::<u32>().map_err(|_| {
                            format!(
                                "屏幕范围 (range): 需要「minW,minH,maxW,maxH」四个数字，得到 {t:?}"
                            )
                        })
                    })
                    .collect::<Result<_, String>>()?;
                if parts.len() != 4 {
                    return Err(format!(
                        "屏幕范围 (range): 需要「minW,minH,maxW,maxH」四个数字，得到 {t:?}"
                    ));
                }
                l.screen = Some(ScreenSpec::Range {
                    min_width: parts[0],
                    min_height: parts[1],
                    max_width: parts[2],
                    max_height: parts[3],
                });
            }
        }
        F::Window => {
            if !t.is_empty() {
                let parts: Vec<u32> = t
                    .split(',')
                    .map(|s| {
                        s.trim()
                            .parse::<u32>()
                            .map_err(|_| format!("窗口尺寸: 需要「宽,高」两个数字，得到 {t:?}"))
                    })
                    .collect::<Result<_, String>>()?;
                if parts.len() != 2 {
                    return Err(format!("窗口尺寸: 需要「宽,高」两个数字，得到 {t:?}"));
                }
                l.window = Some((parts[0], parts[1]));
            } else {
                l.window = None;
            }
        }
        F::Locale => l.locale = parse_list(t),
        F::Persistent => {
            l.persistent_context = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "persistent_context")?)
            }
        }
        F::UserDataDir => {
            l.user_data_dir = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        F::DownloadsDir => {
            l.downloads_dir = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        F::Fingerprint => {
            if t.trim().is_empty() {
                l.fingerprint = None;
            } else {
                let v: serde_json::Value =
                    serde_json::from_str(t).map_err(|e| format!("fingerprint JSON: {e}"))?;
                l.fingerprint = Some(v);
            }
        }
        F::FfVersion => {
            l.ff_version = if t.is_empty() {
                None
            } else {
                Some(
                    t.parse::<u32>()
                        .map_err(|_| format!("版本号: {t:?} 不是整数"))?,
                )
            };
        }
        F::Addons => l.addons = parse_list(t).unwrap_or_default(),
        F::ExcludeAddons => l.exclude_addons = parse_list(t).unwrap_or_default(),
        F::Fonts => l.fonts = parse_list(t),
        F::CustomFontsOnly => {
            l.custom_fonts_only = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "custom_fonts_only")?)
            }
        }
        F::FpPreset => {
            l.fingerprint_preset = if t.is_empty() || t == "off" {
                None
            } else if t == "random" {
                Some(FingerprintPreset::Random)
            } else {
                let v: serde_json::Value =
                    serde_json::from_str(t).map_err(|e| format!("fingerprint_preset JSON: {e}"))?;
                Some(FingerprintPreset::Value { value: v })
            };
        }
        F::GeoipDb => {
            l.geoip_db = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        F::MainWorldEval => {
            l.main_world_eval = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "main_world_eval")?)
            }
        }
        F::EnableCache => {
            l.enable_cache = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "enable_cache")?)
            }
        }
        F::DebugMode => {
            l.debug = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "debug")?)
            }
        }
        F::IKnowWhat => {
            l.i_know_what_im_doing = if t.is_empty() {
                None
            } else {
                Some(parse_bool(t, "i_know_what_im_doing")?)
            }
        }
        F::ExecutablePath => {
            l.executable_path = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        F::Browser => {
            l.browser = if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        }
        F::FirefoxUserPrefs => l.firefox_user_prefs = parse_json_map(t, "firefox_user_prefs")?,
        F::Args => l.args = parse_list(t).unwrap_or_default(),
        F::Env => l.env = parse_json_map(t, "env")?,
        F::ExtraLaunchOptions => {
            l.extra_launch_options = parse_json_map(t, "extra_launch_options")?
        }
    }
    Ok(())
}

fn parse_json_map(t: &str, name: &str) -> Result<camoforge_protocol::JsonMap, String> {
    if t.trim().is_empty() {
        return Ok(camoforge_protocol::JsonMap::new());
    }
    let v: serde_json::Value =
        serde_json::from_str(t).map_err(|e| format!("{name}: JSON 解析失败 {e}"))?;
    match v {
        serde_json::Value::Object(m) => Ok(m.into_iter().collect()),
        _ => Err(format!("{name}: 应为 JSON 对象")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_FIELDS: &[F] = &[
        F::Os,
        F::Headless,
        F::ColorScheme,
        F::BlockImages,
        F::BlockWebrtc,
        F::BlockWebgl,
        F::DisableCoop,
        F::GeoipMode,
        F::GeoipIp,
        F::ProxyServer,
        F::ProxyUser,
        F::ProxyPass,
        F::ProxyPort,
        F::HumanizeMode,
        F::HumanizeMaxTime,
        F::WebglVendor,
        F::WebglRenderer,
        F::ScreenSpec,
        F::ScreenWH,
        F::ScreenMinMax,
        F::Window,
        F::Locale,
        F::Persistent,
        F::UserDataDir,
        F::DownloadsDir,
        F::Fingerprint,
        F::FfVersion,
        F::Addons,
        F::ExcludeAddons,
        F::Fonts,
        F::CustomFontsOnly,
        F::FpPreset,
        F::GeoipDb,
        F::MainWorldEval,
        F::EnableCache,
        F::DebugMode,
        F::IKnowWhat,
        F::ExecutablePath,
        F::Browser,
        F::FirefoxUserPrefs,
        F::Args,
        F::Env,
        F::ExtraLaunchOptions,
    ];

    #[test]
    fn specs_cover_every_field_once() {
        let specs: Vec<F> = SPECS
            .iter()
            .chain(ADVANCED_SPECS)
            .map(|spec| spec.field)
            .collect();
        assert_eq!(specs.len(), ALL_FIELDS.len());
        for field in ALL_FIELDS {
            assert_eq!(specs.iter().filter(|item| *item == field).count(), 1);
        }
    }

    #[test]
    fn every_field_keeps_its_empty_value_semantics() {
        for field in ALL_FIELDS {
            let mut profile = Profile::new("test");
            let result = set_text(&mut profile, *field, "");
            if *field == F::HumanizeMaxTime {
                assert!(result.is_err());
            } else {
                result.unwrap_or_else(|error| panic!("{field:?} rejected empty value: {error}"));
            }
            let _ = get_text(&profile, *field);
        }
    }

    #[test]
    fn field_families_keep_validation_and_round_trip() {
        let mut profile = Profile::new("test");
        for (field, text) in [
            (F::Headless, "virtual"),
            (F::BlockImages, "true"),
            (F::ProxyPort, "8080"),
            (F::Window, "1280,800"),
            (F::Locale, "zh-CN,zh"),
            (F::Env, r#"{"FOO":"bar"}"#),
        ] {
            set_text(&mut profile, field, text).unwrap();
            assert!(!get_text(&profile, field).is_empty());
        }

        assert!(set_text(&mut profile, F::BlockImages, "maybe").is_err());
        assert!(set_text(&mut profile, F::ProxyPort, "not-a-port").is_err());
        assert!(set_text(&mut profile, F::Window, "1280").is_err());
        assert!(set_text(&mut profile, F::Env, "[]").is_err());
    }
}
