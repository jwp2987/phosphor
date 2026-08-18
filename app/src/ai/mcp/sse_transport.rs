//! Legacy SSE client transport for MCP.
//!
//! Upstream `rmcp` removed the SSE client transport in v0.11.0
//! (`SseClientTransport`, `SseClientConfig`, the `SseClient` trait and the
//! `transport-sse-client*` features are absent from every published 1.x, 2.x
//! and 3.x release). Only the SSE *parsing* primitives survive, behind the
//! still-present `client-side-sse` feature, where they serve the
//! streamable-HTTP client.
//!
//! Phosphor still needs the transport: [`super::templatable_manager::native`]
//! preflights Streamable HTTP and falls back to legacy SSE when that returns a
//! 404, so dropping it would silently break every remote MCP server that only
//! speaks the older protocol.
//!
//! Ported verbatim from the pinned oracle (Warp `42effe840`,
//! `crates/mcp/src/sse_transport/`), which vendored it for the same reason when
//! it moved to the published crate. The four submodules are byte-for-byte
//! identical to the oracle so that future re-pins diff cleanly; only this module
//! root differs, to follow this tree's `foo.rs` + `foo/` layout rather than the
//! oracle's `mod.rs`.

mod auth_impl;
mod client_side_sse;
mod reqwest_impl;
mod sse_client;

#[allow(unused_imports)]
pub use client_side_sse::{ExponentialBackoff, FixedInterval, NeverRetry, SseRetryPolicy};
#[allow(unused_imports)]
pub use sse_client::{SseClient, SseClientConfig, SseClientTransport, SseTransportError};
