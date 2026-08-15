//! Prompt cache serialization stability test suite (corresponds to doc sections
//! P1-8 / P1-9 / P1-13).
//!
//! Anthropic's docs explicitly warn:
//! > Verify that the keys in your `tool_use` content blocks have stable
//! > ordering as some languages (for example, Swift, Go) randomize key order
//! > during JSON conversion, breaking caches
//!
//! This means any `serde_json::Value` produced on the Rust side **must**:
//!   1. Be byte-equal across calls for the same input (deterministic)
//!   2. Not depend on `HashMap` iteration order
//!   3. Not depend on external state (timestamps, randomness, PID, etc.)
//!
//! This test suite is Zap's "anti-regression guardrail" —— any future change to the
//! prompt construction path that breaks byte-level stability will fail an assertion
//! here.

// `warp_multi_agent_api` is an external pinned proto crate
// (warpdotdev/warp-proto-apis). Several of its fields carry `[deprecated = true]`
// in the .proto, but the deprecation is aspirational -- e.g. `InputContext::
// executed_shell_commands` is marked "TODO: these fields should be _attachments_,
// not part of the input context" with no replacement field defined yet. The
// generated Rust structs still require every field to be initialised, so
// constructing one in a test cannot avoid naming them.
#![allow(deprecated)]


use crate::ai::agent::{MCPContext, MCPServer};
use api::message;
use warp_multi_agent_api as api;

use super::chat_stream;
use super::tools;

// ---------------------------------------------------------------------------
// P1-8: tool schema field order stability
// ---------------------------------------------------------------------------

/// Calls `(parameters)()` twice for each tool in `REGISTRY`, asserting byte-equality.
///
/// Risk: if the enum / oneof nested inside a tool schema converts to Value via a
/// `HashMap<String, Schema>`, the order gets scrambled. The `serde_json::Map`
/// produced by a `json!({...})` literal preserves **insertion order** by default
/// (`preserve_order` is on by default, see Cargo.toml), so a literally hardcoded key
/// order is stable across calls. This test guards that invariant.
#[test]
fn registry_tool_schemas_are_deterministic() {
    for tool in tools::REGISTRY {
        let s1 = (tool.parameters)();
        let s2 = (tool.parameters)();
        let j1 = serde_json::to_string(&s1).unwrap();
        let j2 = serde_json::to_string(&s2).unwrap();
        assert_eq!(
            j1, j2,
            "tool `{}`'s schema must be byte-equal across calls (a prerequisite for prompt cache hits)",
            tool.name
        );
    }
}

/// Calls each tool in `REGISTRY` repeatedly 50 times, asserting all calls produce
/// byte-equal output.
/// Guards against occasional HashMap iteration order drift (running just twice might
/// happen to match).
#[test]
fn registry_tool_schemas_stable_under_repetition() {
    for tool in tools::REGISTRY {
        let baseline = serde_json::to_string(&(tool.parameters)()).unwrap();
        for i in 0..50 {
            let candidate = serde_json::to_string(&(tool.parameters)()).unwrap();
            assert_eq!(
                baseline, candidate,
                "tool `{}`'s output on call {i} does not match the baseline (possible HashMap order drift)",
                tool.name
            );
        }
    }
}

/// `tools::REGISTRY`'s own order is static, but we verify it anyway: iterating
/// multiple times within the same process yields the same (name, description)
/// sequence.
#[test]
fn registry_iteration_order_is_stable() {
    let names1: Vec<&str> = tools::REGISTRY.iter().map(|t| t.name).collect();
    let names2: Vec<&str> = tools::REGISTRY.iter().map(|t| t.name).collect();
    assert_eq!(names1, names2);
}

// ---------------------------------------------------------------------------
// P1-9: serialize_outgoing_tool_call historical replay stability
// ---------------------------------------------------------------------------

/// Simulates a Grep tool call, verifying that serializing it twice produces
/// byte-equal output.
/// `serialize_outgoing_tool_call` reruns on every build_chat_request, converting a
/// historical turn's ToolCall into (name, args Value). Any HashMap- or time-related
/// instability would invalidate the cache for the back half of the messages segment.
///
/// Grep is chosen because its fields are the simplest (`queries: Vec<String>`,
/// `path: String`), with no dependency on implicit default prost fields.
#[test]
fn serialize_grep_tool_call_is_deterministic() {
    let tc = message::ToolCall {
        tool_call_id: "call-grep-1".to_owned(),
        tool: Some(message::tool_call::Tool::Grep(message::tool_call::Grep {
            queries: vec!["fn main".to_owned(), "Result<".to_owned()],
            path: "src/".to_owned(),
        })),
    };

    let (n1, v1) = chat_stream::serialize_outgoing_tool_call_for_test(&tc, None, "");
    let (n2, v2) = chat_stream::serialize_outgoing_tool_call_for_test(&tc, None, "");
    assert_eq!(n1, n2, "tool name must match");
    let j1 = serde_json::to_string(&v1).unwrap();
    let j2 = serde_json::to_string(&v2).unwrap();
    assert_eq!(j1, j2, "the same ToolCall must be byte-equal across serializations");
}

