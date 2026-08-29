//! CamouForge 协议层：Profile 数据模型 + stdio JSON-RPC 类型 + 指纹字段注册表。
//!
//! 本 crate 无 UI 依赖，是 Rust 应用与 Python worker 之间的唯一契约。

pub mod profile;
pub mod registry;
pub mod rpc;

pub use profile::*;
pub use registry::*;
pub use rpc::*;
