# CamouForge 配置修改 Cookbook

> 目的：结合 **camoufox Python SDK 源码**（0.5.5，`.venv` 内随包）、**官方文档**（camoufox.com/fingerprint、GitHub README）与 **CLI `--help`**，给出完整的配置键 / 启动参数 / 数据源参考，方便后续（人类或 agent）**手动修改配置、新增或调整表单字段、排查配置不生效**。
>
> 适用版本：camoufox SDK 0.5.5（browserforge 1.2.4 / playwright 1.60）、CamouForge 当前代码（registry 91 字段）。camoufox 升级后本文件需重新核对（尤其是 §4 properties.json 与 §3 数据源路径）。
>
> 配置修改的工作纪律与项目约束见 `AGENTS.md`。

---

## 0. TL;DR（改配置前先看）

| 现象 | 结论 |
|---|---|
| 改了一个键但浏览器没生效 | 键名与 `properties.json` 不一致 → `validate_config` **静默跳过**（§8.1） |
| UI 表单里找不到某个键 | registry 只登记 91 个键；其余 17 个只能在「原始 JSON」面板编辑（§4.3） |
| 想给表单加一个字段 | 在 `crates/protocol/src/registry.rs` 加一行 `f(...)`，用 `FieldKind` 决定控件（§6） |
| 值类型写错 | 不静默，抛 `InvalidPropertyType`（§8.2） |
| 填了 `port` / `webGl2:vendor` / `webGl2:renderer` | 无效键，SDK 无消费点（§8.3） |
| 填了 `fonts`/`voices` 但被换了 | 走 fingerprint/preset 模式时 SDK 按目标 OS 重新生成随机子集（§7） |

---

## 1. 配置数据流（改配置前先看懂这张图）

```
CamouForge UI（GPUI）
  ├─ 表单面板    ← 由 registry.rs 的 FieldGroup/FieldKind 驱动渲染（§6）
  └─ 原始 JSON    ← 未登记键的唯一编辑入口
        │  每字段编辑即持久化 → profile.json（BTreeMap<String, serde_json::Value>）
        ▼
Profile JSON：{ "id": "...", "name": "...", "launch": {...}, "config": {...} }
        │
        ▼  Rust 端 AppState.launch_profile() → 通过 stdio JSON-RPC 发送到 worker
worker/camoforge_worker.py
        │  Manager.launch() → translate_profile()：把 launch 语义枚举展开为 SDK 原生参数（§5.2）
        ▼
camoufox.sync_api.Camoufox(**kw)
        │  launch_options(config=..., os=..., headless=..., ...)
        ▼
camoufox/utils.py::validate_config()   ← 对照浏览器版本的 properties.json 做类型校验
        │   未知键：print("Skipping unknown patch ...") 后继续（不报错、不剔除）
        │   类型错：抛 InvalidPropertyType
        ▼
NewBrowser → NewContext（addInitScript 注入指纹，脚本自销毁）
        ▼
浏览器页面
```

要点：

- **`config` 字典整体直通**：worker 的 `translate_profile` 里 `kw["config"] = dict(profile["config"])`，profile 的 `config` 段原样交给 SDK。所以「改配置」绝大多数时候就是改 `profile.json` 的 `config` 段，或改 registry 让 UI 能编辑它。
- **`launch` 段是语义枚举**：不是 SDK 原生参数，由 worker 展开（如 `headless: "virtual"` → `kw["headless"]="virtual"`、`humanize.mode=="custom"` → `kw["humanize"]=float(max_time)`）。
- **worker 与 Rust 端协议**：stdin/stdout 行分隔 JSON，`{"id","method","params"}` / `{"id","ok","result"}` / `{"event":...}`。方法：`ping / launch / stop / stop_all / list / generate_fingerprint / validate / list_webgl / list_versions / list_fonts / list_voices / shutdown`。

---

## 2. 从哪里获取信息（权威数据源）

| 数据源 | 位置 | 内容 / 用途 |
|---|---|---|
| **properties.json**（最权威） | Camoufox 浏览器安装目录下的 `properties.json`；运行时用 `camoufox.utils.get_path("properties.json")` 定位 | **108 个 config 键 + 类型表**（str/uint/int/double/bool/array/dict）。`validate_config` 的类型校验依据。**随浏览器版本变化**——升级浏览器后必须重新导出 |
| SDK 源码 | `.venv/Lib/site-packages/camoufox/`（0.5.5） | `utils.py`（launch_options/validate_config/check_valid_os）、`fingerprints.py`（preset→config、自动修正函数）、`sync_api.py`/`async_api.py`（NewBrowser/NewContext）、`__main__.py`（CLI）、`repos.yml`（浏览器 repo + GeoIP 库枚举）、`geolocation.py`、`addons.py`（DefaultAddons 仅 UBO） |
| SDK 数据文件 | `.venv/Lib/site-packages/camoufox/` | `fonts.json`（win 107 / mac 574 / lin 134）、`voices.json`（mac 190 / win 53 / lin 131）、`webgl/webgl_data.db`（13 vendor × 33 组合）、`territoryInfo.xml`（CLDR 258 地区 / 710 语言）、`fingerprint-presets.json` / `-v150.json`（实采真实指纹池）、`browserforge.yml`（BrowserForge→config 键映射） |
| 官方文档 | camoufox.com/fingerprint（14 个子页，按分类的散文文档）；GitHub daijro/camoufox README | 只列分类（Navigator/Cursor/Fonts/Screen/Window/Document/HTTP Headers/Geolocation&Intl/WebRTC IP/WebGL/Media&Audio/Voices/Addons/Miscellaneous），**无机器可读键清单**。README 明确：未设置的 config 由 BrowserForge 自动填充 |
| CLI | `.venv/Scripts/python.exe -m camoufox --help` 及各子命令 | 版本管理 + 枚举（§10） |
| BrowserForge 贝叶斯网络 | `.venv/Lib/site-packages/apify_fingerprint_datapoints/`（v0.15.0） | 默认指纹路径的真正值池（25 节点：UA 235 条、hardwareConcurrency 39 值、maxTouchPoints 12 值、screen 4569 组合…），（值池即随包贝叶斯网络数据点，直接查询该目录 JSON） |