/// Grep's `queries` is a `Vec<String>`, and order must be stable (a Vec is naturally
/// stable, but this is asserted defensively).
/// This reflects a broader rule: any Vec field inside a user ToolCall must preserve
/// input order.
#[test]
fn serialize_grep_preserves_queries_order() {
    let tc = message::ToolCall {
        tool_call_id: "call-grep-2".to_owned(),
        tool: Some(message::tool_call::Tool::Grep(message::tool_call::Grep {
            queries: vec!["zzz".to_owned(), "aaa".to_owned()],
            path: ".".to_owned(),
        })),
    };
    let (_, v) = chat_stream::serialize_outgoing_tool_call_for_test(&tc, None, "");
    let s = serde_json::to_string(&v).unwrap();
    let pos_z = s.find("zzz").expect("queries should contain zzz");
    let pos_a = s.find("aaa").expect("queries should contain aaa");
    assert!(pos_z < pos_a, "Vec order must be preserved as given (zzz first, aaa second)");
}

/// MCP tool call contains a `prost_types::Struct`; verifies serialization stability.
/// `prost_types::Struct.fields` internally uses a `BTreeMap`, which is inherently
/// stable — this test just confirms it.
#[test]
fn serialize_mcp_tool_call_is_deterministic() {
    use prost_types::{value::Kind, Struct, Value as ProstValue};
    use std::collections::BTreeMap;

    let mut fields = BTreeMap::new();
    fields.insert(
        "key_z".to_owned(),
        ProstValue {
            kind: Some(Kind::StringValue("v_z".to_owned())),
        },
    );
    fields.insert(
        "key_a".to_owned(),
        ProstValue {
            kind: Some(Kind::NumberValue(42.0)),
        },
    );

    let server_id = "srv-uuid-1".to_owned();
    let tc = message::ToolCall {
        tool_call_id: "call-mcp-1".to_owned(),
        tool: Some(message::tool_call::Tool::CallMcpTool(
            message::tool_call::CallMcpTool {
                name: "echo".to_owned(),
                args: Some(Struct { fields }),
                server_id: server_id.clone(),
            },
        )),
    };

    // Build an mcp_context so sanitize_server_name can look up the server name
    let ctx = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![MCPServer {
            id: server_id.clone(),
            name: "my-server".to_owned(),
            description: String::new(),
            resources: vec![],
            tools: vec![],
        }],
    };

    let (n1, v1) = chat_stream::serialize_outgoing_tool_call_for_test(&tc, Some(&ctx), "");
    let (n2, v2) = chat_stream::serialize_outgoing_tool_call_for_test(&tc, Some(&ctx), "");
    assert_eq!(n1, n2);
    let j1 = serde_json::to_string(&v1).unwrap();
    let j2 = serde_json::to_string(&v2).unwrap();
    assert_eq!(j1, j2);
    // BTreeMap should output in key lexicographic order (key_a before key_z)
    let pos_a = j1.find("key_a").expect("should contain key_a");
    let pos_z = j1.find("key_z").expect("should contain key_z");
    assert!(
        pos_a < pos_z,
        "prost_types::Struct should follow BTreeMap key lexicographic order"
    );
}

// ---------------------------------------------------------------------------
// Issue #245: carrier with invalid JSON args must fall back to empty object
// ---------------------------------------------------------------------------

