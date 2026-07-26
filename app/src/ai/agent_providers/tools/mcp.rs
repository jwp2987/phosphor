//! Tool injection and bidirectional translation for MCP (Model Context Protocol)
//! servers.
//!
//! Unlike static tools like `shell.rs` / `files.rs`, MCP tools are **dynamic**:
//! each user-configured MCP server exposes its own list of tools (name +
//! description + JSON Schema), which need to be injected into the OpenAI tools
//! array on the fly, at each request-construction time, based on
//! `RequestParams.mcp_context`.
//!
//! ## Naming convention
//!
//! OpenAI function name: `mcp__<server_name_safe>__<tool_name>`
//! - Separated by double underscores, to avoid colliding with built-in tool names
//!   (which are underscore-separated words)
//! - server_name_safe = server.name with every character not in
//!   `[a-zA-Z0-9_-]` replaced with `_`
//!
//! ## Reverse resolution
//!
//! When a `mcp__`-prefixed name is seen:
//! 1. Split out `server_name_safe` and `tool_name`
//! 2. Match against `params.mcp_context.servers` by sanitized name, to get server.id
//! 3. Build `Message::ToolCall::CallMcpTool { name: tool_name, args, server_id }`
//!
//! ## Result serialization
//!
//! The result inside `ToolCallResultType::CallMcpTool(CallMcpToolResult)` is
//! structured MCP content, converted to JSON for the upstream model.

use anyhow::{anyhow, Result};
use prost_types::value::Kind as ProstKind;
use serde_json::{json, Map, Value};
use warp_multi_agent_api as api;

use crate::ai::agent::{MCPContext, MCPServer};

const PREFIX: &str = "mcp__";
const SEP: &str = "__";
/// Unified function name for reading an MCP resource (the uri spans servers, but
/// semantically this is a single tool).
const READ_RESOURCE_NAME: &str = "mcp_read_resource";

/// Convert server.name into a string safe to use as part of an OpenAI function name.
fn sanitize_server_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Generate an OpenAI function name for an MCP tool.
pub fn function_name(server: &MCPServer, tool_name: &str) -> String {
    format!(
        "{}{}{}{}",
        PREFIX,
        sanitize_server_name(&server.name),
        SEP,
        tool_name
    )
}

/// Determine whether a given OpenAI function name is an MCP call (covers both
/// dynamic mcp__-prefixed tool calls and the unified mcp_read_resource read).
pub fn is_mcp_function(name: &str) -> bool {
    name == READ_RESOURCE_NAME || name.starts_with(PREFIX)
}

/// Convert the tools of every server in mcp_context into OpenAI tool definitions
/// (name/description/parameters). Also, if at least one server exposes resources,
/// append a unified `mcp_read_resource` tool definition for the model to read
/// resources with.
/// Returns a triple `(name, description, parameters_value)` — the caller wraps it
/// into a ToolDef.
///
/// **P0-3 prompt cache optimization**: output is **stable in lexicographic order**.
/// Reason: Anthropic explicitly warns that any change to the tools field invalidates
/// every cache layer. The upstream dependency `ctx.servers`
/// (`MCPContext.servers: Vec<MCPServer>`) does not itself guarantee ordering
/// (HashMap iteration / process startup order / concurrent connections can all
/// cause the order to drift across requests). Here we sort by `function_name`
/// (which includes server.name and tool.name) to lock that down, then append
/// `mcp_read_resource` last (its fixed name doesn't participate in sorting).
pub fn build_mcp_tool_defs(ctx: &MCPContext) -> Vec<(String, String, Value)> {
    let mut out = Vec::new();
    for server in &ctx.servers {
        for tool in &server.tools {
            // rmcp::Tool.input_schema is Arc<Map<String,Value>>; clone it and wrap in Value::Object.
            let schema = Value::Object((*tool.input_schema).clone());
            let desc = tool
                .description
                .as_ref()
                .map(|d| d.to_string())
                .unwrap_or_default();
            let prefixed_desc = if desc.is_empty() {
                format!("Tool {} from MCP server `{}`", tool.name, server.name)
            } else {
                format!("[MCP/{}] {}", server.name, desc)
            };
            out.push((function_name(server, &tool.name), prefixed_desc, schema));
        }
    }
    // P0-3: sort by function_name in lexicographic order, to guarantee that the
    // same static context produces a consistent order across requests.
    // function_name is globally unique (`mcp__<server_safe>__<tool>`), so there's
    // no conflict when using it as the sort key.
    out.sort_by(|a, b| a.0.cmp(&b.0));

    // Only inject the read_resource tool if any server exposes resources, to avoid
    // the model calling it for nothing (the readable list is decided by the server).
    let any_resources = ctx.servers.iter().any(|s| !s.resources.is_empty());
    if any_resources {
        let mut available_uris: Vec<String> = Vec::new();
        for s in &ctx.servers {
            for r in &s.resources {
                available_uris.push(format!("[{}] {} ({})", s.name, r.name, r.uri));
            }
        }
        // P0-3: available_uris depends on ctx.servers order × server.resources
        // order, which likewise needs to be stable across requests. Sort by
        // literal lexicographic order to avoid HashMap iteration order drift.
        available_uris.sort();
        let desc = format!(
            "Read a resource exposed by an MCP server (file / database / API, etc.). \
             Available resources:\n- {}",
            available_uris.join("\n- ")
        );
        let schema = json!({
            "type": "object",
            "properties": {
                "uri": {
                    "type": "string",
                    "description": "Resource URI (pick from the available resources list)."
                },
                "server": {
                    "type": "string",
                    "description": "Optional: name of the MCP server owning the resource (matched after sanitization). Required when multiple servers expose the same uri."
                }
            },
            "required": ["uri"],
            "additionalProperties": false
        });
        out.push((READ_RESOURCE_NAME.to_owned(), desc, schema));
    }

    out
}

