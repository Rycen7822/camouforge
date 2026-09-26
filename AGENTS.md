# AGENTS.md — AI Agent 协作指南

这份文件写给帮助用户下载、部署、配置 CamouForge 的 AI agent（ZCode / Claude Code / Codex
等）。用户会把 README 中的 prompt 发给你；你的任务边界与纪律以本文件为准。

## 项目是什么（30 秒了解）

- 原生 Windows 桌面应用：Rust/GPUI 前端 + 受监管的 Python worker，驱动
  [camoufox](https://camoufox.com/)（Python SDK 0.5.5）反指纹浏览器。
- 终端用户零预装：便携包 = `camoforge.exe` + `worker/` + `pyproject.toml` + `uv.lock` +
  `uv.toml` + `LICENSE` 放同一目录。首次启动自动下载固定版 uv 0.12.4（SHA-256 校验）、
  托管 CPython 3.12，并按锁文件同步依赖到 `.venv`——全部落在应用目录内。
- 浏览器本体不通过 pip 安装：在应用内「应用设置 → camoufox 浏览器下载」选择官方
  release 下载到 exe 同级 `camoufox/<版本>/`。**禁止 `python -m camoufox fetch`**。

## 权威文档（按任务选读）

| 任务 | 先读 |
|---|---|
| 修改任何配置/指纹字段、排查「改了没生效」 | `COOKBOOK.md`（§0 TL;DR + §8 静默失败模式） |
| 给 UI 表单加字段 | `COOKBOOK.md` §6 |
| launch 参数在 UI/worker/SDK 三层的对应关系 | `COOKBOOK.md` §5 |
| 从示例创建新身份 | `examples/README.md` + `examples/example-profile.json` |
| 构建、打包、环境自举 | `README.md`（Build from source / portable） |

## 配置纪律（最容易翻车的先看）

1. 配置表单由 `crates/protocol/src/registry.rs` 驱动：只有登记过的键出现在 UI；未登记键
   在「原始 JSON」面板编辑，同样会生效。
2. 键名必须与浏览器版本 `properties.json` 完全一致——不一致会被 SDK `validate_config`
   **静默跳过**（不报错、不剔除）；类型错误才会抛 `InvalidPropertyType`。
3. `port`、`webGl2:vendor`、`webGl2:renderer` 是无效键（SDK 无消费点）；`fonts`/`voices`
   在 fingerprint/preset 模式下会被 SDK 按目标 OS 重新随机生成。
4. 修改 `pyproject.toml` 依赖：必须用固定版 uv 0.12.4 重新 `uv lock` 并提交 `uv.lock`；
   用户侧删除 `.venv\.camouforge-env.json`（或整个 `.venv`）以触发下次启动重建环境。
5. 改动不会热生效：`settings.json`（应用设置）在启动时一次性读入内存；profile 配置修改
   后需要重新启动该身份的浏览器实例。

## 从示例创建新身份

`examples/example-profile.json` 是完整可用的身份模板（东京出口 / macOS Firefox 152 固定
指纹），结构与应用存储格式逐字段一致。帮用户创建身份的步骤：

1. 以示例为基底，把 `id` 改为新随机 UUIDv4（示例的全零 UUID 保留给示例本身，勿复用），
   `name` 按用户意图命名，`created_at`/`updated_at` 置当前 unix 秒。
2. 写入 `<数据目录>\profiles\<id>.json`（文件名必须等于 id）。数据目录默认为
   `camoforge.exe` 同级的 `camoforge_data\`，环境变量 `CAMOFORGE_DATA_DIR` 可覆盖。
3. **必须完全退出应用（含托盘）再重启**——profile 列表只在启动时加载，运行中放入的
   文件不会出现。
4. 按用户需求改写示例时保持一致性，不要产生自相矛盾的指纹：
   - `browser` 的 Firefox 版本须与指纹 UA 的 `rv:` 一致，且该版本确实可用（已安装目录或
     应用内已下载；`executable_path` 留空时走全局 `settings.json` 的 `camoufox_dir`）。
   - UA / platform / oscpu / headers UA / geoip 与 timezone / geolocation / webgl / 字体
     必须指向同一 OS 与地区（示例是 macOS + 东京成套值，改动要成套改）。
   - 新增 `config` 键必须是该浏览器版本 `properties.json` 的合法键（COOKBOOK §4），
     键名不符会被静默跳过；有 `fingerprint` 时 `navigator.*`/`window.*`/`screen.*`/
     `locale:*` 前缀键会被 SDK 剥离（COOKBOOK §7）。
5. 代理凭据以明文存在 profile JSON 里：生成后提醒用户，且不要把含凭据的内容回显到
   日志或对话。
6. 创建结果交用户在 UI 里确认（GUI 交互归用户）；配置问题按「配置纪律」排查。

## 下载 / 部署纪律

1. 打包只用 `scripts/package-portable.ps1`（产物在 `dist\camouforge\`）。不要手工只拷
   exe——exe 同级缺少 `pyproject.toml`/`uv.lock`/`uv.toml`/`worker` 时应用会报
   「发布包不完整」并拒绝启动。
2. 不要移动或删除应用目录内自动生成的运行时产物：`.venv/`（worker 环境）、
   `.camouforge-runtime/`（uv 工具、托管 Python、缓存）、`camoforge_data/`（身份数据）、
   `camoufox/`（浏览器版本）。`camoforge_data/cache` 根的 `.0.5_FLAG` 是 SDK 防误删标记，
   缺失会导致 SDK 清掉整个缓存目录。
3. 修复 Python 环境走应用内「设置 → Python worker 运行环境 → 检查并修复」；不要手删
   `.venv`——应用有 staging 原子替换机制，旧环境保留到新环境完整验证成功。
4. 用户对 C 盘占用敏感：所有数据与运行时都在应用目录（D 盘等），不要建议把任何缓存、
   环境或数据装到 C 盘。

## Agent 工作纪律

1. 所有 GUI 交互（应用内点击、浏览器内验证）归用户手动执行；agent 可以截图查看，
   不要代替点击。
2. 不要直接运行 `target/release/camoforge.exe` 或开发构建来「替用户试试」——开发运行
   会在 exe 同级创建数据目录（`cargo clean` 会连同数据删掉）。功能验证交给用户在部署
   目录执行，或用 `verify_worker.py` / `cargo test` 做无界面验证。
3. 临时脚本与调试产物放仓库 `temp/` 目录（已 gitignore）；不要写入用户 C 盘。
4. 提交信息遵循 Conventional Commits；绝不提交 `.venv`、`.camouforge-runtime`、`dist`、
   `camoforge_data`、uv 工具或任何运行时产物。
5. Rust 改动须过 `cargo fmt --all -- --check`、`cargo clippy --all-targets -- -D warnings`、
   `cargo test`；Python 改动用 `.venv\Scripts\python.exe -m py_compile` 与
   `tests/test_worker_unit.py` 验证。
