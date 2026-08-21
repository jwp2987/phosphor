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
//! - server_name_safe = an **injective** encoding of server.name into
//!   `[A-Za-z0-9-]` plus single (never doubled, never leading/trailing) `_`
//!   joiners — see `sanitize_server_name`. Injective is the load-bearing word:
//!   this string is a *routing key*, so two servers that share one are two
//!   servers whose tool calls cannot be told apart.
//!
//! ## Reverse resolution
//!
//! When a `mcp__`-prefixed name is seen:
//! 1. Split out `server_name_safe` and `tool_name`
//! 2. Match against `params.mcp_context.servers` by routing key (`server_keys`),
//!    to get server.id — **more than one match is an error, never a pick**
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

/// Longest stem kept in the hashed branch of `sanitize_server_name`. The fingerprint carries
/// the identity, so truncating the stem costs readability and nothing else; the cap exists
/// because the stem plus `_` plus 8 hex digits plus the tool name all share one function-name
/// length budget.
const MAX_STEM: usize = 32;
/// Stem used when a name contributes no ASCII alphanumerics at all (`"文件服务器"`, `"!!!"`).
/// Such names always take the hashed branch, so they read as `srv_<fingerprint>` — which a
/// server literally named `srv` can never collide with, because that name is canonical and
/// canonical keys contain no `_`. The stem is uninformative on purpose; the tool *description*
/// still carries the real name (`[MCP/文件服务器] …`), which is what the model reads.
const EMPTY_STEM: &str = "srv";
/// Marker stem for the `serialize_outgoing_call` id fallback. See `unresolved_server_key`.
const UNRESOLVED_STEM: &str = "unresolved";

/// 64-bit FNV-1a, folded to 32 bits and rendered as 8 lowercase hex digits.
///
/// Hand-rolled rather than `DefaultHasher` on purpose: this digest is baked into function
/// names that must be **byte-identical across restarts and across toolchain upgrades** (they
/// are prompt-cache keys, and they appear in serialized conversation history). `DefaultHasher`
/// documents its algorithm as unspecified and free to change between Rust releases; FNV-1a is
/// a fixed, self-contained definition that cannot drift.
fn fingerprint(bytes: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", ((h >> 32) ^ (h & 0xffff_ffff)) as u32)
}