/// When the model emits a tool call with invalid JSON escape sequences (e.g. `\e`, `\``),
/// the carrier message stores the raw string in server_message_data.
/// On the next turn, serialize_outgoing_tool_call must return a Value::Object (not
/// Value::String) so that genai serializes it as a JSON object for `arguments`,
/// not a doubly-wrapped JSON string that the provider would reject with "Invalid \escape".
#[test]
fn carrier_with_invalid_json_args_falls_back_to_empty_object() {
    use api::message;
    // Simulate a carrier message: tool = None, server_message_data = "fn_name\n<invalid_json>"
    let tc = message::ToolCall {
        tool_call_id: "call-invalid".to_owned(),
        tool: None,
    };
    let server_message_data = "shell\n{\"command\": \"echo \\epath\"}";

    let (fn_name, args_value) =
        chat_stream::serialize_outgoing_tool_call_for_test(&tc, None, server_message_data);

    assert_eq!(fn_name, "shell");
    // Must NOT be a String (which would cause double-wrapping and Invalid \escape on the wire)
    assert!(
        !args_value.is_string(),
        "args must be a JSON object/value, not a raw string (would cause Invalid \\escape)"
    );
    // Must be a valid JSON value that serde_json can serialize
    let serialized = serde_json::to_string(&args_value).expect("args_value must be serializable");
    // The serialized form must itself be valid JSON
    serde_json::from_str::<serde_json::Value>(&serialized)
        .expect("serialized args must be valid JSON");
}

// ---------------------------------------------------------------------------
// P1-13: build_tools_array overall stability (works with P0-3's MCP ordering)
// ---------------------------------------------------------------------------

/// End-to-end assertion: running tools array assembly twice for the same
/// `(REGISTRY + same mcp_context)` produces byte-equal strings. This covers the key
/// stability constraint for the tools array in the prompt (per Anthropic's docs:
/// changing tool definitions invalidates the entire cache).
///
/// We don't call `build_tools_array(params: &RequestParams)` directly because
/// `RequestParams` has too many fields to construct easily; instead this replicates
/// its core assembly logic for the REGISTRY and mcp parts.
#[test]
fn full_tools_array_serialization_is_stable() {
    let assemble = || -> String {
        let mut buf = String::new();
        // Built-in tools (REGISTRY iteration order is static)
        for t in tools::REGISTRY {
            buf.push_str(t.name);
            buf.push('|');
            buf.push_str(t.description);
            buf.push('|');
            let schema = (t.parameters)();
            buf.push_str(&serde_json::to_string(&schema).unwrap());
            buf.push('\n');
        }
        // MCP tools (already sorted inside build_mcp_tool_defs; empty when there's
        // no ctx)
        buf
    };
    let a = assemble();
    let b = assemble();
    assert_eq!(a.len(), b.len());
    assert_eq!(a, b, "tools array serialization must be byte-equal across calls");
}

/// End-to-end assembly stability with an MCP server (works with P0-3's ordering
/// guarantee).
#[test]
fn full_tools_array_with_mcp_is_stable() {
    use rmcp::model::{AnnotateAble, RawResource, Tool as McpTool};
    use serde_json::json;
    use std::sync::Arc;

    let schema_obj = json!({
        "type": "object",
        "properties": { "x": { "type": "string" } }
    })
    .as_object()
    .unwrap()
    .clone();

    let server_a = MCPServer {
        id: "id-a".to_owned(),
        name: "server-a".to_owned(),
        description: String::new(),
        resources: vec![RawResource::new("file:///x.txt", "X").no_annotation()],
        tools: vec![
            McpTool::new("zeta", "Z desc", Arc::new(schema_obj.clone())),
            McpTool::new("alpha", "A desc", Arc::new(schema_obj.clone())),
        ],
    };
    let ctx1 = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![server_a.clone()],
    };
    // Rebuild the same ctx once more (servers Vec order is the same):
    let ctx2 = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![server_a],
    };

    let assemble = |ctx: &MCPContext| -> String {
        let mut buf = String::new();
        for t in tools::REGISTRY {
            buf.push_str(t.name);
            buf.push('|');
            buf.push_str(t.description);
            buf.push('|');
            let schema = (t.parameters)();
            buf.push_str(&serde_json::to_string(&schema).unwrap());
            buf.push('\n');
        }
        for (name, desc, schema) in tools::mcp::build_mcp_tool_defs(ctx) {
            buf.push_str(&name);
            buf.push('|');
            buf.push_str(&desc);
            buf.push('|');
            buf.push_str(&serde_json::to_string(&schema).unwrap());
            buf.push('\n');
        }
        buf
    };

    let a = assemble(&ctx1);
    let b = assemble(&ctx2);
    assert_eq!(a, b, "the tools array with MCP must be byte-equal across calls");
    // Verify MCP tools are in function_name lexicographic order (alpha before zeta)
    let pos_alpha = a.find("mcp__server-a__alpha").expect("should contain alpha");
    let pos_zeta = a.find("mcp__server-a__zeta").expect("should contain zeta");
    assert!(pos_alpha < pos_zeta, "P0-3 ordering guarantee: alpha < zeta");
}

