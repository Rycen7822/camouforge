# CamouForge

[English](README.md) | **简体中文**

CamouForge 是一个原生 Windows 桌面应用，用于创建、编辑和运行
[Camoufox](https://camoufox.com/) 浏览器身份。项目采用 Rust/GPUI 界面和受监管的
Python worker，将 Camoufox 启动参数与指纹配置整理为结构化图形界面。

![CamouForge 界面](docs/images/camouforge-ui.png)

## 主要功能

- 管理多个相互独立的浏览器身份及其持久化数据。
- 编辑启动、导航器、屏幕窗口、WebGL/媒体、地理语言、网络、字体声音及原始指纹配置。
- 生成 BrowserForge 指纹、校验身份配置，并在应用中选择已下载的 Camoufox 版本。
- 通过原生 GPUI 桌面界面启动、停止和监控浏览器实例。
- 创建网页快捷方式，并将浏览器下载保存到用户指定的本地目录。
- 在应用内下载 Camoufox 官方版本。
- 使用 [uv](https://docs.astral.sh/uv/) 自动配置和修复 Python worker 环境；便携版不要求
  系统预装 Python。

## 架构

```text
GPUI 桌面应用（Rust）
  ├─ 身份存储与 UI 状态
  ├─ Python 环境自举（uv + CPython 3.12）
  ├─ worker 监管与 JSON-RPC 通信
  └─ Windows 进程与 Job Object 管理
                 │ stdio JSON-RPC
Python worker
  ├─ Camoufox SDK 适配
  ├─ 身份转换与校验
  ├─ 浏览器生命周期管理
  └─ 下载及浏览器补丁辅助模块
```

- `crates/protocol`：身份数据模型、RPC 类型和指纹字段注册表。
- `crates/app`：GPUI 应用、存储、自举、worker 监管及 Windows 集成。
- `worker`：Python RPC 入口和 Camoufox 集成模块。

## 平台与前置条件

CamouForge 当前面向 Windows 10 或更高版本。

从源码构建需要安装：

- Rust stable
- Visual Studio Build Tools，并勾选 **使用 C++ 的桌面开发**
- [uv](https://docs.astral.sh/uv/)

便携版首次启动需要联网下载固定版本的 uv、托管的 CPython 3.12、Python 依赖，以及用户
在应用中选择的 Camoufox 浏览器版本。完成配置后可以复用本地环境。

## 从源码构建

```powershell
git clone https://github.com/Rycen7822/camouforge.git
cd camouforge
uv sync --frozen --no-dev --python 3.12
cargo build --release --locked
```

运行开发版本：

```powershell
cargo run --release --locked
```

生成便携目录：

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\package-portable.ps1
```

产物位于 `dist\camouforge\`。请将 `camoforge.exe`、`worker\`、`pyproject.toml`、
`uv.lock`、`uv.toml` 和 `LICENSE` 保持在同一目录。应用会在 EXE 旁创建 `.venv`、
`.camouforge-runtime` 和 `camoforge_data`；这些运行时目录不会进入仓库或发布包。

## 测试

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test --all --locked
.venv\Scripts\python.exe tests\test_worker_unit.py
.venv\Scripts\python.exe verify_worker.py
```

`tests/test_worker_integration.py` 中的浏览器集成测试需要真实 Camoufox，并在 Linux 上使用
Xvfb 显示环境。

## 项目结构

```text
camouforge/
├─ crates/app/                 # GPUI 桌面应用
├─ crates/protocol/            # 共享身份与 RPC 模型
├─ worker/                     # Python Camoufox worker
├─ tests/                      # worker 单元测试与集成测试
├─ scripts/package-portable.ps1
├─ COOKBOOK.md                 # 面向贡献者的配置参考
├─ pyproject.toml / uv.lock    # 锁定的 worker 环境
└─ Cargo.toml / Cargo.lock     # Rust workspace
```

## 本地数据与安全

身份和运行时数据默认只保存在本机。身份 JSON 中的代理凭据目前以明文保存，请勿复用重要
凭据，并妥善保护 `camoforge_data` 目录。请仅在你有权测试的系统和服务上使用 CamouForge。

## 许可证

[MIT](LICENSE)
