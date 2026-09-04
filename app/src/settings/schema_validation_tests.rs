use schemars::SchemaGenerator;
use settings::schema::SettingSchemaEntry;
use settings_value::SettingsValue;
use warpui::accessibility::AccessibilityVerbosity;

fn entries() -> Vec<&'static SettingSchemaEntry> {
    inventory::iter::<SettingSchemaEntry>.into_iter().collect()
}

/// Wraps a generated schema in a root JSON-Schema document, folding in the
/// `$defs` the generator accumulated so that `$ref`s inside the schema resolve
/// during validation.
fn root_schema_document(
    schema: schemars::Schema,
    schema_gen: &mut SchemaGenerator,
) -> serde_json::Value {
    let mut root = serde_json::Map::new();
    root.insert(
        "$schema".to_string(),
        serde_json::Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    if let serde_json::Value::Object(obj) = schema.to_value() {
        for (k, v) in obj {
            root.insert(k, v);
        }
    }
    let defs = schema_gen.take_definitions(true);
    if !defs.is_empty() {
        root.insert("$defs".to_string(), serde_json::Value::Object(defs));
    }
    serde_json::Value::Object(root)
}

/// Validates that every registered setting's file default value conforms to
/// its generated JSON schema.
///
/// This catches mismatches where `SettingsValue::to_file_value` produces
/// a shape that differs from what `file_schema` declares (e.g. Duration
/// serialized as integer seconds vs. the schemars-derived `{secs, nanos}`
/// object).
///
/// Because this test lives in the app crate, all real settings are linked
/// via `inventory`, giving full coverage of every setting in the application.
#[test]
fn file_defaults_validate_against_schema() {
    let mut failures = Vec::new();

    for entry in entries() {
        // Skip private settings — they have no toml_path and aren't in the
        // user-visible schema.
        if entry.is_private {
            continue;
        }

        // Generate the type's schema with a fresh generator so $defs accumulate.
        let mut schema_gen = SchemaGenerator::default();
        let schema = (entry.schema_fn)(&mut schema_gen);
        let root_value = root_schema_document(schema, &mut schema_gen);

        // Parse the file default value.
        let default_json = (entry.file_default_value_fn)();
        let default_value: serde_json::Value =
            serde_json::from_str(&default_json).unwrap_or_else(|e| {
                panic!(
                    "file_default_value_fn for '{}' produced invalid JSON: {e}",
                    entry.storage_key
                )
            });

        // Validate.
        if let Err(err) = jsonschema::draft202012::validate(&root_value, &default_value) {
            failures.push(format!(
                "  '{}': default {default_json} — {err}",
                entry.storage_key,
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "File default values that do not match their schema:\n{}",
        failures.join("\n")
    );
}

/// Every `AccessibilityVerbosity` variant — not just the default — must spell the
/// same way in the settings file as the generated schema advertises, and must
/// parse back from that spelling.
///
/// #638 reported `accessibility.accessibility_verbosity` as a schema/parser
/// mismatch: the enum carries `#[serde(rename = "VERBOSE")]` while the schema
/// advertises `verbose`. It is not a mismatch. The settings file is read through
/// `SettingsValue`, not serde (see the `settings_value` module docs — serde is
/// reached only by cloud sync and the platform-native stores), and the derive
/// snake-cases an explicit serde rename (`settings_value_derive::file_variant_name`),
/// so `"VERBOSE"` becomes the file spelling `verbose` — exactly what
/// `#[schemars(rename = "verbose")]` puts in the schema.
///
/// `file_defaults_validate_against_schema` above only exercises each setting's
/// *default*, so it would keep passing if a non-default variant drifted. This
/// covers every variant and pins the exact file spelling, so a change to either
/// rename shows up as a red test instead of a silently rejected user setting.
#[test]
fn accessibility_verbosity_file_values_match_schema() {
    let mut schema_gen = SchemaGenerator::default();
    let schema = AccessibilityVerbosity::file_schema(&mut schema_gen);
    let root_value = root_schema_document(schema, &mut schema_gen);

    for (variant, file_spelling) in [
        (AccessibilityVerbosity::Verbose, "verbose"),
        (AccessibilityVerbosity::Concise, "concise"),
    ] {
        let file_value = variant.to_file_value();
        assert_eq!(
            file_value,
            serde_json::Value::String(file_spelling.to_string()),
            "{variant:?} should be written to the settings file as {file_spelling:?}"
        );
        assert!(
            jsonschema::draft202012::validate(&root_value, &file_value).is_ok(),
            "{variant:?} is written as {file_value} but the schema does not accept that value"
        );
        assert_eq!(
            AccessibilityVerbosity::from_file_value(&file_value),
            Some(variant),
            "the value the schema advertises for {variant:?} must parse back to it"
        );
    }

    // Recorded, not endorsed: the serde spelling is NOT accepted from the settings
    // file — `from_file_value` returns `None`, the setting is skipped and the user
    // silently keeps the default. `docs/manual/08` told users to write `VERBOSE`
    // until #638 corrected it. If an alias is ever added so both spellings parse,
    // this is the assertion to update.
    assert_eq!(
        AccessibilityVerbosity::from_file_value(&serde_json::Value::String("VERBOSE".to_string())),
        None,
        "settings-file parsing is snake_case; if this now parses, the manual and this test need \
         updating together"
    );
}