/// Reverse resolution: translate a `mcp__server__tool` or `mcp_read_resource` call
/// returned by the upstream model into warp's `Tool::CallMcpTool` or
/// `Tool::ReadMcpResource`.
/// Failure reasons: malformed name / server not found / args parse failure.
pub fn parse_mcp_tool_call(
    function_name: &str,
    arguments_json: &str,
    ctx: Option<&MCPContext>,
) -> Result<api::message::tool_call::Tool> {
    if function_name == READ_RESOURCE_NAME {
        return parse_read_resource(arguments_json, ctx);
    }
    let body = function_name
        .strip_prefix(PREFIX)
        .ok_or_else(|| anyhow!("not an MCP function name"))?;
    let (server_name_safe, tool_name) = body
        .split_once(SEP)
        .ok_or_else(|| anyhow!("malformed MCP function name (missing __): {function_name}"))?;

    let ctx = ctx.ok_or_else(|| anyhow!("MCP function called but no mcp_context present"))?;
    let server = ctx
        .servers
        .iter()
        .find(|s| sanitize_server_name(&s.name) == server_name_safe)
        .ok_or_else(|| anyhow!("MCP server `{server_name_safe}` not in current mcp_context"))?;

    // args: JSON object → prost_types::Struct
    let parsed: Value = if arguments_json.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(arguments_json)?
    };
    let obj = parsed
        .as_object()
        .ok_or_else(|| anyhow!("MCP tool args must be a JSON object"))?;
    let args_struct = json_object_to_prost_struct(obj);

    Ok(api::message::tool_call::Tool::CallMcpTool(
        api::message::tool_call::CallMcpTool {
            name: tool_name.to_owned(),
            args: Some(args_struct),
            server_id: server.id.clone(),
        },
    ))
}

fn json_object_to_prost_struct(obj: &Map<String, Value>) -> prost_types::Struct {
    let mut fields = std::collections::BTreeMap::new();
    for (k, v) in obj {
        fields.insert(k.clone(), json_value_to_prost(v));
    }
    prost_types::Struct {
        fields: fields.into_iter().collect(),
    }
}

