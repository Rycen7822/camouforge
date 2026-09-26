# 示例 Profile

[`example-profile.json`](example-profile.json) 是一个完整、可直接使用的身份配置示例：
东京出口（Asia/Tokyo + 东京 geolocation）、macOS Firefox 152 指纹（Apple M1 / 1728×1117 @2x /
ja-JP）、固定指纹 + 持久化上下文 + 本地代理。文件结构与 `camoforge_data\profiles\` 下的
存储格式完全一致（serde 2 空格缩进 JSON）。

AI agent 帮用户基于此示例创建身份的步骤与一致性约束见根目录 `AGENTS.md`
「从示例创建新身份」。

## 使用

1. 把 JSON 复制到部署目录的 `camoforge_data\profiles\00000000-0000-0000-0000-000000000001.json`
   （文件名 = 身份 id）。
2. 完全退出 CamouForge（含托盘）后重新启动——profile 列表在启动时加载。
3. 按需修改；每次字段编辑会在 UI 内即时写盘。

示例用全零 UUID（`00000000-…-0001`），不会与真实身份冲突。

## 结构速览

| 段 | 内容 |
|---|---|
| `launch` | 启动参数：os/humanize/headless、代理、geoip、快捷方式、下载目录、浏览器版本（`browser` 指定 `official/152.0.4-beta.29`，`executable_path` 留空走全局 `camoufox_dir` 回退） |
| `launch.fingerprint` | 完整 BrowserForge 指纹（screen/navigator/headers/插件/显卡/字体/编解码） |
| `launch.firefox_user_prefs` | 直接写入浏览器的 about:config 偏好 |
| `config` | SDK 注入补丁（timezone、geolocation、webgl、window、mediaDevices 等 29 键） |

注意：同时给出 `fingerprint` 时，SDK 会剥离 `config` 中 `navigator.*` / `window.*` /
`screen.*` / `locale:*` 前缀键（指纹注入已覆盖这些面），实际生效的是 fingerprint 内的值。
详见根目录 `COOKBOOK.md` §7 与 §8。
