//! Unit tests for the `search_codebase` descriptor.

use super::*;

/// The descriptor must be intercepted upstream, never routed through the protobuf executor:
/// `from_args` always errors and `result_to_json` always yields `None` (mirrors websearch).
#[test]
fn from_args_always_errors() {
    let err = (SEARCH_CODEBASE.from_args)(r#"{"query":"x"}"#).unwrap_err();
    assert!(
        err.to_string().contains("intercepted"),
        "from_args error must explain interception: {err}"
    );
}

#[test]
fn descriptor_name_and_schema() {
    assert_eq!(SEARCH_CODEBASE.name, "search_codebase");
    assert_eq!(TOOL_NAME, "search_codebase");

    let schema = (SEARCH_CODEBASE.parameters)();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["query"]["type"], "string");
    assert_eq!(schema["properties"]["max_results"]["type"], "integer");
    // `query` is required; `max_results` is optional.
    assert_eq!(schema["required"], serde_json::json!(["query"]));
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn description_is_local_only_and_non_empty() {
    assert!(!SEARCH_CODEBASE.description.is_empty());
    let d = SEARCH_CODEBASE.description.to_lowercase();
    assert!(d.contains("local"), "description must advertise local-only search");
    // Must not imply any cloud/network capability.
    assert!(!d.contains("cloud search") && !d.contains("upload"));
}

/// The descriptor is registered in the shared REGISTRY so it is advertised to the model.
#[test]
fn registered_in_registry() {
    assert!(
        super::super::lookup("search_codebase").is_some(),
        "search_codebase must be present in tools::REGISTRY"
    );
}
