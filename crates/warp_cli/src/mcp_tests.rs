// Port audit against the pinned oracle (02b53fcd8, ORACLE.md), #2 sweep,
// docs/sweep/warp-cli.md: the pin's `mcp_tests.rs` has 3 tests with no fork
// equivalent -- `test_parse_well_known_integration_id`,
// `test_bare_identifier_treated_as_well_known`,
// `test_bare_identifier_treated_as_json_when_flag_disabled`. All three cover
// `MCPSpec::WellKnown`, gated on `FeatureFlag::WellKnownMcpIds`: a bare
// identifier (e.g. "linear") resolved to a real MCP server config by Warp's
// *server* at run setup ("the server owns the set of recognized ids", per the
// pin's own doc comment on `MCPSpec::WellKnown`). This fork's `MCPSpec` has
// only `Uuid`/`Json` -- no `WellKnown` variant, no `WellKnownMcpIds` flag --
// and the decision not to add them is already made and documented at
// `app/src/ai/agent_sdk/mcp_config.rs`: "Do not port the well-known variant --
// it would add a second spec the driver can construct but never resolve."
// CLOUD, not test debt.

use super::*;
use clap::builder::TypedValueParser;
use std::ffi::OsStr;

fn parse_mcp_spec(value: &str) -> Result<MCPSpec, clap::Error> {
    let cmd = clap::Command::new("test");
    let parser = MCPSpecParser;
    parser.parse_ref(&cmd, None, OsStr::new(value))
}

#[test]
fn test_parse_uuid() {
    let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
    let result = parse_mcp_spec(uuid_str).unwrap();
    match result {
        MCPSpec::Uuid(uuid) => assert_eq!(uuid.to_string(), uuid_str),
        MCPSpec::Json(_) => panic!("Expected Uuid variant"),
    }
}

#[test]
fn test_parse_inline_json_cli_server() {
    let json = r#"{"server-name": {"command": "npx", "args": ["-y", "mcp-server"]}}"#;
    let result = parse_mcp_spec(json).unwrap();
    match result {
        MCPSpec::Json(s) => assert_eq!(s, json),
        MCPSpec::Uuid(_) => panic!("Expected Json variant"),
    }
}

#[test]
fn test_parse_inline_json_single_server() {
    let json = r#"{"command": "npx", "args": ["-y", "mcp-server"]}"#;
    let result = parse_mcp_spec(json).unwrap();
    match result {
        MCPSpec::Json(s) => assert_eq!(s, json),
        MCPSpec::Uuid(_) => panic!("Expected Json variant"),
    }
}

#[test]
fn test_parse_inline_json_sse_server() {
    let json = r#"{"url": "http://localhost:3000/mcp", "headers": {"API_KEY": "value"}}"#;
    let result = parse_mcp_spec(json).unwrap();
    match result {
        MCPSpec::Json(s) => assert_eq!(s, json),
        MCPSpec::Uuid(_) => panic!("Expected Json variant"),
    }
}

#[test]
fn test_parse_inline_json_mcp_servers_wrapper() {
    let json = r#"{"mcpServers": {"server-name": {"command": "npx", "args": []}}}"#;
    let result = parse_mcp_spec(json).unwrap();
    match result {
        MCPSpec::Json(s) => assert_eq!(s, json),
        MCPSpec::Uuid(_) => panic!("Expected Json variant"),
    }
}

#[test]
fn test_uuid_takes_precedence_over_json() {
    // A valid UUID should be parsed as UUID, not as JSON
    let uuid_str = "550e8400-e29b-41d4-a716-446655440000";
    let result = parse_mcp_spec(uuid_str).unwrap();
    assert!(matches!(result, MCPSpec::Uuid(_)));
}

#[test]
fn test_invalid_uuid_treated_as_json() {
    // An invalid UUID that looks like it could be one should be treated as JSON
    let invalid_uuid = "not-a-valid-uuid";
    let result = parse_mcp_spec(invalid_uuid).unwrap();
    assert!(matches!(result, MCPSpec::Json(_)));
}

#[test]
fn test_non_identifier_non_json_treated_as_json() {
    // Anything that isn't a UUID and isn't an existing file falls through to
    // the inline-JSON path (and fails JSON parsing later).
    let result = parse_mcp_spec("missing-config.json").unwrap();
    assert!(matches!(result, MCPSpec::Json(_)));
}