### 2.1 一键导出当前浏览器版本的 properties.json

```python
from camoufox.utils import get_path
import json
with open(get_path("properties.json"), encoding="utf-8") as f:
    for item in json.load(f):
        print(item["property"], "\t", item["type"])
```

> ⚠️ 这是**列表**结构（`[{property, type}, ...]`），不是 dict；`validate_config` 内部才转成 dict。

---

## 3. profile.json 结构速览

一个 profile 的两段：

```json
{
  "id": "3fa85f64-...",
  "name": "身份 1",
  "launch": {
    "os": ["windows", "macos"],           // 目标 OS 列表（可随机）
    "headless": "headed",                  // headed | headless | virtual
    "color_scheme": "auto",                // auto | dark | light
    "humanize": { "mode": "off", "max_time": 2.5 },   // off | on | custom
    "geoip": { "mode": "off", "ip": "" },  // off | auto | ip
    "geoip_db": "MaxMind GeoLite2",
    "proxy": { "server": "", "username": "", "password": "", "port": null },
    "screen": { "mode": "off", "width": 0, "height": 0, "min_width": 0, ... },
    "window": [1280, 720],
    "fingerprint": { ... },                // browserforge 完整 JSON（最高优先级）
    "fingerprint_preset": { "mode": "off" },  // off | random | value
    "webgl_config": { "vendor": "", "renderer": "" },
    "fonts": null, "custom_fonts_only": false,
    "addons": null, "exclude_addons": null,
    "locale": null, "ff_version": null,
    "persistent_context": false, "user_data_dir": "",
    "downloads_dir": null,               // 下载保存目录；留空= %USERPROFILE%\Downloads（见 §8.10）
    "shortcuts": [ { "name": "chatgpt", "url": "chatgpt.com" } ],  // 新标签页/主页快捷方式
    "block_images": false, "block_webrtc": false, "block_webgl": false,
    "disable_coop": false, "main_world_eval": false,
    "enable_cache": false, "debug": false, "i_know_what_im_doing": null,
    "browser": null, "executable_path": null,
    "firefox_user_prefs": null, "args": null, "env": null,
    "extra_launch_options": null
  },
  "config": {
    "navigator.userAgent": "Mozilla/5.0 ...",
    "timezone": "Asia/Tokyo",
    "showcursor": false
  }
}
```

- **`config` 的键名**必须与 properties.json 完全一致（含大小写、冒号），否则被静默跳过。
- **`launch` 的键名**由 worker `translate_profile` 解释（§5.2），错名会被忽略（走默认值）。

---

## 4. properties.json 108 键全表（config 合法键面）

类型图例：`str` 字符串 · `uint` 无符号整数 · `int` 有符号整数 · `double` 浮点 · `bool` 布尔 · `array` 数组 · `dict` 对象。
registry 控件图例：✅=已下拉/Switch · 文本=自由输入 · `—`=未登记（仅原始 JSON）。

### 4.1 navigator（str / uint / bool / array）

| 键 | 类型 | registry | 值域 / 说明 |
|---|---|---|---|
| navigator.userAgent | str | Multiline | 自由；留空由 SDK 按目标 OS 生成一致 UA |
| navigator.doNotTrack | str | Select ✅ | `0` / `1` / `null` |
| navigator.appCodeName | str | Select ✅ | 常量 `Mozilla` |
| navigator.appName | str | Select ✅ | 常量 `Netscape` |
| navigator.appVersion | str | Select ✅ | `5.0 (Macintosh)` / `(Windows)` / `(X11; Linux x86_64)` / `(X11; Linux i686)` |
| navigator.oscpu | str | Select ✅ | `Intel Mac OS X 10.15` / `Windows NT 10.0; Win64; x64` / `Linux x86_64` / `Linux i686` |
| navigator.language | str | Select ✅ | BCP-47（32 常用清单） |
| navigator.languages | array | SelectMulti ✅ | BCP-47 列表 |
| navigator.platform | str | Select ✅ | `Win32` / `MacIntel` / `Linux x86_64` / `Linux i686`（BrowserForge 网络另有 iPhone/iPad/arm 值，桌面 Firefox 只取这 4 个） |
| navigator.hardwareConcurrency | uint | SelectInt ✅ | 39 值闭合池（2…640）；properties 只要求 uint，池来自 BrowserForge |
| navigator.product | str | Select ✅ | 常量 `Gecko` |
| navigator.productSub | str | Select ✅ | `20100101` / `20030107` |
| navigator.maxTouchPoints | uint | SelectInt ✅ | 12 值：0/1/2/3/5/10/15/16/20/40/80/256 |
| navigator.cookieEnabled | bool | Bool ✅ | 自由 |
| navigator.globalPrivacyControl | bool | Bool ✅ | 自由 |
| navigator.buildID | str | Select ✅ | YYYYMMDDHHMMSS 模板（20250301000000…20250801000000） |
| navigator.onLine | bool | Bool ✅ | 自由 |

### 4.2 screen / window / document（uint / int / double）