/// Convert server.name into a string safe to use as part of an OpenAI function name.
///
/// # Invariants
///
/// 1. The result never contains `__`, never starts or ends with `_`, and is never empty.
///    `SEP` is `__` and `parse_mcp_tool_call` splits the body at its *first* `__`, so a key
///    that could contain (or abut) a `__` puts the split in the wrong place.
/// 2. The mapping is **injective**: distinct names get distinct keys. This is the invariant
///    that matters, because the key is what tool calls are *routed* by.
/// 3. Every output matches either `[A-Za-z0-9-]+` (no `_` at all) or `<stem>_<8 hex digits>`.
///    `server_keys` and `unresolved_server_key` both lean on this; see their comments.
///
/// # Why the previous shape was worse than the bug it fixed
///
/// Mapping every non-alphanumeric to `_` one-for-one turned `"GitHub (remote)"` into
/// `GitHub__remote_`, which split back to a server `GitHub` that matched nothing: that
/// server's tools were advertised and then uncallable. Collapsing `_` runs and trimming the
/// ends restored the invariant — but *lossily*, and a lossy function used as a routing key
/// trades one broken server for two indistinguishable ones:
///
/// - `"GitHub (remote)"` and `"GitHub remote"` both collapsed to `GitHub_remote`. Both were
///   now advertised, both resolved, and `find(..).first()` sent every call for one of them to
///   the other. Silently.
/// - The filter was `is_ascii_alphanumeric`, so *every* name written in a non-Latin script
///   (`"文件服务器"`, `"Сервер"`) collapsed to `""` — and `mcp__` + `""` + `__` + tool splits
///   back to a server key of `""`, which matched the first such server. One collision class
///   for the entire non-ASCII world.
///
/// # The encoding
///
/// - A name already in `[A-Za-z0-9-]+` is used verbatim ("canonical"). This keeps the common
///   case (`server-a`, `my-server`) readable and unchanged, and identity is trivially
///   injective. Note `_` is deliberately *not* canonical, so canonical keys contain no `_`.
/// - Anything else becomes `<stem>_<fingerprint of the full original name>`. The stem is
///   cosmetic (for the model's benefit); the fingerprint is what distinguishes. Because the
///   two branches are separated by the presence of `_`, they can never collide with each
///   other.
///
/// The fingerprint is 32 bits, so the encoding is injective up to a hash collision rather than
/// absolutely. That residual is closed at the other end, not ignored: `parse_mcp_tool_call`
/// counts its matches and refuses an ambiguous one instead of picking, so a collision degrades
/// to a visible error, never to a misroute.
///
/// A tool name may still contain `__` freely: only the *first* `__` is the separator, and
/// everything after it is the tool name verbatim.
fn sanitize_server_name(name: &str) -> String {
    // Canonical: already in the safe alphabet, and non-empty. Identity, so injective.
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return name.to_owned();
    }

    // Cosmetic stem: everything outside the alphabet becomes `-`, runs collapse, ends trim.
    // `_` maps to `-` too, so the stem can never reintroduce a `__`.
    let mut stem = String::with_capacity(name.len());
    for c in name.chars() {
        let c = if c.is_ascii_alphanumeric() || c == '-' {
            c
        } else {
            '-'
        };
        if c == '-' && stem.ends_with('-') {
            continue;
        }
        stem.push(c);
    }
    let stem = stem.trim_matches('-');
    let stem = &stem[..stem.len().min(MAX_STEM)];
    let stem = stem.trim_end_matches('-');
    let stem = if stem.is_empty() { EMPTY_STEM } else { stem };

    // Exactly one `_`, with a non-empty alphanumeric run on each side: no `__`, no leading or
    // trailing `_`, never empty.
    format!("{stem}_{}", fingerprint(name.as_bytes()))
}

/// Routing keys for a whole context, one per server, positionally aligned with `servers`.
///
/// `sanitize_server_name` is injective over *names*, which leaves exactly one case it cannot
/// resolve on its own: two servers configured with the **same** name. No name-derived function
/// can separate those, so the tiebreak comes from `server.id` — which is the persisted
/// installation UUID, i.e. stable across restarts. Deriving it from position in `ctx.servers`
/// would not be: that order is explicitly documented above as drifting between requests.
///
/// The tiebreak is applied only to the servers that actually clash, so a unique name keeps its
/// plain readable key and its prompt-cache entry.
///
/// Appending `_<fingerprint>` preserves invariant 3 of `sanitize_server_name` (the result still
/// ends in `_` + 8 hex digits) and cannot create a `__`, since a key never ends in `_`.
fn server_keys(servers: &[MCPServer]) -> Vec<String> {
    let base: Vec<String> = servers
        .iter()
        .map(|s| sanitize_server_name(&s.name))
        .collect();
    let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for b in &base {
        *counts.entry(b.as_str()).or_insert(0) += 1;
    }
    base.iter()
        .zip(servers)
        .map(|(b, s)| {
            if counts.get(b.as_str()).copied().unwrap_or(0) > 1 {
                format!("{b}_{}", fingerprint(s.id.as_bytes()))
            } else {
                b.clone()
            }
        })
        .collect()
}

/// Key to emit for a `server_id` that is no longer in `mcp_context`.
///
/// Deliberately built so it can never equal a live server's key: every `server_keys` output
/// either contains no `_` or ends in `_` + 8 hex digits (invariant 3), and this ends in `_id`.
/// That is the whole point — see the comment in `serialize_outgoing_call`.
fn unresolved_server_key(server_id: &str) -> String {
    format!("{UNRESOLVED_STEM}_{}_id", fingerprint(server_id.as_bytes()))
}

