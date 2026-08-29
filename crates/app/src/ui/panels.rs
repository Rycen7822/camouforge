//! 指纹表单的值转换。

use camoforge_protocol::{FieldKind, FieldSpec};

pub fn value_to_text(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Number(n) => n.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// 用户输入文本 → JSON 值。解析失败返回错误文案（供行内提示，不再静默降级存字符串）。
pub fn text_to_value(text: &str, spec: &FieldSpec) -> Result<serde_json::Value, String> {
    let t = text.trim();
    if t.is_empty() {
        return Ok(serde_json::Value::Null);
    }
    let bad = |kind: &str| format!("{}: 需要 {kind}，得到 {text:?}", spec.label);
    match spec.kind {
        FieldKind::Bool => match t {
            "true" | "1" | "yes" => Ok(serde_json::Value::Bool(true)),
            "false" | "0" | "no" => Ok(serde_json::Value::Bool(false)),
            _ => Err(bad("true/false")),
        },
        FieldKind::Int | FieldKind::SInt | FieldKind::SelectInt(_) => t
            .parse::<i64>()
            .map(|n| serde_json::Value::Number(n.into()))
            .map_err(|_| bad("整数")),
        FieldKind::Float => t
            .parse::<f64>()
            .map(|n| {
                serde_json::Number::from_f64(n)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            })
            .map_err(|_| bad("数字")),
        FieldKind::TextList | FieldKind::SelectMulti(_) => Ok(serde_json::Value::Array(
            t.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .map(serde_json::Value::String)
                .collect(),
        )),
        FieldKind::Json => {
            serde_json::from_str(t).map_err(|e| format!("{}: JSON 解析失败 {e}", spec.label))
        }
        _ => Ok(serde_json::Value::String(t.into())),
    }
}