| 键 | 类型 | registry | 说明 |
|---|---|---|---|
| screen.availHeight / availWidth | uint | Int | 自由 |
| screen.availTop / availLeft | uint | SInt | 自由（properties 是 uint，负值会类型报错） |
| screen.height / width | uint | Int | 自由 |
| screen.colorDepth / pixelDepth | uint | SelectInt ✅ | **24 / 30 / 32**（48 是错值） |
| screen.pageXOffset / pageYOffset | double | Float | 自由 |
| window.scrollMinX / scrollMinY | int | SInt | 自由 |
| window.scrollMaxX / scrollMaxY | int | Int | 自由 |
| window.outerWidth / outerHeight | uint | Int | 自由 |
| window.innerWidth / innerHeight | uint | Int | 自由 |
| window.screenX / screenY | int | SInt | 自由 |
| window.history.length | uint | Int | 自由 |
| window.devicePixelRatio | double | Float | 自由（SDK 注释：非 1.0 可疑，BrowserForge 不映射此键） |
| document.body.clientWidth / clientHeight / clientTop / clientLeft | uint | Int | 自由 |

### 4.3 headers / webrtc / 杂项 str

| 键 | 类型 | registry | 说明 |
|---|---|---|---|
| headers.User-Agent | str | Multiline | 自由 |
| headers.Accept-Language | str | Select ✅ | 9 模板（`en-US,en;q=0.9` 等） |
| headers.Accept-Encoding | str | Select ✅ | 7 组合（`gzip, deflate, br, zstd` 等） |
| webrtc:ipv4 / webrtc:ipv6 | str | Text | 自由 |
| **webrtc:localipv4 / localipv6** | str | — | 未登记（原始 JSON 编辑） |
| **pdfViewerEnabled** | bool | — | 未登记 |
| timezone | str | Select ✅ | IANA 312 规范区域（zone1970.tab，`timezones.inc`） |
| **audio:seed** | uint | — | 未登记；preset 模式每次启动随机 1..2³²-1 |
| **canvas:seed** | uint | — | 未登记；同上 |
| **humanize** | bool | — | 未登记（launch 面板有同义参数） |
| **showcursor** | bool | Bool ✅ | 已登记（HUMANIZE 组） |
| **canvas:aaOffset** | int | — | 未登记 |
| **canvas:aaCapOffset** | bool | — | 未登记 |

### 4.4 battery / fonts / voices / audio（bool / double / uint / array）

| 键 | 类型 | registry | 说明 |
|---|---|---|---|
| battery:charging | bool | Bool ✅ | 自由 |
| battery:chargingTime / dischargingTime / level | double | Float | 自由（Infinity=不可充电） |
| fonts | array | TextList | 按 OS 闭合目录：win 107 / mac 574 / lin 134（fonts.json）；**留空由 SDK 按 OS 随机生成子集** |
| fonts:spacing_seed | uint | Int | 自由种子（1..2³²-1） |
| voices | array | TextList | 按 OS 闭合目录：mac 190 / win 53 / lin 131（voices.json） |
| voices:blockIfNotDefined | bool | Bool ✅ | 未列出语音不出现在 getVoices() |
| voices:fakeCompletion | bool | Bool ✅ | speak() 立即触发完成事件 |
| voices:fakeCompletion:charsPerSecond | double | Float | 自由 |
| AudioContext:sampleRate | uint | SelectInt ✅ | 44100 / 48000 / 96000 |
| AudioContext:outputLatency | double | Float | 自由 |
| AudioContext:maxChannelCount | uint | SelectInt ✅ | 2 / 6 / 8 |

### 4.5 geolocation / locale（double / str）

| 键 | 类型 | registry | 说明 |
|---|---|---|---|
| geolocation:latitude / longitude / accuracy | double | Float | 自由；开启 geoip 时自动计算 |
| locale:language | str | Select ✅ | BCP-47 裸语言码（50 常用；CLDR 全集 710 种） |
| locale:region | str | Select ✅ | BCP-47 地区码（50 常用；CLDR 全集 258 种） |
| locale:script | str | Select ✅ | ISO 15924（22 常用） |
| locale:all | str | Text | 逗号分隔全部 locale |

### 4.6 webgl（str / array / dict / bool）

| 键 | 类型 | registry | 说明 |
|---|---|---|---|
| webGl:vendor / webGl:renderer | str | Text | 必须为 webgl_data.db 精确值（13 vendor / 33 组合，**按 OS 配对**），否则 SDK 抛 "No WebGL data found" |
| webGl:supportedExtensions / webGl2:supportedExtensions | array | TextList | 自由 |
| webGl:parameters / webGl2:parameters | dict | Json | 自由 |
| webGl:parameters:blockIfNotDefined / webGl2 同 | bool | Bool ✅ | 未定义参数返回 null |
| webGl:shaderPrecisionFormats / webGl2 同 | dict | Json | 自由 |
| webGl:shaderPrecisionFormats:blockIfNotDefined / webGl2 同 | bool | Bool ✅ | 未定义精度返回 null |
| webGl:contextAttributes / webGl2 同 | dict | Json | 自由 |

> ⚠️ **`webGl2:vendor` / `webGl2:renderer` 不在 properties.json**——已从 registry 移除；SDK 的 WebGL2 继承 WebGL1 的 vendor/renderer。

### 4.7 mediaDevices / misc（bool / uint / array）

| 键 | 类型 | registry | 说明 |
|---|---|---|---|
| mediaDevices:micros / webcams / speakers | uint | SelectInt ✅ | 设备计数 0-4 |
| mediaDevices:enabled | bool | Bool ✅ | 自由 |
| allowMainWorld | bool | Bool ✅ | page.evaluate 加 `mw:` 前缀 |
| **allowAddonNewtab** | bool | — | 未登记（properties.json 第 108 个键，旧文档漏掉） |
| **forceScopeAccess** | bool | — | 未登记 |
| **disableTheming** | bool | — | 未登记 |
| **disableInstantAnimations** | bool | — | 未登记 |
| **memorysaver** | bool | — | 未登记 |
| **addons** | array | — | 未登记（launch 面板有同义参数） |
| **certificatePaths / certificates** | array | — | 未登记 |
| **debug** | bool | — | 未登记（launch 面板有同义参数） |