/// Generate an OpenAI function name for an MCP tool.
///
/// Uses the server's own name-derived key. When several servers in one context share a name,
/// `build_mcp_tool_defs` uses the context-aware `server_keys` instead, which disambiguates
/// them; for the overwhelmingly common unique-name case the two agree exactly.
pub fn function_name(server: &MCPServer, tool_name: &str) -> String {
    function_name_for_key(&sanitize_server_name(&server.name), tool_name)
}

fn function_name_for_key(server_key: &str, tool_name: &str) -> String {
    format!("{PREFIX}{server_key}{SEP}{tool_name}")
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
    let keys = server_keys(&ctx.servers);
    for (server, server_key) in ctx.servers.iter().zip(&keys) {
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
            out.push((
                function_name_for_key(server_key, &tool.name),
                prefixed_desc,
                schema,
            ));
        }
    }
    // P0-3: sort by function_name in lexicographic order, to guarantee that the
    // same static context produces a consistent order across requests.
    //
    // What the function name actually guarantees: `sanitize_server_name` is injective over
    // names and `server_keys` additionally separates same-named servers by installation id, so
    // two *different* (server, tool) pairs produce the same name only if two 32-bit
    // fingerprints collide. The earlier claim here — "function_name is globally unique, so
    // there's no conflict" — was simply false while the sanitizer was lossy, and a duplicate
    // name is not a harmless tie for a sort key: OpenAI-compatible endpoints reject a tools
    // array containing two identically named functions with a 400, which fails the whole turn.
    //
    // So the residual is handled rather than asserted away. Sorting puts any duplicates
    // adjacent; `dedup_by` then keeps the request well-formed, and `parse_mcp_tool_call`
    // refuses the ambiguous key rather than dispatching it at a guessed server. The tools
    // behind a collision become visibly unusable — which is the correct conservative outcome,
    // and the one thing that must not happen (a call silently executing against the wrong
    // server) cannot.
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out.dedup_by(|a, b| a.0 == b.0);

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
    // Count the matches instead of taking the first one. `sanitize_server_name` is injective
    // and `server_keys` separates same-named servers, so a second match means two 32-bit
    // fingerprints collided — vanishingly rare, but the alternative to noticing it is running
    // the user's tool call against a server they did not name. Refusing is recoverable; a
    // silent misroute is not.
    let keys = server_keys(&ctx.servers);
    let mut matches = keys
        .iter()
        .zip(&ctx.servers)
        .filter(|(k, _)| k.as_str() == server_name_safe)
        .map(|(_, s)| s);
    let server = matches
        .next()
        .ok_or_else(|| anyhow!("MCP server `{server_name_safe}` not in current mcp_context"))?;
    if let Some(other) = matches.next() {
        return Err(anyhow!(
            "MCP server key `{server_name_safe}` is ambiguous: it matches both `{}` (id {}) \
             and `{}` (id {}). Refusing to guess which one the call was for; rename one of \
             the servers.",
            server.name,
            server.id,
            other.name,
            other.id
        ));
    }

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
            // Both sides go through the same encoding, so this matches the raw name the model
            // was shown. If it somehow matches two servers, fall through to the empty id
            // rather than picking one: an empty id means "server side, locate it by uri",
            // which may be imprecise, whereas an arbitrary pick is confidently wrong.
            Some(name) => {
                let key = sanitize_server_name(name);
                let mut hits = ctx
                    .servers
                    .iter()
                    .filter(|s| sanitize_server_name(&s.name) == key);
                match (hits.next(), hits.next()) {
                    (Some(s), None) => s.id.clone(),
                    _ => String::new(),
                }
            }
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
    // Look up the corresponding routing key. `server_keys` rather than `sanitize_server_name`
    // so a history entry serializes to the same key `build_mcp_tool_defs` advertised.
    let server_key = ctx
        .and_then(|c| {
            server_keys(&c.servers)
                .into_iter()
                .zip(&c.servers)
                .find(|(_, s)| s.id == tc.server_id)
                .map(|(k, _)| k)
        })
        // Fallback: the server this historical call belongs to is no longer in mcp_context.
        //
        // The previous fallback ran the raw id through the sanitizer, reasoning that an id
        // containing `__` would otherwise split in the wrong place and turn an unresolvable
        // call into a mis-resolved one. That has it backwards. Sanitizing maps the id *into*
        // the same key space live servers occupy, which makes an accidental match with a real
        // server's key more likely, not less — and resolution is by key, so an accidental
        // match is exactly the mis-resolution it was trying to avoid.
        //
        // What we actually want is a key that is well-formed (so the `__` split still lands
        // correctly and the transcript stays parseable) and provably matches nothing live.
        // `unresolved_server_key` gives both: it ends in `_id`, and every `server_keys` output
        // either contains no `_` or ends in `_` + 8 hex digits.
        .unwrap_or_else(|| unresolved_server_key(&tc.server_id));
    let name = function_name_for_key(&server_key, &tc.name);
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

