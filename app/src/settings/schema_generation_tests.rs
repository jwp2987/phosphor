use serde_json::json;

use super::{settings_schema_json, strip_empty_enum_entries, strip_numeric_metadata};

#[test]
fn strips_numeric_metadata_recursively() {
    let mut schema = json!({
        "type": "object",
        "properties": {
            "count": {
                "type": "integer",
                "minimum": 0,
                "maximum": 255,
                "format": "uint8"
            }
        }
    });

    strip_numeric_metadata(&mut schema);

    assert_eq!(
        schema,
        json!({
            "type": "object",
            "properties": {
                "count": {
                    "type": "integer"
                }
            }
        })
    );
}

#[test]
fn strips_empty_enum_entries() {
    let mut schema = json!({
        "oneOf": [
            {
                "enum": [],
                "type": "string"
            },
            {
                "const": "kept"
            }
        ]
    });

    strip_empty_enum_entries(&mut schema);

    assert_eq!(
        schema,
        json!({
            "oneOf": [
                {
                    "const": "kept"
                }
            ]
        })
    );
}

/// `settings_schema_json` takes a feature-flag predicate rather than
/// mutating global state (see the module doc for why), so this can call it
/// directly with no global side effects -- unlike the removed
/// `generate_settings_schema` binary, which had to mutate real
/// `FeatureFlag` state because it had no other way to gate settings.
#[test]
fn generates_a_settings_schema_with_no_flags_enabled() {
    let schema = settings_schema_json("dev", |_flag| false).expect("schema should build");
    let schema: serde_json::Value = serde_json::from_str(&schema).expect("schema should parse");

    assert_eq!(schema["title"], "Phosphor Settings");
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"].is_object());
    assert!(
        schema["description"]
            .as_str()
            .is_some_and(|d| d.contains("dev channel")),
        "description should name the requested channel, got {:?}",
        schema["description"]
    );
}

/// A setting gated behind a flag the predicate reports as disabled must not
/// appear in the schema -- the whole point of the predicate indirection.
#[test]
fn a_feature_gated_setting_is_included_only_when_its_flag_is_enabled() {
    use settings::schema::SettingSchemaEntry;

    let Some(gated_entry) = inventory::iter::<SettingSchemaEntry>
        .into_iter()
        .find(|entry| !entry.is_private && entry.feature_flag.is_some())
    else {
        // No feature-gated public setting exists in this build (e.g. a
        // minimal feature set) -- nothing to assert against.
        return;
    };
    let gated_flag = gated_entry.feature_flag.expect("checked above");

    let without = settings_schema_json("dev", |_flag| false).expect("schema should build");
    let without: serde_json::Value = serde_json::from_str(&without).expect("schema should parse");
    let with = settings_schema_json("dev", |flag| flag == gated_flag).expect("schema should build");
    let with: serde_json::Value = serde_json::from_str(&with).expect("schema should parse");

    let contains_key = |root: &serde_json::Value, key: &str| -> bool {
        fn search(value: &serde_json::Value, key: &str) -> bool {
            match value {
                serde_json::Value::Object(map) => {
                    map.contains_key(key) || map.values().any(|v| search(v, key))
                }
                serde_json::Value::Array(arr) => arr.iter().any(|v| search(v, key)),
                _ => false,
            }
        }
        search(root, key)
    };

    assert!(
        !contains_key(&without, gated_entry.storage_key),
        "{} should be absent when its flag is disabled",
        gated_entry.storage_key
    );
    assert!(
        contains_key(&with, gated_entry.storage_key),
        "{} should be present when its flag is enabled",
        gated_entry.storage_key
    );
}