### 4.8 registry 统计

- properties.json：**108 键**；registry 登记：**91 字段**；properties 有、registry 缺：**17 键**（上表 `—` 项）。
- registry 有、properties 无（无效键）：**0 个**（`port`、`webGl2:vendor`、`webGl2:renderer` 已清理——`port` 仍由 worker 写入但 SDK 无消费点，见 §8.3）。

---

## 5. launch 参数全表（UI ↔ worker ↔ SDK）

### 5.1 launch 面板字段（`crates/app/src/ui/launch_panel.rs`）

| 分组 | 字段 | JSON 键 | 控件 / 枚举 |
|---|---|---|---|
| 系统 | 操作系统 | launch.os | 多选 chips（windows/macos/linux，逗号存储） |
| 系统 | 显示模式 | launch.headless | Select：headed / headless / virtual |
| 系统 | 颜色方案 | launch.color_scheme | Select：auto / dark / light |
| 隐私拦截 | 拦截图片 / WebRTC / WebGL / COOP | block_images / block_webrtc / block_webgl / disable_coop | Switch |
| 地理位置 | GeoIP 模式 | launch.geoip.mode | Select：off / auto / ip |
| 地理位置 | 指定 IP | launch.geoip.ip | 文本 |
| 代理 | 服务器 / 用户名 / 密码 / 端口伪装 | launch.proxy.* | 文本（port 写入 config.port，无效键见 §8.3） |
| 人类化 | 模式 / 最大耗时 | launch.humanize.{mode,max_time} | Select：off / on / custom + 文本 |
| WebGL | 厂商 / 渲染器 | launch.webgl_config.{vendor,renderer} | 文本 + 「WebGL 组合」列表加载（worker `list_webgl`） |
| 屏幕与窗口 | 模式 / exact / range / 窗口尺寸 | launch.screen.*, launch.window | Select off/exact/range + 文本 |
| 语言与持久化 | 语言 / 持久化 / 数据目录 | launch.locale / persistent_context / user_data_dir | 文本 + Switch |
| 指纹 | 指纹 JSON（生成按钮）/ Firefox 版本 | launch.fingerprint / ff_version | 文本域 + 按钮 |
| 指纹预设 | 模式 | launch.fingerprint_preset.mode | Select：off / random（value 模式在原始 JSON） |
| Addons | 加载 / 排除 | launch.addons / exclude_addons | 文本（ubo 短名） |
| 字体 | 注入字体 / 仅自定义 | launch.fonts / custom_fonts_only | 文本 + Switch |
| 调试与注入 | GeoIP 库 / 主世界 / 缓存 / 调试 / 确认风险 | launch.geoip_db / main_world_eval / enable_cache / debug / i_know_what_im_doing | Select（geoip_db：MaxMind GeoLite2 / GeoIP AIO by daijro）+ Switches |
| 浏览器 | 版本 / Firefox 路径 | launch.browser / executable_path | 动态下拉（worker `list_versions`）+ 文本 |
| 下载 | 下载目录 | launch.downloads_dir | 文本 + 文件夹选择按钮（留空= %USERPROFILE%\Downloads） |
| 快捷方式 | 网页快捷方式（名称 + 网址） | launch.shortcuts | 自定义列表编辑器（非 registry 驱动；逐行 name/url + 增删按钮） |

### 5.2 worker `translate_profile` 展开规则（改 launch 段前必读）

源码：`worker/profile_translate.py::translate_profile`。

| launch JSON | → SDK kwarg | 规则 |
|---|---|---|
| `os: ["windows","macos"]` | `os` | 单元素→str，多元素→list；非法值（非 windows/macos/linux）抛 ValueError |
| `humanize.mode` | `humanize` | off→省略；on→`True`；custom→`float(max_time)` |
| `headless` | `headless` | headless→`True`；virtual→`"virtual"`；headed→省略 |
| `proxy` | `proxy` | 有 server 才设置；`proxy.port` 整数写入 **config.port** |
| `geoip.mode` | `geoip` | ip→ip 字符串；auto→`True`；off→省略；`geoip_db` 另传 |
| `fingerprint` | `fingerprint` | 重建 browserforge Fingerprint；**删除 config 中以 `navigator.` / `window.` / `screen.` / `headers.User-Agent` / `locale:` 开头的键**（避免与指纹冲突）；把 `fp.screen.devicePixelRatio` 写回 `config["window.devicePixelRatio"]`（补 SDK 不映射的 DPR） |
| `fingerprint_preset` | `fingerprint_preset` | random→`True`；value→dict；off→省略 |
| 透传 | `os_version` `geoip_db` `locale` `ff_version` `executable_path` `browser` | 非空直接透传；`locale` 列表长 1 时展成 str |
| `fonts` / `custom_fonts_only` | 同名 | 非 null 透传 |
| `screen` | `screen` | exact→`Screen(min=max=w/h)`；range→min/max 四元组；off→省略 |
| `window` | `window` 或写入指纹 | 有 fingerprint 时用 `handle_window_size` 写进指纹（SDK 忽略 window 参数）；无指纹时传 `(w,h)`；**headed 且未设窗口时默认 1280x720** |
| `webgl_config` | `webgl_config` | `(vendor, renderer)` 元组 |
| 布尔开关 | `block_images` `block_webrtc` `block_webgl` `disable_coop` `main_world_eval` `enable_cache` `debug` `i_know_what_im_doing` | 非 null 透传 |
| `addons` / `exclude_addons` | 同名 | exclude 的 `"ubo"` 短名→`DefaultAddons.UBO` |
| `firefox_user_prefs` / `args` / `env` / `extra_launch_options` | 同名 | **原样透传**；extra_launch_options 直接 `kw.update()`——可塞任意 SDK/Playwright 参数 |
| `color_scheme` | `color_scheme` | 仅 dark/light 透传（Playwright 驱动默认把 prefers-color-scheme 强制成 light，见 §8.6） |
| `persistent_context` | `persistent_context` + `user_data_dir` | true 时开启；目录留空= `<data_dir>/user_data/<profile_id>` |
| — | `executable_path` | 未指定时自动探测：`CAMOUFOX_EXECUTABLE_PATH` 环境变量 → `<项目根>/../camoufox*` 兄弟目录 → `<项目根>/camoufox` |
| — | `ff_version` | 自定义 executable_path 且未指定时从 `application.ini` 推断主版本号 |
| — | `i_know_what_im_doing` | 设置了 fingerprint 或 ff_version 且未显式指定时默认 `True`（抑制告警） |