/// Routing-key coverage for the `mcp__<server>__<tool>` encoding.
///
/// Lives here rather than in `mcp_tests.rs` only because that file is outside the edit set for
/// this fix.
///
/// The tests these replace were vacuous for the cases that mattered. They asserted only
/// `!contains("__")`, `!starts_with('_')` and `!ends_with('_')` — every one of which passes
/// trivially on the empty string, which is exactly what the old sanitizer returned for `"!!!"`
/// and for every non-Latin name. And the round-trip test used three servers whose sanitized
/// names were already distinct, so it could never have observed a collision. A test that
/// cannot fail for the failure mode under repair is not coverage.
///
/// Each test that names a past failure — the collapsed-together pair, the non-Latin names, the
/// all-punctuation name, injectivity, and the id fallback — fails on the lossy encoding rather
/// than passing vacuously. `keys_are_stable_under_reordering_and_across_rebuilds` is the one
/// exception, and is honest about it: it guards the *new* tiebreak, which had nothing to be
/// wrong about before.
#[cfg(test)]
mod server_key_encoding_tests {
    use super::*;
    use crate::ai::agent::{MCPContext, MCPServer};

    fn mk_server(id: &str, name: &str) -> MCPServer {
        MCPServer {
            id: id.to_owned(),
            name: name.to_owned(),
            description: String::new(),
            resources: Vec::new(),
            tools: Vec::new(),
        }
    }

    fn mk_ctx(servers: Vec<MCPServer>) -> MCPContext {
        MCPContext {
            #[allow(deprecated)]
            resources: vec![],
            #[allow(deprecated)]
            tools: vec![],
            servers,
        }
    }

    /// Build the function name this server's tool is actually advertised under (the
    /// context-aware key, same as `build_mcp_tool_defs` uses), resolve it back, and return the
    /// server id it routed to. Panics with the parse error if it does not resolve at all.
    fn route(ctx: &MCPContext, server: &MCPServer, tool: &str) -> String {
        let fname = function_name_for_key(&key_in(ctx, server), tool);
        assert!(is_mcp_function(&fname), "{fname} must be an MCP name");
        let parsed = parse_mcp_tool_call(&fname, "{}", Some(ctx))
            .unwrap_or_else(|e| panic!("{fname} should resolve: {e}"));
        let api::message::tool_call::Tool::CallMcpTool(call) = parsed else {
            panic!("{fname} should parse as a CallMcpTool");
        };
        assert_eq!(call.name, tool, "tool name for {fname}");
        call.server_id
    }

    fn key_in(ctx: &MCPContext, server: &MCPServer) -> String {
        let keys = server_keys(&ctx.servers);
        keys.into_iter()
            .zip(&ctx.servers)
            .find(|(_, s)| s.id == server.id)
            .map(|(k, _)| k)
            .expect("server must be in ctx")
    }

