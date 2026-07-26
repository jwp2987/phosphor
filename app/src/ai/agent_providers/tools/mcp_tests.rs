//! Unit tests for `mcp.rs`.
//!
//! Covers the P0-3 prompt cache optimization: `build_mcp_tool_defs` must be
//! **stable in lexicographic order** — calling it multiple times with the same
//! `MCPContext` across requests must produce a byte-equal tools list, otherwise
//! Anthropic will judge the tools field to have changed and invalidate every
//! cache layer.
//!
//! Note: `rmcp::model::Tool` and `rmcp::model::Resource` (=
//! `Annotated<RawResource>`) come from an upstream vendor crate; here we only use
//! their public construction paths (`Tool::new` / `RawResource::new`).

use rmcp::model::{AnnotateAble, RawResource, Tool};
use serde_json::json;
use std::sync::Arc;

use crate::ai::agent::{MCPContext, MCPServer};

use super::{build_mcp_tool_defs, function_name};

/// Construct an `rmcp::model::Tool` with a minimal input schema.
fn mk_tool(name: &'static str, desc: &'static str) -> Tool {
    let schema: serde_json::Map<String, serde_json::Value> = json!({
        "type": "object",
        "properties": {
            "x": { "type": "string" }
        }
    })
    .as_object()
    .unwrap()
    .clone();
    // `Tool::new` accepts Arc<JsonObject>; here we pass the Map directly (it implements Into<Arc<JsonObject>>).
    Tool::new(name, desc, Arc::new(schema))
}

/// Construct an MCPServer. The tools order and resources order are kept exactly
/// as passed in (simulating the shuffled input that upstream might supply under
/// HashMap iteration order).
fn mk_server(
    id: &str,
    name: &str,
    tools: Vec<Tool>,
    resources: Vec<rmcp::model::Resource>,
) -> MCPServer {
    MCPServer {
        id: id.to_owned(),
        name: name.to_owned(),
        description: String::new(),
        resources,
        tools,
    }
}

fn mk_resource(uri: &str, name: &str) -> rmcp::model::Resource {
    // RawResource → Annotated<RawResource> (with no annotation).
    // The safe conversion entry point upstream provides is `AnnotateAble::no_annotation`.
    RawResource::new(uri, name).no_annotation()
}

/// Building twice from the same ctx must produce byte-equal (name, description,
/// schema) triples. This is the bare minimum for prompt cache hits — any
/// instability and Anthropic's cache is entirely invalidated.
#[test]
fn build_mcp_tool_defs_is_stable_across_calls() {
    let ctx = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![
            mk_server(
                "id-b",
                "server-b",
                vec![mk_tool("zeta", "z"), mk_tool("alpha", "a")],
                vec![],
            ),
            mk_server(
                "id-a",
                "server-a",
                vec![mk_tool("beta", "b"), mk_tool("gamma", "g")],
                vec![],
            ),
        ],
    };
    let r1 = build_mcp_tool_defs(&ctx);
    let r2 = build_mcp_tool_defs(&ctx);
    assert_eq!(r1, r2, "build_mcp_tool_defs must produce deterministic output");
}

/// When input servers / tools are out of order, the output is sorted by
/// function_name lexicographic order.
/// This is P0-3's core assertion: if upstream ctx.servers order differs across
/// requests (e.g. due to HashMap iteration), the output is still byte-equal.
#[test]
fn build_mcp_tool_defs_outputs_lexicographic_order() {
    let ctx = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![
            mk_server(
                "id-b",
                "server-b",
                // Out of order: zeta before alpha
                vec![mk_tool("zeta", "z"), mk_tool("alpha", "a")],
                vec![],
            ),
            mk_server(
                "id-a",
                "server-a",
                vec![mk_tool("beta", "b"), mk_tool("gamma", "g")],
                vec![],
            ),
        ],
    };
    let out = build_mcp_tool_defs(&ctx);
    let names: Vec<&str> = out.iter().map(|(n, _, _)| n.as_str()).collect();
    // After sorting by function_name: server-a/beta < server-a/gamma < server-b/alpha < server-b/zeta
    let expected = [
        function_name(&mk_server("id-a", "server-a", vec![], vec![]), "beta"),
        function_name(&mk_server("id-a", "server-a", vec![], vec![]), "gamma"),
        function_name(&mk_server("id-b", "server-b", vec![], vec![]), "alpha"),
        function_name(&mk_server("id-b", "server-b", vec![], vec![]), "zeta"),
    ];
    assert_eq!(
        names,
        expected.iter().map(|s| s.as_str()).collect::<Vec<_>>()
    );
}

/// Even when the input servers order differs across requests (simulating a
/// HashMap reshuffle), the output is still byte-equal.
#[test]
fn build_mcp_tool_defs_invariant_under_servers_permutation() {
    let server_a = mk_server(
        "id-a",
        "server-a",
        vec![mk_tool("beta", "b"), mk_tool("gamma", "g")],
        vec![],
    );
    let server_b = mk_server(
        "id-b",
        "server-b",
        vec![mk_tool("zeta", "z"), mk_tool("alpha", "a")],
        vec![],
    );
    let ctx1 = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![server_a.clone(), server_b.clone()],
    };
    let ctx2 = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![server_b, server_a],
    };
    assert_eq!(build_mcp_tool_defs(&ctx1), build_mcp_tool_defs(&ctx2));
}

/// When any server exposes resources, the available_uris in the read_resource
/// description must also be stable in lexicographic order, and read_resource is
/// always last in the array.
#[test]
fn read_resource_description_is_stable_and_sorted() {
    let ctx1 = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![mk_server(
            "id-a",
            "srv",
            vec![mk_tool("t", "")],
            vec![
                mk_resource("file:///z.txt", "Z"),
                mk_resource("file:///a.txt", "A"),
            ],
        )],
    };
    // Same ctx but with resources order swapped
    let ctx2 = MCPContext {
        #[allow(deprecated)]
        resources: vec![],
        #[allow(deprecated)]
        tools: vec![],
        servers: vec![mk_server(
            "id-a",
            "srv",
            vec![mk_tool("t", "")],
            vec![
                mk_resource("file:///a.txt", "A"),
                mk_resource("file:///z.txt", "Z"),
            ],
        )],
    };
    let r1 = build_mcp_tool_defs(&ctx1);
    let r2 = build_mcp_tool_defs(&ctx2);
    assert_eq!(r1, r2, "read_resource description must be byte-equal");

    let last = r1.last().expect("should contain at least read_resource");
    assert_eq!(last.0, "mcp_read_resource");
    // After sorting, a.txt comes before z.txt
    let pos_a = last.1.find("a.txt").expect("should contain a.txt");
    let pos_z = last.1.find("z.txt").expect("should contain z.txt");
    assert!(pos_a < pos_z, "available_uris must be sorted lexicographically");
}