> **不在 `translate_profile` 里的两个 launch 键**（在 `Manager.launch` 里处理）：
> - `downloads_dir`：`_resolve_downloads_dir` 取 `launch.downloads_dir`，留空回退 `%USERPROFILE%\Downloads`（**不**解析 Windows 重定向后的「下载」文件夹，重定向用户需显式填写）。下载通过 Juggler 的 `download` 事件 + `save_as` 落盘：事件回调只 `inst.submit` 入队（回调里直接 `save_as` 会死锁），`save_as` 在实例命令线程执行；命令循环空闲时用 `q.get(timeout=0.05)` + `page.wait_for_timeout(200)` 泵调度器，否则事件交不到 Python。`accept_downloads` 是**上下文级**参数——持久化上下文随 `launch_persistent_context` 传入；非持久化则对 `browser.new_context(accept_downloads=True)` 显式开启（`launch()` 不收该参数，传了抛 `TypeError`）。三层坑全解见 §8.10。
> - `shortcuts`：`launch.shortcuts`（`{name,url}` 列表）渲染成本地 HTML 首页（`browser.startup.homepage` + `browser.startup.page=1`）并生成一个 `chrome_url_overrides.newtab` 小扩展追加进 `addons`，使每个新标签页（Ctrl+T）也显示快捷方式网格；同时把 `browser.newtabpage.enabled` 置 `true`（camoufox 默认 `false` 导致新标签页空白）。

### 5.3 SDK `launch_options()` 全参数（`utils.py`，含默认值语义）

| 参数 | 类型 | 说明 |
|---|---|---|
| `config` | dict | **指纹注入键**（§4 的 108 键），直通 validate_config |
| `os` | str \| list | windows / macos / linux（小写！列表随机挑）；默认 `["windows","macos","linux"]` |
| `block_images` / `block_webrtc` / `block_webgl` / `disable_coop` | bool | 拦截图片 / WebRTC / WebGL / 禁 COOP（跨源 iframe 可点） |
| `webgl_config` | (str, str) | 必须为 webgl_data.db 精确值 |
| `geoip` | str \| bool | IP 字符串 / True（自动找 IP）；据此算经纬度/时区/国家/locale |
| `geoip_db` | str | GeoIP 库**名字**（MaxMind GeoLite2 / GeoIP AIO by daijro），不是路径 |
| `humanize` | bool \| float | True 或光标移动最大秒数 |
| `locale` | str \| list | 首个用于 Intl API |
| `addons` / `exclude_addons` | list | addon 名 / 默认 addon 排除 |
| `fonts` / `custom_fonts_only` | list / bool | 额外字体 / 只加载自定义字体 |
| `screen` | Screen | 约束指纹屏幕尺寸（exact/range 由 min/max 表达） |
| `window` | (int,int) | 固定窗口尺寸（有 fingerprint 时被忽略） |
| `fingerprint` | Fingerprint | 自定义 BrowserForge 指纹（最高优先级，非全部值被实现） |
| `fingerprint_preset` | bool \| dict | True=随机内置 preset；dict=指定 preset；None(默认)=BrowserForge 网络 |
| `ff_version` | int | UA 版本号（默认当前 camoufox 版本） |
| `headless` | bool \| "virtual" | virtual 仅 Linux（Xvfb） |
| `main_world_eval` | bool | `mw:` 前缀脚本 |
| `executable_path` | str \| Path | 自定义浏览器二进制 |
| `browser` | str | repo/build/version specifier：`official/beta.20`、`beta.20`、`134.0.2-beta.20` |
| `firefox_user_prefs` / `proxy` / `args` / `env` | dict / list | Playwright 透传 |
| `enable_cache` | bool | 页面/请求缓存（费内存） |
| `i_know_what_im_doing` / `debug` | bool | 确认风险 / 打印实际 config |
| `virtual_display` | str | Linux 虚拟显示 |
| `**launch_options` | — | 其余透传给 Playwright launch |

### 5.4 NewContext 上下文级参数（每个上下文独立指纹）

| 参数 | 说明 |
|---|---|
| `preset` | 指定 preset dict；None 随机 |
| `os` | preset 选择的 OS（windows/macos/linux） |
| `ff_version` | UA 打补丁的 Firefox 版本串 |
| `webrtc_ip` | WebRTC ICE 伪装 IPv4 |
| `proxy` | 每上下文代理（会派生 webrtc_ip / timezone） |
| `geolocation` | `{"latitude": float, "longitude": float}` |

---

## 6. registry.rs 字段注册指南（给 UI 加字段）

文件：`crates/protocol/src/registry.rs`。UI 表单完全由它驱动。