// ---------------------------------------------------------------------------
// Message content stability across turns (live rendering vs historical replay)
// ---------------------------------------------------------------------------
//
// This section targets a whole class of real bugs: **the same logical message
// renders different text when it's "sent live" versus "replayed as history"**.
//
// Why this is fatal: local inference services like FLM compare the cache message by
// message, **and stop at the first mismatch** (shows up in logs as
// `matched 0 of N` + `first divergence at message [i]`). Any drift in a historical
// message's content forces everything after it to be re-prefilled —— measured at
// roughly 10K tokens / 33s on a local 9B model.
//
// Known pitfalls we've hit:
//   1. `<env>` living in the system prompt — a single `cd` changes message[0] (full
//      recompute). → Moved to an `<environment_context>` block at the end of the
//      message list.
//   2. `<attached_context>` (auto-attached executed commands) only rendered on the
//      live turn, never reconstructed on replay, shrinking the message from long
//      text down to a bare query.
//   3. On replay, `command_id` got filled with a default value, so even reconstructed
//      content wouldn't match.
//
// The assertions below are the guardrails for these three issues.

use crate::ai::agent::api::convert_conversation::convert_input_context;
use crate::ai::agent::AIAgentContext;
use crate::ai::block_context::BlockContext;
use crate::ai::agent_providers::user_context;

fn sample_block(id: &str) -> BlockContext {
    BlockContext {
        id: id.to_string().into(),
        index: 0.into(),
        command: "ls -la".to_string(),
        output: "total 0\ndrwxr-xr-x 1 winters winters 0 Jul 19 13:00 .".to_string(),
        exit_code: 0.into(),
        is_auto_attached: true,
        started_ts: None,
        finished_ts: None,
        pwd: None,
        shell: None,
        username: None,
        hostname: None,
        git_branch: None,
        os: None,
        session_id: None,
    }
}

/// **Core guardrail**: auto-attached command blocks must render byte-identically
/// whether rendered live or via "persist → replay".
///
/// This assertion directly corresponds to bugs #2/#3. If someone fills a field like
/// `command_id` with a default value on the replay side again, or forgets to
/// reconstruct `<attached_context>` on the replay path, this fails immediately.
#[test]
fn attached_context_survives_persist_replay_byte_identical() {
    let block = sample_block("precmd-17844818512123-1");

    // live: render directly from the in-memory AIAgentContext
    let live_ctx = vec![AIAgentContext::Block(Box::new(block.clone()))];
    let live = user_context::collect_user_attachments(&live_ctx)
        .prefix
        .expect("live should render an attached_context");

    // replay: round-trip through the persisted proto and back
    let api_context = api::InputContext {
        executed_shell_commands: vec![block.into()],
        ..Default::default()
    };
    let replayed_ctx = convert_input_context(Some(&api_context));
    let replayed = user_context::collect_user_attachments(&replayed_ctx)
        .prefix
        .expect("replay should render an attached_context");

    assert_eq!(
        live, replayed,
        "attached_context drifted between live and replay —— \
         this would cause a prompt cache mismatch at this message, forcing a full \
         re-prefill of everything after it"
    );
}

/// `command_id` must round-trip through persistence unchanged, not be filled with a
/// default value.
#[test]
fn block_command_id_round_trips_through_persistence() {
    let block = sample_block("precmd-abc-42");
    let api_context = api::InputContext {
        executed_shell_commands: vec![block.into()],
        ..Default::default()
    };

    let restored = convert_input_context(Some(&api_context));
    let AIAgentContext::Block(b) = &restored[0] else {
        panic!("expected a Block to be restored, got {:?}", restored[0]);
    };
    assert_eq!(b.id.to_string(), "precmd-abc-42");
}

/// `<environment_context>` must **change with cwd** (complementing the system
/// prompt's constancy).
///
/// Both invariants are required: the constant system segment guarantees cache hits,
/// while the tracking tail block guarantees the model sees the real directory. The
/// old "freeze cwd" approach satisfied the former and violated the latter —— the
/// model was told the wrong directory.
#[test]
fn environment_tail_tracks_cwd_while_staying_deterministic() {
    let at = |pwd: &str| {
        user_context::render_environment_context(&[AIAgentContext::Directory {
            pwd: Some(pwd.into()),
            home_dir: None,
            are_file_symbols_indexed: false,
        }])
        .expect("should render")
    };

    // same input → same bytes
    assert_eq!(at("/home/winters"), at("/home/winters"));
    // different cwd → content actually changes
    assert_ne!(at("/home/winters"), at("/etc"));
    assert!(at("/etc").contains("Working directory: /etc"));
}