    /// The structural invariants the `__` split depends on — including the two the old test
    /// could not see, because the empty string satisfies all three of its assertions.
    #[test]
    fn keys_are_well_formed_and_never_empty() {
        let very_long = "x".repeat(200);
        for name in [
            "GitHub (remote)",
            "GitHub remote",
            "my  server",
            "__leading",
            "trailing__",
            "a---b",
            "!!!",
            "文件服务器",
            "Сервер",
            "server-a",
            "",
            very_long.as_str(),
        ] {
            let key = sanitize_server_name(name);
            assert!(!key.is_empty(), "{name:?} sanitized to an empty key");
            assert!(!key.contains(SEP), "{name:?} sanitized to {key:?}");
            assert!(!key.starts_with('_'), "{name:?} sanitized to {key:?}");
            assert!(!key.ends_with('_'), "{name:?} sanitized to {key:?}");
            assert!(
                key.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
                "{name:?} sanitized to {key:?}, which leaves the safe alphabet"
            );
        }
    }

    /// Invariant 3, which `unresolved_server_key` and the dup tiebreak both rely on: a key
    /// either has no `_` at all, or ends in `_` + exactly 8 lowercase hex digits.
    #[test]
    fn key_shape_is_one_of_exactly_two_forms() {
        for name in [
            "server-a",
            "GitHub (remote)",
            "!!!",
            "文件服务器",
            "a---b",
            "",
        ] {
            let key = sanitize_server_name(name);
            let ok = if let Some((stem, tail)) = key.rsplit_once('_') {
                !stem.is_empty()
                    && tail.len() == 8
                    && tail
                        .chars()
                        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
            } else {
                true
            };
            assert!(ok, "{name:?} sanitized to {key:?}, which is neither form");
        }
    }

    /// The regression this whole fix exists for. Under the lossy sanitizer both of these
    /// names became `GitHub_remote`, both were advertised, and every call meant for the
    /// second was dispatched to the first — silently.
    #[test]
    fn names_that_used_to_collapse_together_route_to_their_own_servers() {
        let bracketed = mk_server("id-bracketed", "GitHub (remote)");
        let spaced = mk_server("id-spaced", "GitHub remote");
        let underscored = mk_server("id-underscored", "GitHub_remote");
        let ctx = mk_ctx(vec![bracketed.clone(), spaced.clone(), underscored.clone()]);

        assert_eq!(route(&ctx, &bracketed, "list_issues"), "id-bracketed");
        assert_eq!(route(&ctx, &spaced, "list_issues"), "id-spaced");
        assert_eq!(route(&ctx, &underscored, "list_issues"), "id-underscored");
    }

    /// The filter was `is_ascii_alphanumeric`, so every non-Latin name mapped to `""` and all
    /// of them collided with each other and with any all-punctuation name. Each must now be
    /// independently routable.
    #[test]
    fn non_latin_and_punctuation_only_names_are_routable_and_distinct() {
        let chinese = mk_server("id-cn", "文件服务器");
        let cyrillic = mk_server("id-ru", "Сервер");
        let emoji = mk_server("id-emoji", "🚀");
        let punct = mk_server("id-punct", "!!!");
        let ctx = mk_ctx(vec![
            chinese.clone(),
            cyrillic.clone(),
            emoji.clone(),
            punct.clone(),
        ]);

        for server in [&chinese, &cyrillic, &emoji, &punct] {
            let key = key_in(&ctx, server);
            assert!(
                !key.is_empty(),
                "{:?} must not sanitize to an empty key",
                server.name
            );
            assert_eq!(
                route(&ctx, server, "read_file"),
                server.id,
                "{:?} (key {key:?}) routed to the wrong server",
                server.name
            );
        }
    }

