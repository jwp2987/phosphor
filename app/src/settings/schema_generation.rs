//! Generates the JSON Schema describing Phosphor's user-facing settings.
//!
//! Moved out of a separate `[[bin]]` (`generate_settings_schema`) and into
//! the main binary as the `dump-settings-schema` subcommand
//! (`crates/warp_cli`'s `Command::DumpSettingsSchema`, dispatched from
//! `app/src/lib.rs::run()`), mirroring upstream `83b4c101e` ("Optimize
//! settings schema generation during release process"): a standalone
//! generator binary recompiled and relinked the whole `warp` dependency
//! graph a second time for no reason other than emitting this one file,
//! which cost real release-pipeline time and, on macOS, could not even load
//! `Sentry.framework` (the generator ran outside the app bundle with no
//! runtime search path for it) -- exactly the kind of problem that goes away
//! once schema generation runs inside the binary that is actually shipped.
//!
//! `ensure_settings_linked()`, the old binary's `black_box` hack to force the
//! linker to keep `inventory::submit!` registrations scattered across the
//! app crate, is gone: this code now lives inside the `warp` library crate
//! itself, which the shipped binary already links in full, so there is no
//! separate, smaller link unit for those registrations to be dropped from.
//!
//! Unlike the old binary, `settings_schema_json` below takes a feature-flag
//! predicate rather than mutating global [`FeatureFlag`] state via
//! `FeatureFlag::set_enabled`. This matters because this now runs inside the
//! same process as the real app, which has already called
//! `init_feature_flags()` for its actual channel by the time a
//! `dump-settings-schema` command is dispatched (see `app/src/lib.rs::run()`)
//! -- `set_enabled` only ever turns a flag on, never off, so mutating global
//! state here could leak the real channel's flags into a schema requested
//! for a different `--channel`. A predicate closure asks "is this flag in
//! the requested channel's set" directly, with no dependency on -- or effect
//! on -- global state, which is also what makes it something a unit test can
//! call directly.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result};
use schemars::SchemaGenerator;
use serde_json::{Map, Value};

use settings::schema::SettingSchemaEntry;
use warp_core::features::{DEBUG_FLAGS, DOGFOOD_FLAGS, FeatureFlag, PREVIEW_FLAGS, RELEASE_FLAGS};

/// Builds the settings JSON schema for `channel` (`dev`, `preview`, or
/// `stable`, defaulting to `dev`) and writes it to `output_path`, or prints
/// it to standard output when no path is given.
///
/// Entry point for `dump-settings-schema` (see `crates/warp_cli`'s
/// `Command::DumpSettingsSchema`, dispatched from `app/src/lib.rs::run()`).
pub(crate) fn dump_settings_schema(
    channel: Option<&str>,
    output_path: Option<&Path>,
) -> Result<()> {
    let channel = channel.unwrap_or("dev");
    let active_flags = active_flags_for_channel(channel);
    let output = settings_schema_json(channel, |flag| active_flags.contains(&flag))?;

    if let Some(path) = output_path {
        std::fs::File::create(path)
            .with_context(|| format!("failed to create output file '{}'", path.display()))?
            .write_all(output.as_bytes())
            .with_context(|| format!("failed to write to '{}'", path.display()))?;
        eprintln!("Wrote settings schema to {}", path.display());
    } else {
        println!("{output}");
    }

    Ok(())
}

/// Builds the settings JSON schema, consulting `is_flag_enabled` for every
/// setting gated behind a [`FeatureFlag`]. Takes a predicate rather than
/// reading global feature-flag state directly (see the module doc for why),
/// which also makes this callable from a unit test with no global side
/// effects at all.
fn settings_schema_json(
    channel: &str,
    is_flag_enabled: impl Fn(FeatureFlag) -> bool,
) -> Result<String> {
    let mut generator = SchemaGenerator::default();
    let mut root_properties = Map::new();
    let mut entry_count = 0;

    for entry in inventory::iter::<SettingSchemaEntry> {
        // Skip private settings
        if entry.is_private {
            continue;
        }

        // Skip settings whose feature flag is not active
        if entry
            .feature_flag
            .is_some_and(|flag| !is_flag_enabled(flag))
        {
            continue;
        }

        let type_schema = (entry.schema_fn)(&mut generator);
        let mut schema_value: Value = type_schema.to_value();

        // Compute default value — prefer file default over serde default
        let default_json = (entry.file_default_value_fn)();
        if let Ok(default_value) = serde_json::from_str::<Value>(&default_json) {
            if let Some(obj) = schema_value.as_object_mut() {
                obj.insert("default".to_string(), default_value);
            }
        }

        // Always overwrite description with the macro-provided one
        if !entry.description.is_empty() {
            if let Some(obj) = schema_value.as_object_mut() {
                obj.insert(
                    "description".to_string(),
                    Value::String(entry.description.to_string()),
                );
            }
        }

        // Place the setting in the hierarchy
        let target = if let Some(hierarchy) = entry.hierarchy {
            ensure_hierarchy(&mut root_properties, hierarchy)
        } else {
            &mut root_properties
        };

        target.insert(entry.storage_key.to_string(), schema_value);
        entry_count += 1;
    }

    // Collect $defs from the generator
    let defs_map = generator.take_definitions(true);

    // Assemble the root document
    let mut root = Map::new();
    root.insert(
        "$schema".to_string(),
        Value::String("https://json-schema.org/draft/2020-12/schema".to_string()),
    );
    root.insert(
        "title".to_string(),
        Value::String("Phosphor Settings".to_string()),
    );
    root.insert(
        "description".to_string(),
        Value::String(format!(
            "JSON Schema for Phosphor settings ({channel} channel, {entry_count} settings)"
        )),
    );
    root.insert("type".to_string(), Value::String("object".to_string()));
    root.insert("properties".to_string(), Value::Object(root_properties));

    if !defs_map.is_empty() {
        root.insert("$defs".to_string(), Value::Object(defs_map));
    }

    // Strip type-derived numeric metadata (minimum, maximum, format) that
    // schemars emits from Rust primitive bounds (e.g. u8 → max 255).
    // These leak implementation details rather than semantic constraints.
    let mut root_value = Value::Object(root);
    strip_numeric_metadata(&mut root_value);
    strip_empty_enum_entries(&mut root_value);

    serde_json::to_string_pretty(&root_value).context("settings schema should serialize")
}

