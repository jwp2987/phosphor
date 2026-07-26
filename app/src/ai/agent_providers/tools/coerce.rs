//! Tool argument tolerance layer.
//!
//! Some BYOP models (especially the DeepSeek reasoner and certain OSS models) will,
//! in the `arguments` of `tool_calls`, write booleans as `"true"`/`"false"`, write
//! numbers as strings, or JSON.stringify an entire array/object. `from_args` parses
//! strictly with serde, so this kind of input gets rejected outright, which shows up
//! on the UI side as "the tool intermittently fails."
//!
//! This module is only invoked after `from_args` fails the first time: it reads the
//! `parameters()` schema and, based on the type declared there, force-converts
//! strings in the JSON Value back to the target type. Coverage:
//!
//! | schema type | model returns | corrected to |
//! |---|---|---|
//! | boolean | "true"/"True"/"1"/"yes" | true |
//! | boolean | "false"/"False"/"0"/"no" | false |
//! | integer | "42" / 42.0 | 42 |
//! | number | "3.14" | 3.14 |
//! | string | 42 / true | "42" / "true" |
//! | array | "[\"a\"]" (JSON string) | ["a"] |
//! | object | "{\"k\":1}" (JSON string) | {"k":1} |
//!
//! Fields that can't be coerced are left as-is, so the original parse error surfaces.

use serde_json::{Number, Value};

/// Attempt to correct the args JSON against the schema. Returns `Some(coerced_string)`
/// if at least one type conversion was made; returns `None` if the input can't be
/// parsed as JSON at all, or no field needed coercion.
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
        // No type declared in the schema: for an object type, try recursing into
        // properties; otherwise give up.
        if let Some(props) = schema.get("properties") {
            coerce_object(value, props, schema, changed);
        }
        return;
    };

    match ty {
        "object" => {
            // Case where the model stringified the whole object: parse one layer
            // and continue.
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

    // Explicit null == not provided (only for non-required fields).
    //
    // Models love to write "I'm not filling in this optional field" as an explicit
    // null instead of omitting the key. serde's `#[serde(default)]` only covers the
    // case where the field is entirely absent; an explicit null still gets rejected,
    // burning a whole tool-call round-trip for nothing. Observed in practice on
    // 2026-07-19: qwen3-it:4b's first call to read_files:
    //
    //   {"files":[{"line_ranges":null,"path":"./start_flm.sh"}]}  → invalid type: null
    //   {"files":[{"line_ranges":[],"path":"./start_flm.sh"}]}    → ok (model retried itself)
    //
    // The two are semantically identical. Dropping this key lets serde fall through
    // to its default, which is equivalent and doesn't require a retry.
    //
    // A null on a required field is always left as-is: that means the model truly
    // omitted a required value, and we shouldn't invent one here — the original parse
    // error should surface (consistent with this module's rule that fields which
    // can't be coerced are left as-is).
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
    // additionalProperties: the schema may also describe fields not listed in properties.
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

    /// Real schema fragment from read_files: `path` is required, `line_ranges` is optional.
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

    /// Regression observed in practice (2026-07-19, qwen3-it:4b's first read_files call).
    /// An optional field with an explicit null should be dropped so serde's
    /// `#[serde(default)]` kicks in.
    #[test]
    fn explicit_null_optional_field_is_dropped() {
        let args = r#"{"files":[{"line_ranges":null,"path":"./start_flm.sh"}]}"#;
        let out = coerce_args_against_schema(args, &read_files_schema()).expect("coerced");
        let v: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["files"][0]["path"], json!("./start_flm.sh"));
        assert!(
            v["files"][0].get("line_ranges").is_none(),
            "an optional field with null should be removed, got: {}",
            v["files"][0]
        );
    }

    /// A required field with null **must not** be removed: that means the model
    /// omitted a required value, and the original parse error should surface rather
    /// than us quietly inventing a default value here.
    #[test]
    fn explicit_null_required_field_is_preserved() {
        let args = r#"{"files":[{"path":null,"line_ranges":[]}]}"#;
        match coerce_args_against_schema(args, &read_files_schema()) {
            None => {}
            Some(out) => {
                let v: Value = serde_json::from_str(&out).unwrap();
                assert!(
                    v["files"][0].get("path").is_some_and(Value::is_null),
                    "a required field with null should not be removed, got: {}",
                    v["files"][0]
                );
            }
        }
    }
}
