//! stdio JSON-RPC 协议（行分隔 JSON）。
//!
//! 请求:  `{"id": "...", "method": "launch", "params": {...}}`
//! 响应:  `{"id": "...", "ok": true, "result": {...}}` 或 `{"id": "...", "ok": false, "error": "..."}`
//! 事件:  `{"event": "instance_exited", "data": {...}}`

use crate::{InstanceInfo, Profile};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl Response {
    pub fn ok(id: impl Into<String>, result: impl Serialize) -> Self {
        Self {
            id: id.into(),
            ok: true,
            result: Some(serde_json::to_value(result).expect("serializable")),
            error: None,
        }
    }

    pub fn err(id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            ok: false,
            result: None,
            error: Some(message.into()),
        }
    }
}

// 说明：worker 的事件走 supervisor.rs 手写的 `{"event": ..., "data": ...}` 解析，
// 版本信息经 `ping` RPC 返回，故这里不再维护已失同步的 WorkerEvent / PingResult 类型。

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchParams {
    pub profile_id: String,
    pub profile: Profile,
    /// None = 使用 profile.launch 中已有的 user_data_dir 或按 id 自动生成。
    #[serde(default)]
    pub user_data_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchResult {
    pub profile_id: String,
    pub pid: Option<u32>,
    pub headless: bool,
    /// 实际生效的 user_data_dir。
    pub user_data_dir: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateFingerprintParams {
    /// None = 全 OS 池；Some(["macos"]) = 指定。
    pub os: Option<Vec<String>>,
    /// 模拟的 Firefox 版本（默认当前版本）。
    pub ff_version: Option<u32>,
    pub locale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateFingerprintResult {
    /// browserforge Fingerprint 完整对象（可直接存入 profile.launch.fingerprint）。
    pub fingerprint: serde_json::Value,
    /// 人类可读摘要。
    pub summary: FingerprintSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FingerprintSummary {
    pub user_agent: String,
    pub os: String,
    pub platform: String,
    pub screen: String,
    pub hardware_concurrency: u32,
    pub device_memory: Option<f64>,
    pub webgl_vendor: String,
    pub webgl_renderer: String,
    pub languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateParams {
    pub profile: Profile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateResult {
    pub warnings: Vec<ConfigWarning>,
    /// worker 干跑 launch_options 抛异常时的错误信息（如 InvalidPropertyType、
    /// CamoufoxNotInstalled）。有值时校验失败，UI 必须优先展示而不是报「通过」。
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigWarning {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceListResult {
    pub instances: Vec<InstanceInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWebglParams {
    /// windows / macos / linux（默认 macos）。
    #[serde(default)]
    pub os: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebglCard {
    pub vendor: String,
    pub renderer: String,
    pub weight: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWebglResult {
    #[serde(default)]
    pub cards: Vec<WebglCard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFontsParams {
    /// windows / macos / linux（默认 macos）。
    #[serde(default)]
    pub os: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListFontsResult {
    #[serde(default)]
    pub fonts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVoicesParams {
    /// windows / macos / linux（默认 macos）。
    #[serde(default)]
    pub os: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVoicesResult {
    #[serde(default)]
    pub voices: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListVersionsResult {
    #[serde(default)]
    pub versions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_result_deserializes_error_field() {
        let v: ValidateResult = serde_json::from_str(
            r#"{"warnings": [], "error": "InvalidPropertyType: bad type for config.x"}"#,
        )
        .unwrap();
        assert!(v
            .error
            .as_deref()
            .unwrap()
            .starts_with("InvalidPropertyType"));

        // 旧 worker（无 error 字段）也必须能反序列化
        let v2: ValidateResult =
            serde_json::from_str(r#"{"warnings": [{"code": "n", "message": "m"}]}"#).unwrap();
        assert!(v2.error.is_none());
        assert_eq!(v2.warnings.len(), 1);
    }
}