/// Which flags are active for a simulated channel — the same three tiers
/// `crates/warp_core/src/features.rs`'s `RELEASE_FLAGS`/`PREVIEW_FLAGS`/
/// `DOGFOOD_FLAGS`/`DEBUG_FLAGS` describe, plus the compile-time GPU default
/// on Windows.
fn active_flags_for_channel(channel: &str) -> HashSet<FeatureFlag> {
    let mut flags = HashSet::new();

    let flag_lists: &[&[FeatureFlag]] = match channel {
        "stable" => &[RELEASE_FLAGS],
        "preview" => &[RELEASE_FLAGS, PREVIEW_FLAGS],
        "dev" => &[RELEASE_FLAGS, PREVIEW_FLAGS, DOGFOOD_FLAGS, DEBUG_FLAGS],
        other => {
            eprintln!("Unknown channel '{other}', defaulting to dev");
            &[RELEASE_FLAGS, PREVIEW_FLAGS, DOGFOOD_FLAGS, DEBUG_FLAGS]
        }
    };

    for list in flag_lists {
        for flag in *list {
            flags.insert(*flag);
        }
    }

    #[cfg(feature = "windows_high_performance_gpu_default")]
    flags.insert(FeatureFlag::WindowsHighPerformanceGpuDefault);

    flags
}

/// Creates intermediate hierarchy objects so that a setting at e.g.
/// `appearance.text` is nested under `properties.appearance.properties.text.properties`.
fn ensure_hierarchy<'a>(
    root_properties: &'a mut Map<String, Value>,
    hierarchy: &str,
) -> &'a mut Map<String, Value> {
    let mut current = root_properties;

    for segment in hierarchy.split('.') {
        // Ensure the segment object exists
        let entry = current.entry(segment.to_string()).or_insert_with(|| {
            Value::Object({
                let mut m = Map::new();
                m.insert("type".to_string(), Value::String("object".to_string()));
                m.insert("properties".to_string(), Value::Object(Map::new()));
                m
            })
        });

        // Navigate into its properties
        current = entry
            .as_object_mut()
            .expect("hierarchy node should be an object")
            .entry("properties")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .expect("properties should be an object");
    }

    current
}

/// Recursively strips `minimum`, `maximum`, and `format` from integer and
/// number schemas. schemars derives these from Rust type bounds (e.g. `u8`
/// -> `minimum: 0, maximum: 255, format: "uint8"`), which are misleading
/// for settings whose valid domain is narrower than the type allows.
fn strip_numeric_metadata(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let is_numeric = map
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| t == "integer" || t == "number");

            if is_numeric {
                map.remove("minimum");
                map.remove("maximum");
                map.remove("format");
            }

            for val in map.values_mut() {
                strip_numeric_metadata(val);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                strip_numeric_metadata(val);
            }
        }
        _ => {}
    }
}

/// Removes `{"enum": [], "type": "string"}` entries from `oneOf` arrays.
/// schemars emits an empty enum bucket for externally-tagged enums when all
/// unit variants have individual descriptions (and are therefore promoted to
/// separate `oneOf` branches with `const`). The empty bucket is unreachable
/// and confuses schema consumers.
fn strip_empty_enum_entries(value: &mut Value) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(one_of)) = map.get_mut("oneOf") {
                one_of.retain(|entry| {
                    !matches!(entry, Value::Object(obj)
                        if obj.get("enum").is_some_and(|e| e.as_array().is_some_and(|a| a.is_empty()))
                    )
                });
            }

            for val in map.values_mut() {
                strip_empty_enum_entries(val);
            }
        }
        Value::Array(arr) => {
            for val in arr {
                strip_empty_enum_entries(val);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
#[path = "schema_generation_tests.rs"]
mod tests;
