//! Unit tests for the `get_relevant_files` descriptor.

use super::*;

/// The descriptor must be intercepted upstream, never routed through the protobuf
/// executor: `from_args` always errors and `result_to_json` always yields `None`.
#[test]
fn from_args_always_errors() {
    let err = (GET_RELEVANT_FILES.from_args)(r#"{"query":"x"}"#).unwrap_err();
    assert!(
        err.to_string().contains("intercepted"),
        "from_args error must explain interception: {err}"
    );
}

#[test]
fn descriptor_name_and_schema() {
    assert_eq!(GET_RELEVANT_FILES.name, "get_relevant_files");
    assert_eq!(TOOL_NAME, "get_relevant_files");

    let schema = (GET_RELEVANT_FILES.parameters)();
    assert_eq!(schema["type"], "object");
    assert_eq!(schema["properties"]["query"]["type"], "string");
    // `query` is the only field and it is required.
    assert_eq!(schema["required"], serde_json::json!(["query"]));
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn description_is_local_only_and_non_empty() {
    assert!(!GET_RELEVANT_FILES.description.is_empty());
    let d = GET_RELEVANT_FILES.description.to_lowercase();
    assert!(d.contains("local"), "description must advertise local-only operation");
    // Must not advertise any cloud/network capability (saying "no cloud access" is fine).
    assert!(!d.contains("cloud search") && !d.contains("upload"));
}

/// The descriptor is registered in the shared REGISTRY so it is advertised to the model.
#[test]
fn registered_in_registry() {
    assert!(
        super::super::lookup("get_relevant_files").is_some(),
        "get_relevant_files must be present in tools::REGISTRY"
    );
}