    /// Injectivity, stated directly: no two distinct names may share a key.
    #[test]
    fn distinct_names_get_distinct_keys() {
        let names = [
            "GitHub (remote)",
            "GitHub remote",
            "GitHub_remote",
            "GitHub-remote",
            "文件服务器",
            "Сервер",
            "!!!",
            "???",
            "",
            " ",
            "srv",
            "server-a",
            "server a",
            "server--a",
            "my  server",
            "my server",
        ];
        let mut seen: std::collections::HashMap<String, &str> = std::collections::HashMap::new();
        for name in names {
            let key = sanitize_server_name(name);
            if let Some(prev) = seen.insert(key.clone(), name) {
                panic!("{name:?} and {prev:?} both sanitize to {key:?}");
            }
        }
    }

    /// The one case no name-derived encoding can separate: two servers configured with the
    /// same name. `server_keys` breaks the tie on the persisted installation id, so both stay
    /// usable and each routes to itself.
    #[test]
    fn identically_named_servers_are_separated_by_id() {
        let a = mk_server("id-aaa", "GitHub (remote)");
        let b = mk_server("id-bbb", "GitHub (remote)");
        let ctx = mk_ctx(vec![a.clone(), b.clone()]);

        let keys = server_keys(&ctx.servers);
        assert_ne!(keys[0], keys[1], "same-named servers must not share a key");
        assert_eq!(route(&ctx, &a, "list_issues"), "id-aaa");
        assert_eq!(route(&ctx, &b, "list_issues"), "id-bbb");
    }

    /// The tiebreak comes from the id, not from position, so shuffling `ctx.servers` (which
    /// the module doc says can happen between requests) does not reassign keys. If it did,
    /// every restart would rewrite the function names and invalidate the prompt cache — and
    /// historical tool calls would name servers that no longer answer to those keys.
    #[test]
    fn keys_are_stable_under_reordering_and_across_rebuilds() {
        let a = mk_server("id-aaa", "dup name");
        let b = mk_server("id-bbb", "dup name");
        let c = mk_server("id-ccc", "solo");
        let forward = mk_ctx(vec![a.clone(), b.clone(), c.clone()]);
        let reversed = mk_ctx(vec![c.clone(), b.clone(), a.clone()]);

        for s in [&a, &b, &c] {
            assert_eq!(
                key_in(&forward, s),
                key_in(&reversed, s),
                "key for {:?} (id {}) depends on position",
                s.name,
                s.id
            );
        }
        // And recomputing from scratch is byte-identical — the digest is a fixed definition,
        // not a per-process hasher.
        assert_eq!(server_keys(&forward.servers), server_keys(&forward.servers));
    }

    /// A full round trip through the public surface, for every awkward name shape crossed
    /// with tool names that themselves contain `__` or lead with `_`.
    #[test]
    fn function_names_round_trip_back_to_their_server_and_tool() {
        let servers = vec![
            mk_server("id-bracketed", "GitHub (remote)"),
            mk_server("id-spaced", "GitHub remote"),
            mk_server("id-plain", "server-a"),
            mk_server("id-underscored", "weird__name_"),
            mk_server("id-cn", "文件服务器"),
            mk_server("id-punct", "!!!"),
        ];
        let ctx = mk_ctx(servers.clone());

        for server in &servers {
            for tool_name in ["list_issues", "odd__tool", "_leading_underscore", "t"] {
                assert_eq!(
                    route(&ctx, server, tool_name),
                    server.id,
                    "{:?} / {tool_name} routed wrong",
                    server.name
                );
            }
        }
    }

