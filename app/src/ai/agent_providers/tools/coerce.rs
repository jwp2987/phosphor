//! 工具参数容错层。
//!
//! 部分 BYOP 模型(尤其 DeepSeek reasoner、某些 OSS 模型)在 tool_calls 的
//! `arguments` 里会把 boolean 写成 `"true"`/`"false"`、把数字写成字符串、把
//! array/object 整个 JSON.stringify 一次。`from_args` 用 serde 严格解,这类
//! 输入会直接 reject,UI 端表现为"工具偶发故障"。
//!
//! 本模块只在 `from_args` 第一次失败后才被调用:读 `parameters()` schema,
//! 按 schema 声明的类型,把 JSON Value 里的 string 强转回目标类型。覆盖:
//!
//! | schema type | 模型返回 | 修正为 |
//! |---|---|---|
//! | boolean | "true"/"True"/"1"/"yes" | true |
//! | boolean | "false"/"False"/"0"/"no" | false |
//! | integer | "42" / 42.0 | 42 |
//! | number | "3.14" | 3.14 |
//! | string | 42 / true | "42" / "true" |
//! | array | "[\"a\"]"(JSON 字符串) | ["a"] |
//! | object | "{\"k\":1}"(JSON 字符串) | {"k":1} |
//!
//! 不能 coerce 的字段保留原值,让原始解析错误透出。

use serde_json::{Number, Value};

/// 尝试根据 schema 修正 args JSON。返回 `Some(coerced_string)` 表示至少做了一次
/// 类型转换;返回 `None` 表示输入根本解不出 JSON 或没有任何字段需要 coerce。
pub fn coerce_args_against_schema(args_str: &str, schema: &Value) -> Option<String> {
    let mut value: Value = serde_json::from_str(args_str).ok()?;
    let mut changed = false;
    coerce_value(&mut value, schema, &mut changed);
    if !changed {
        return None;
    }
    serde_json::to_string(&value).ok()
}

fn coerce_value(value: &mut Value, schema: &Value, changed: &mut bool) {
    let Some(ty) = schema.get("type").and_then(|t| t.as_str()) else {
        // schema 没标 type:对象类型尝试递归 properties,否则放弃。
        if let Some(props) = schema.get("properties") {
            coerce_object(value, props, schema, changed);
        }
        return;
    };

    match ty {
        "object" => {
            // 模型把整个 object 字符串化的情况:解一层后再继续。
            if let Some(s) = value.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    if parsed.is_object() {
                        *value = parsed;
                        *changed = true;
                    }
                }
            }
            if let Some(props) = schema.get("properties") {
                coerce_object(value, props, schema, changed);
            }
        }
        "array" => {
            if let Some(s) = value.as_str() {
                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                    if parsed.is_array() {
                        *value = parsed;
                        *changed = true;
                    }
                }
            }
            if let (Some(arr), Some(items_schema)) = (value.as_array_mut(), schema.get("items")) {
                for item in arr {
                    coerce_value(item, items_schema, changed);
                }
            }
        }
        "boolean" => {
            if let Some(s) = value.as_str() {
                match s.to_ascii_lowercase().as_str() {
                    "true" | "1" | "yes" => {
                        *value = Value::Bool(true);
                        *changed = true;
                    }
                    "false" | "0" | "no" => {
                        *value = Value::Bool(false);
                        *changed = true;
                    }
                    _ => {}
                }
            }
        }
        "integer" => {
            if let Some(s) = value.as_str() {
                if let Ok(n) = s.parse::<i64>() {
                    *value = Value::Number(n.into());
                    *changed = true;
                } else if let Ok(f) = s.parse::<f64>() {
                    if f.fract() == 0.0 && f.is_finite() {
                        if let Some(num) = Number::from_f64(f).and_then(|n| n.as_i64()) {
                            *value = Value::Number(num.into());
                            *changed = true;
                        }
                    }
                }
            } else if let Some(f) = value.as_f64() {
                if f.fract() == 0.0 && f.is_finite() {
                    let n = f as i64;
                    *value = Value::Number(n.into());
                    *changed = true;
                }
            }
        }
        "number" => {
            if let Some(s) = value.as_str() {
                if let Ok(f) = s.parse::<f64>() {
                    if let Some(num) = Number::from_f64(f) {
                        *value = Value::Number(num);
                        *changed = true;
                    }
                }
            }
        }
        "string" => match value {
            Value::Number(n) => {
                let s = n.to_string();
                *value = Value::String(s);
                *changed = true;
            }
            Value::Bool(b) => {
                *value = Value::String(b.to_string());
                *changed = true;
            }
            _ => {}
        },
        _ => {}
    }
}