fn json_value_to_prost(v: &Value) -> prost_types::Value {
    let kind = match v {
        Value::Null => ProstKind::NullValue(0),
        Value::Bool(b) => ProstKind::BoolValue(*b),
        Value::Number(n) => ProstKind::NumberValue(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => ProstKind::StringValue(s.clone()),
        Value::Array(arr) => ProstKind::ListValue(prost_types::ListValue {
            values: arr.iter().map(json_value_to_prost).collect(),
        }),
        Value::Object(o) => ProstKind::StructValue(json_object_to_prost_struct(o)),
    };
    prost_types::Value { kind: Some(kind) }
}

#[derive(Debug, serde::Deserialize)]
struct ReadResourceArgs {
    uri: String,
    #[serde(default)]
    server: Option<String>,
}

fn parse_read_resource(
    arguments_json: &str,
    ctx: Option<&MCPContext>,
) -> Result<api::message::tool_call::Tool> {
    let parsed: ReadResourceArgs = serde_json::from_str(arguments_json)?;
    // Resolve server_id:
    // 1) If a server name is given, match against it after sanitizing
    // 2) Otherwise, look across all servers for a resource with this uri (take the first hit)
    // 3) Fall back to an empty server_id (the server side locates it by uri itself)
    let server_id = if let Some(ctx) = ctx {
        match parsed.server.as_deref() {
            Some(name) => ctx
                .servers
                .iter()
                .find(|s| sanitize_server_name(&s.name) == sanitize_server_name(name))
                .map(|s| s.id.clone())
                .unwrap_or_default(),
            None => ctx
                .servers
                .iter()
                .find(|s| {
                    s.resources
                        .iter()
                        .any(|r| r.uri.as_str() == parsed.uri.as_str())
                })
                .map(|s| s.id.clone())
                .unwrap_or_default(),
        }
    } else {
        String::new()
    };
    Ok(api::message::tool_call::Tool::ReadMcpResource(
        api::message::tool_call::ReadMcpResource {
            uri: parsed.uri,
            server_id,
        },
    ))
}

/// Serialize a `Tool::ReadMcpResource` from history into the (name, args_json) form
/// used in OpenAI tool_calls.
pub fn serialize_outgoing_read_resource(
    tc: &api::message::tool_call::ReadMcpResource,
    ctx: Option<&MCPContext>,
) -> (String, String) {
    let server_name = ctx
        .and_then(|c| c.servers.iter().find(|s| s.id == tc.server_id))
        .map(|s| s.name.clone());
    let mut args = json!({ "uri": tc.uri });
    if let Some(name) = server_name {
        args["server"] = json!(name);
    }
    (READ_RESOURCE_NAME.to_owned(), args.to_string())
}

/// Serialize a `Tool::CallMcpTool` from history into the (name, args_json) pair
/// used in OpenAI tool_calls.
pub fn serialize_outgoing_call(
    tc: &api::message::tool_call::CallMcpTool,
    ctx: Option<&MCPContext>,
) -> (String, String) {
    // Look up the corresponding server.name (fall back to server_id if mcp_context has changed)
    let server_name = ctx
        .and_then(|c| c.servers.iter().find(|s| s.id == tc.server_id))
        .map(|s| sanitize_server_name(&s.name))
        .unwrap_or_else(|| tc.server_id.clone());
    let name = format!("{PREFIX}{server_name}{SEP}{}", tc.name);
    // args (Option<prost_types::Struct>) → serde_json
    let args_value = tc
        .args
        .as_ref()
        .map(|s| Value::Object(prost_struct_to_json(s)))
        .unwrap_or_else(|| json!({}));
    (name, args_value.to_string())
}

fn prost_struct_to_json(s: &prost_types::Struct) -> Map<String, Value> {
    let mut out = Map::new();
    for (k, v) in &s.fields {
        out.insert(k.clone(), prost_value_to_json(v));
    }
    out
}

fn prost_value_to_json(v: &prost_types::Value) -> Value {
    match &v.kind {
        Some(ProstKind::NullValue(_)) | None => Value::Null,
        Some(ProstKind::BoolValue(b)) => Value::Bool(*b),
        Some(ProstKind::NumberValue(n)) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Some(ProstKind::StringValue(s)) => Value::String(s.clone()),
        Some(ProstKind::ListValue(l)) => {
            Value::Array(l.values.iter().map(prost_value_to_json).collect())
        }
        Some(ProstKind::StructValue(o)) => Value::Object(prost_struct_to_json(o)),
    }
}

/// Serialize the result of CallMcpTool or ReadMcpResource within ToolCallResult for
/// the upstream model.
pub fn serialize_result(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::call_mcp_tool_result::Result as McpR;
    use api::message::tool_call_result::Result as R;
    use api::read_mcp_resource_result::Result as ReadR;

    if let R::CallMcpTool(r) = result {
        let value = match &r.result {
            Some(McpR::Success(s)) => json!({
                "status": "ok",
                // s.content is a Vec<rmcp Content>; simplified here to a debug string.
                "content": format!("{:?}", s),
            }),
            Some(McpR::Error(e)) => json!({ "status": "error", "message": e.message }),
            None => json!({ "status": "cancelled" }),
        };
        return Some(value);
    }
    if let R::ReadMcpResource(r) = result {
        let value = match &r.result {
            Some(ReadR::Success(s)) => json!({
                "status": "ok",
                // contents is Vec<rmcp ResourceContents>; debug serialization preserves all info
                "contents": format!("{:?}", s.contents),
            }),
            Some(ReadR::Error(e)) => json!({ "status": "error", "message": e.message }),
            None => json!({ "status": "cancelled" }),
        };
        return Some(value);
    }
    None
}

#[cfg(test)]
#[path = "mcp_tests.rs"]
mod tests;