### 6.1 FieldKind（11 种控件）

```rust
pub enum FieldKind {
    Text,          // 单行文本
    Multiline,     // 多行文本（UA 等）
    Int,           // 无符号整数输入
    SInt,          // 有符号整数输入
    Float,         // 浮点输入
    Bool,          // Switch
    TextList,      // 逗号分隔数组输入（fonts / voices / extensions）
    Select(&[&str]),          // 固定选项下拉（字符串值）
    SelectInt(&[&str]),       // 固定选项下拉（整数值，字符串存储）
    SelectMulti(&[&str]),     // 多选：下拉添加 + 标签删除（navigator.languages）
    Json,          // 任意 JSON 文本域（webGl:parameters 等 dict）
}
```

### 6.2 新增字段三步

1. **加常量池**（若固定值）：如 `pub const MY_POOL: &'static [&'static str] = &[...];`（`timezones.inc` 是 `include!` 进来的大文件示例）。
2. **在对应组加一行**：

   ```rust
   f("config.key", "中文标签", FieldKind::Select(MY_POOL), "说明。", "占位"),
   ```

   参数顺序：`key / label / kind / hint / placeholder`。组：`NAVIGATOR_FIELDS / WINDOW_FIELDS / SCREEN_FIELDS / DOCUMENT_FIELDS / WEBGL_FIELDS / MEDIA_AUDIO_FIELDS / WEBRTC_FIELDS / GEO_INTL_FIELDS / HEADER_FIELDS / FONT_VOICE_FIELDS / BATTERY_FIELDS / HUMANIZE_FIELDS / MISC_FIELDS`，顺序由 `FIELD_GROUPS` 决定。
3. **`cargo build` 零警告**即可；UI 自动渲染新字段（`ui/mod.rs` 遍历 registry 生成表单）。

### 6.3 关键行为（改 UI 逻辑前必读）

- **未登记键不丢**：profile.json 里有的值、registry 没有的键，在「原始 JSON」面板仍可编辑；`ensure_field_select` 对「值不在选项池」的情况处理为显示未选中且**不覆盖原值**。
- **field_selects 缓存**：`ui/mod.rs` 里以 `tab_key::spec.key` 为 key 缓存下拉组件，`field_edits` 事件按值写回。改 registry 后重启应用生效（无热重载）。
- **类型必须匹配 properties.json**：`FieldKind::Int` 配 `uint` 键、`Bool` 配 `bool` 键等；值类型错会在启动时抛 `InvalidPropertyType`。
- **大池下拉可搜索**：gpui-component Select 基于 SearchableList，312 项时区池能搜索过滤。
- 下拉的**固定值池只是 UI 建议**，不是 SDK 强制——properties.json 类型表才是唯一约束（如 hardwareConcurrency 在 SDK 层任意 uint 都合法，下拉只是便利）。

### 6.4 已有常量池（直接复用）

| 池 | 值域 |
|---|---|
| LANGUAGES | 32 个 BCP-47 locale（language / languages） |
| LANGUAGES_BARE | 50 个裸语言码（locale:language） |
| REGIONS | 50 个地区码（locale:region） |
| SCRIPTS | 22 个 ISO 15924（locale:script） |
| TIMEZONES | **312 个 IANA 规范区域**（timezones.inc，zone1970.tab） |
| COLOR_DEPTH | 24 / 30 / 32 |
| MAX_TOUCH_POINTS | 0/1/2/3/5/10/15/16/20/40/80/256 |
| HARDWARE_CONCURRENCY | 39 值（2…640） |
| DEVICE_COUNTS | 0-4 |
| APP_VERSIONS | 4 个 appVersion 模板 |
| BUILD_IDS | 6 个 YYYYMMDDHHMMSS |
| ACCEPT_ENCODINGS | 7 个组合 |
| ACCEPT_LANGUAGES | 9 个模板 |

---

## 7. SDK 自动规范化逻辑（改配置时要知道这些会覆盖/修正你的值）

来源：`camoufox/fingerprints.py`。配置注入前 SDK 会做这些修正，手动填值时可能被覆盖：

| 函数 | 行为 |
|---|---|
| `fix_navigator_arch(config, target_os)` | Linux armv8l/armv81 平台时派生/修正 `navigator.oscpu`（Linux i686 由平台派生，不是网络原生值） |
| `fix_screen_no_taskbar(config, target_os)` | 修正 `screen.availHeight`（去任务栏逻辑） |
| `clamp_window_dimensions(config)` | 把窗口/屏幕尺寸钳制到合法几何 |
| `set_media_devices_defaults(config)` | 未设 `mediaDevices:*` 时填默认设备计数 |
| `from_preset(preset, ff_version)` | preset→config 映射：userAgent（可换版本号）、platform、hardwareConcurrency、oscpu（可由 platform 派生）、maxTouchPoints、screen.width/height/colorDepth/pixelDepth/avail*、webGl:vendor/renderer、timezone；**每次启动随机生成 `fonts:spacing_seed`/`audio:seed`/`canvas:seed`（1..2³²-1）**；按 OS 随机生成 fonts（含 marker 字体）/voices 子集 |
| `_generate_random_font_subset(os)` / `_generate_random_voice_subset(os)` | 从 fonts.json / voices.json 目录按 OS 采样（fonts 30–78%，mac voices 40–80%）；含 CreepJS 标记字体（macOS `Helvetica Neue` / Windows `Segoe UI` / Linux `Arimo` 等）保证基础子集 |
| `from_browserforge(fp, ff_version)` | BrowserForge 网络→config 映射（browserforge.yml）；**不映射** devicePixelRatio、videoCard、multimediaDevices（camoufox 用 mediaDevices:* 替代） |
| `handle_window_size(fp, outer_w, outer_h)` | 把目标外窗尺寸写进指纹 screen（居中），保证 inner/outer/screen 几何自洽（worker 调用） |