fn coerce_object(value: &mut Value, props: &Value, parent_schema: &Value, changed: &mut bool) {
    let Some(obj) = value.as_object_mut() else {
        return;
    };
    let Some(props_map) = props.as_object() else {
        return;
    };

    // 显式 null == 没填(仅限非 required 字段)。
    //
    // 模型很爱把"这个可选字段我不填"写成显式 null 而不是省略 key。serde 的
    // `#[serde(default)]` 只兜得住"字段整个缺失",显式 null 照样 reject,白白烧掉
    // 一轮 tool call。实测 2026-07-19,qwen3-it:4b 第一次调 read_files:
    //
    //   {"files":[{"line_ranges":null,"path":"./start_flm.sh"}]}  → invalid type: null
    //   {"files":[{"line_ranges":[],"path":"./start_flm.sh"}]}    → ok(模型自己重试)
    //
    // 两者语义完全一样。删掉这个 key 让 serde 走 default,等价且不需要重试。
    //
    // required 字段的 null 一律保留原样:那是模型真的漏了必填值,这里不该替它编一个,
    // 应该让原始解析错误透出去(与本模块"不能 coerce 的字段保留原值"一致)。
    let required: std::collections::HashSet<&str> = parent_schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
        .unwrap_or_default();
    let null_optional: Vec<String> = obj
        .iter()
        .filter(|(k, v)| v.is_null() && !required.contains(k.as_str()))
        .map(|(k, _)| k.clone())
        .collect();
    for k in null_optional {
        obj.remove(&k);
        *changed = true;
    }

    for (key, prop_schema) in props_map {
        if let Some(field) = obj.get_mut(key) {
            coerce_value(field, prop_schema, changed);
        }
    }
    // additionalProperties: schema 也有可能描述未列在 properties 中的字段。
    if let Some(additional) = parent_schema
        .get("additionalProperties")
        .filter(|v| v.is_object())
    {
        let known: std::collections::HashSet<&String> = props_map.keys().collect();
        // SAFETY: keys collected before mutating values. Walk via owned copy of
        // the keys to avoid double borrow.
        let extra_keys: Vec<String> = obj.keys().filter(|k| !known.contains(k)).cloned().collect();
        for k in extra_keys {
            if let Some(field) = obj.get_mut(&k) {
                coerce_value(field, additional, changed);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shell_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "command": {"type": "string"},
                "is_read_only": {"type": "boolean"},
                "uses_pager": {"type": "boolean"},
                "is_risky": {"type": "boolean"},
                "wait_until_complete": {"type": "boolean"}
            },
            "required": ["command"]
        })
    }

    #[test]
    fn boolean_strings_coerced() {
        let args =
            r#"{"command":"echo b","is_read_only":"true","is_risky":"False","uses_pager":"0"}"#;
        let out = coerce_args_against_schema(args, &shell_schema()).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["is_read_only"], json!(true));
        assert_eq!(v["is_risky"], json!(false));
        assert_eq!(v["uses_pager"], json!(false));
    }

    #[test]
    fn no_change_returns_none() {
        let args = r#"{"command":"echo b","is_read_only":true}"#;
        assert!(coerce_args_against_schema(args, &shell_schema()).is_none());
    }

    #[test]
    fn malformed_json_returns_none() {
        let args = r#"{not json"#;
        assert!(coerce_args_against_schema(args, &shell_schema()).is_none());
    }

    fn grep_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "queries": {"type": "array", "items": {"type": "string"}},
                "path": {"type": "string"}
            }
        })
    }

    #[test]
    fn array_string_coerced_to_array() {
        let args = r#"{"queries":"[\"mod menu\",\"foo\"]","path":"app/src/lib.rs"}"#;
        let out = coerce_args_against_schema(args, &grep_schema()).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["queries"], json!(["mod menu", "foo"]));
    }

    #[test]
    fn integer_string_coerced() {
        let schema = json!({
            "type": "object",
            "properties": {"count": {"type": "integer"}}
        });
        let args = r#"{"count":"42"}"#;
        let out = coerce_args_against_schema(args, &schema).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["count"], json!(42));
    }

    #[test]
    fn nested_array_items_coerced() {
        let schema = json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {"flag": {"type": "boolean"}}
                    }
                }
            }
        });
        let args = r#"{"items":[{"flag":"true"},{"flag":"false"}]}"#;
        let out = coerce_args_against_schema(args, &schema).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["items"][0]["flag"], json!(true));
        assert_eq!(v["items"][1]["flag"], json!(false));
    }

    #[test]
    fn number_to_string_field() {
        let schema = json!({
            "type": "object",
            "properties": {"path": {"type": "string"}}
        });
        let args = r#"{"path":42}"#;
        let out = coerce_args_against_schema(args, &schema).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["path"], json!("42"));
    }

    #[test]
    fn stringified_object_coerced() {
        let schema = json!({
            "type": "object",
            "properties": {
                "config": {
                    "type": "object",
                    "properties": {"enabled": {"type": "boolean"}}
                }
            }
        });
        let args = r#"{"config":"{\"enabled\":\"true\"}"}"#;
        let out = coerce_args_against_schema(args, &schema).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["config"]["enabled"], json!(true));
    }

    /// read_files 的真实 schema 片段:`path` 必填,`line_ranges` 可选。
    fn read_files_schema() -> Value {
        json!({
            "type": "object",
            "properties": {
                "files": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "path": {"type": "string"},
                            "line_ranges": {"type": "array", "items": {"type": "object"}}
                        },
                        "required": ["path"]
                    }
                }
            },
            "required": ["files"]
        })
    }

    /// 实测回归(2026-07-19,qwen3-it:4b 首次 read_files 调用)。
    /// 显式 null 的可选字段应被删掉,让 serde 的 `#[serde(default)]` 生效。
    #[test]
    fn explicit_null_optional_field_is_dropped() {
        let args = r#"{"files":[{"line_ranges":null,"path":"./start_flm.sh"}]}"#;
        let out = coerce_args_against_schema(args, &read_files_schema()).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["files"][0]["path"], json!("./start_flm.sh"));
        assert!(
            v["files"][0].get("line_ranges").is_none(),
            "null 的可选字段应被删除,实际: {}",
            v["files"][0]
        );
    }

    /// required 字段为 null 时**不能**被删:那是模型漏了必填值,
    /// 应该让原始解析错误透出,而不是在这里悄悄编一个默认值。
    #[test]
    fn explicit_null_required_field_is_preserved() {
        let args = r#"{"files":[{"path":null,"line_ranges":[]}]}"#;
        match coerce_args_against_schema(args, &read_files_schema()) {
            None => {}
            Some(out) => {
                let v: Value = serde_json::from_str(&out).unwrap();
                assert!(
                    v["files"][0].get("path").is_some_and(Value::is_null),
                    "required 的 null 字段不应被删除,实际: {}",
                    v["files"][0]
                );
            }
        }
    }
}