    /// `build_mcp_tool_defs` must never advertise a name that resolution then refuses, and
    /// must never emit the same name twice (OpenAI-compatible endpoints 400 on duplicate tool
    /// names, which fails the entire turn).
    #[test]
    fn advertised_names_are_unique_and_all_resolve() {
        let mut servers = vec![
            mk_server("id-bracketed", "GitHub (remote)"),
            mk_server("id-spaced", "GitHub remote"),
            mk_server("id-cn", "文件服务器"),
            mk_server("id-dup-a", "dup"),
            mk_server("id-dup-b", "dup"),
        ];
        for s in &mut servers {
            s.tools = vec![rmcp::model::Tool::new(
                "echo",
                "echo it back",
                std::sync::Arc::new(serde_json::Map::new()),
            )];
        }
        let ctx = mk_ctx(servers.clone());

        let defs = build_mcp_tool_defs(&ctx);
        assert_eq!(defs.len(), servers.len(), "one tool advertised per server");

        let mut names: Vec<&str> = defs.iter().map(|(n, _, _)| n.as_str()).collect();
        names.sort_unstable();
        let unique = {
            let mut v = names.clone();
            v.dedup();
            v
        };
        assert_eq!(names, unique, "duplicate function name in the tools array");

        let mut routed: Vec<String> = defs
            .iter()
            .map(|(n, _, _)| match parse_mcp_tool_call(n, "{}", Some(&ctx)) {
                Ok(api::message::tool_call::Tool::CallMcpTool(c)) => c.server_id,
                Ok(_) => panic!("{n} parsed as the wrong tool kind"),
                Err(e) => panic!("advertised {n} but it does not resolve: {e}"),
            })
            .collect();
        routed.sort();
        let mut expected: Vec<String> = servers.iter().map(|s| s.id.clone()).collect();
        expected.sort();
        assert_eq!(routed, expected, "each server must be reached exactly once");
    }

    /// The id fallback for a server that has dropped out of `mcp_context`. Sanitizing the raw
    /// id (the previous behaviour) folded it into the live key space; the point of the
    /// fallback is that it cannot land on a live server.
    #[test]
    fn unresolved_id_fallback_never_matches_a_live_server() {
        let live = mk_server("id-live", "GitHub (remote)");
        let ctx = mk_ctx(vec![live.clone()]);

        // A departed server whose id is, adversarially, exactly the live server's key — the
        // shape the old `sanitize_server_name(&tc.server_id)` fallback would hand straight to
        // the live server.
        let tc = api::message::tool_call::CallMcpTool {
            name: "list_issues".to_owned(),
            args: None,
            server_id: sanitize_server_name(&live.name),
        };
        let (name, _) = serialize_outgoing_call(&tc, Some(&ctx));

        assert!(is_mcp_function(&name), "{name} must stay well-formed");
        let body = name.strip_prefix(PREFIX).expect("prefix");
        let (key, tool) = body.split_once(SEP).expect("the split must still land");
        assert_eq!(tool, "list_issues", "the tool name must survive intact");
        assert!(
            !server_keys(&ctx.servers).contains(&key.to_owned()),
            "fallback key {key:?} collides with a live server"
        );
        assert!(
            parse_mcp_tool_call(&name, "{}", Some(&ctx)).is_err(),
            "a call to a departed server must not resolve to anything"
        );
        // Deterministic: same departed id, same emitted name, restart after restart.
        assert_eq!(serialize_outgoing_call(&tc, Some(&ctx)).0, name);
    }

    /// A server still present in `mcp_context` serializes to the key it was advertised under,
    /// so history replay and the live tools array agree byte for byte.
    #[test]
    fn resolved_history_calls_reuse_the_advertised_key() {
        let a = mk_server("id-aaa", "dup name");
        let b = mk_server("id-bbb", "dup name");
        let ctx = mk_ctx(vec![a.clone(), b.clone()]);

        for server in [&a, &b] {
            let tc = api::message::tool_call::CallMcpTool {
                name: "echo".to_owned(),
                args: None,
                server_id: server.id.clone(),
            };
            let (name, _) = serialize_outgoing_call(&tc, Some(&ctx));
            assert_eq!(
                name,
                function_name_for_key(&key_in(&ctx, server), "echo"),
                "history name for {:?} diverges from the advertised one",
                server.id
            );
            let Ok(api::message::tool_call::Tool::CallMcpTool(call)) =
                parse_mcp_tool_call(&name, "{}", Some(&ctx))
            else {
                panic!("{name} should resolve back to a CallMcpTool");
            };
            assert_eq!(call.server_id, server.id);
        }
    }
}