> **含义**：`fonts`/`voices` 留空时永远被随机生成；想固定必须显式填，且只在**不走 fingerprint/preset** 时完全由你控制（fingerprint 模式会删 config 同域键）。seeds 走 preset 时必被重随机。

---

## 8. 常见坑与 FAQ

### 8.1 键不生效：validate_config 静默跳过未知键

```python
for key, value in config_map.items():
    expected_type = property_types.get(key)
    if not expected_type:
        print(f'Skipping unknown patch {key} : {value}')
        continue  # Property not supported by this browser version; skip silently
```

键名差一个字母/大小写/冒号→静默跳过，**不报错**。排查第一步：对照 §4 全表核对键名；再确认与当前浏览器版本的 properties.json 一致（升级浏览器后键面可能变化）。

### 8.2 类型不匹配：抛 InvalidPropertyType（不静默）

```python
if not validate_type(value, expected_type):
    raise InvalidPropertyType(f"Invalid type for property {key}. Expected {expected_type}, got {type(value).__name__}")
```

- `int` 接受整值浮点（`1.0`）但不接受字符串。
- `uint` 额外要求 ≥ 0。
- `array`/`dict` 必须是 list/dict（JSON 面板里 `{}` / `[]` 之外的会报错）。

### 8.3 无效键：`port` / `webGl2:vendor` / `webGl2:renderer`

- 三者均不在 properties.json → validate_config 打印 "Skipping unknown patch" 后继续（值仍会被 chunk 进 `CAMOU_CONFIG_<n>` 环境变量发给浏览器，但 launchServer.js 无对应处理，**无实际效果**）。
- `webGl2:vendor/renderer` 已从 registry 移除（WebGL2 继承 WebGL1 的 vendor/renderer，见 `browserforge.yml` 注释）。
- `port` 仍由 worker 从 `proxy.port` 写入 config（防代理端口泄漏的设计意图），但 SDK 0.5.5 无消费点——**改它没有用**，不要浪费时间调它。

### 8.4 `browser` 参数语义（不是 "firefox"/"chrome"）

合法取值是**已装版本 specifier**：`official/stable`、`beta.20`、`134.0.2-beta.20`、`official/134.0.2-beta.20`。可用 `python -m camoufox list` 或 worker `list_versions` 获取。旧版 UI 曾用 `["firefox","chrome"]`——那是错的，已改为动态下拉。

### 8.5 `geoip_db` 是库名不是路径

枚举只有两个：`MaxMind GeoLite2`（默认） / `GeoIP AIO by daijro`（来自 repos.yml geoip 段）。填别的名字抛 `ValueError: GeoIP database 'X' not found`。

### 8.6 `prefers-color-scheme` 恒为 light 的坑

Playwright Firefox 驱动默认把 context colorScheme 写死 `"light"`，导致支持暗色的站点渲染成白色。`launch.color_scheme = "dark"/"light"` 会透传给驱动覆盖默认。这是 worker 层的修复（§5.2）。

### 8.7 时区/语言等大池怎么改

- `timezone` 下拉 312 值来自 IANA zone1970.tab（`crates/protocol/src/timezones.inc`）；增删重跑「下载 zone1970.tab → 提取第 3 列规范名」脚本即可。
- `locale:language`/`locale:region` 的完整 CLDR 集（710 语言 / 258 地区）在 SDK `territoryInfo.xml`；UI 目前用常用子集，完整值仍可在原始 JSON 填（SDK `verify_locale` 用 `language_tags` 校验 BCP-47）。

### 8.8 fingerprint 模式会删掉你的 config

`launch.fingerprint` 非空时，worker 删除 config 里 `navigator.` / `window.` / `screen.` / `headers.User-Agent` / `locale:` 开头的键——这些信息由指纹携带，手动填了也会被抹掉（避免冲突告警）。**想手动控制这些键就别开 fingerprint**。

### 8.9 debug=True 看真实 config

`launch.debug = true` 时 SDK 打印发给浏览器的 config。排查「我填的值到底进没进去」最快的手段。

### 8.10 下载落盘的坑（三层：accept_downloads / 回调死锁 / 调度器停转）

camoufox 基于 Playwright 的 Juggler 版 Firefox。要让下载正确落到 `launch.downloads_dir`，必须依次过三层坑：

1. **`accept_downloads` 是上下文级参数**。只有 `accept_downloads=True` 时 Juggler 才触发 `download` 事件，否则静默丢弃。但它是 `new_context` / `launch_persistent_context` 的参数，**不是 `launch()` 的**：持久化上下文放进 launch kwargs 随 `launch_persistent_context` 生效；非持久化 `launch()` 收到它会抛 `TypeError`，须对 `browser.new_context(accept_downloads=True)` 显式开启。worker 把 download 事件挂到 context 上（`Browser` 本身没有 `download` 事件）。

2. **事件回调里不能调 `save_as()`**。sync Playwright 的事件回调跑在调度器 greenlet 上，在回调里调 `dl.save_as()` 会让调度器等自己继续收包 → 死锁。表现为左上角下载图标一闪、Firefox 下载库为空、目标目录无文件、日志既无「已下载」也无「保存失败」。正确做法：回调只 `inst.submit(...)` 把保存任务投进实例命令队列，由命令线程调 `save_as`。

3. **命令线程空闲时必须泵调度器**。实例线程空闲若阻塞在 `queue.get()`，调度器停转：Juggler 已拦截下载，Python 却永远收不到 `download` 事件（日志只有「下载目录」、从无「收到下载事件」）。命令循环须改成 `q.get(timeout=0.05)`，超时调 `page.wait_for_timeout(200)` 泵一次调度器，把积压事件交出来。

落盘目录 = `launch.downloads_dir`，留空回退 `%USERPROFILE%\Downloads`（**不**自动跟随 Windows 重定向后的「下载」文件夹，重定向用户需显式填 `downloads_dir`）。日志关键字：`[camoforge] 收到下载事件` / `[camoforge] 已下载: <路径>` / `[camoforge] 下载保存失败: ...`（进 `logs/` 按天日志）。

> **兜底**：worker 还配置 Firefox 原生下载目录（`browser.download.useDownloadDir=true` + `folderList=2` + `browser.download.dir=<downloads_dir>` + `always_ask_before_handling_new_types=false` + 常见 MIME 的 `neverAsk.saveToDisk`，均 `setdefault` 不覆盖用户显式 pref），作为确实不走 Juggler 的下载的兜底。

### 8.11 用户直接关浏览器窗口，主窗口状态不回落

实例线程的命令循环只在收到 `stop` 哨兵时退出；用户直接关 camoufox 窗口时浏览器进程结束，但没人通知 worker，主窗口的「停止」按钮/绿点/「运行实例: 1」会永远停在运行态。修法：`run()` 用 `_attach_close_watcher` 给浏览器对象挂 `disconnected`（Browser）/ `close`（BrowserContext，持久化上下文时 browser 本身就是 context；两个名字都挂，不认识的不会触发），回调只置 `inst.browser_gone` 标志并写退出原因「浏览器已退出（窗口被关闭或进程结束）」；命令循环在空闲泵时看到标志即退出，`finally` 统一发 `instance_exited`。Rust 端收事件后 `running.remove(profile_id)` + `cx.notify()`（`state.rs`），按钮回落「启动」。注意：close 事件的送达同样依赖空闲泵（§8.10 第 3 层），泵不能删。

---

## 9. 验证方法（改完配置后）

```bash
# 1. 编译（registry 改动后必须零警告）
cargo build --release

# 2. worker 干跑校验：复用 launch_options 收集 LeakWarning，不启动浏览器
#    （Rust 端「校验」按钮调 worker.validate）

# 3. CLI 环境自检
.venv/Scripts/python.exe -m camoufox version        # SDK/浏览器版本
.venv/Scripts/python.exe -m camoufox list           # 已装浏览器（browser 参数 specifier）
.venv/Scripts/python.exe -m camoufox path           # 安装目录

# 4. 用 Playwright inspector 打开页面看实际注入
.venv/Scripts/python.exe -m camoufox test https://example.com

# 5. 直接调 launch_options 干跑（不发请求不启浏览器）
.venv/Scripts/python.exe -c "
from camoufox.utils import launch_options
kw = launch_options(config={'timezone': 'Asia/Tokyo', 'navigator.hardwareConcurrency': 8}, headless=True)
print(kw['config'] if 'config' in kw else 'ok')
"
```

---

## 10. CLI 参考速查（`python -m camoufox`）

### 10.1 命令

| 命令 | 作用 | 参数 |
|---|---|---|
| `active` | 打印当前活动版本 | — |
| `fetch [VERSION]` | 安装活动/指定版本 | 例：`fetch official/135.0-beta.25` |
| `set [SPECIFIER]` | 设置活动版本并下载 | `--geoip`（切 GeoIP 源选择）；specifier 见下 |
| `list [installed\|all]` | 列出已装/全部版本 | `--path` |
| `path` | 打印安装目录 | — |
| `remove [VERSION_PATH]` | 删除下载数据 | `--select` / `-y` |
| `server` | 启动 Playwright server | — |
| `test [URL]` | 打开 Playwright inspector | `--executable-path` |
| `gui` | Camoufox Manager GUI（需 PySide6） | `--debug` |
| `sync` | 从远程 repo 同步版本 | `--spoof-os [auto\|mac\|win\|lin]`、`--spoof-arch [auto\|x86_64\|i686\|arm64]` |
| `version` | 版本/包/浏览器/存储信息 | — |

### 10.2 set specifier 语法

```
official/stable/134.0.2-beta.20    # 固定版本
official/stable                    # 频道最新
beta.20 / 134.0.2-beta.20          # 只写 build/version
```

### 10.3 枚举常量（来自 repos.yml / __main__.py / addons.py）

- 浏览器 repo（`browser` 参数与 `set` specifier 的 repo 段）：**official**（daijro/camoufox, camoufox/camoufox）、**CoryKing**、**JWriter20**
- 频道（channel）：**stable**（βeta.19 起）/ **prerelease**（含 alpha.*）
- GeoIP 库（`geoip_db`）：**MaxMind GeoLite2**（默认）/ **GeoIP AIO by daijro**
- OS（`os` / `fetch --os` / `sync --spoof-os`）：windows/macos/linux（config 语义）；auto/mac/win/lin（CLI 短名）
- 架构（`fetch --arch` / `sync --spoof-arch`）：auto / x86_64 / i686 / arm64
- 默认 addon（`exclude_addons`）：仅 **UBO**（uBlock Origin）

---

## 附：本 cookbook 与源码对照索引

| 需求 | 看哪里 |
|---|---|
| 某 config 键是否合法、什么类型 | §4 全表 / properties.json |
| config 怎么送到浏览器 | §1 数据流 + worker `translate_profile`（§5.2） |
| 某 launch 参数怎么传 | §5.2 / §5.3 |
| 给 UI 加字段 | §6 |
| 为什么我填的值被改了 | §7 SDK 规范化 + §8.8 fingerprint 覆盖 |
| 配置不生效 | §8.1 → §8.9 排查清单 |
| 浏览器版本/库名/枚举 | §10 |
