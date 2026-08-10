//! Event types for the codebase-embedding index.
//!
//! Restored from the pin (`02b53fcd8:crates/ai/src/telemetry.rs`) along with the
//! index itself, but **only as far as the type**. This fork has physically
//! removed telemetry transmission: `warp_core::telemetry` is a set of macro
//! shims that type-check the event expression in an `if false` branch and send
//! nothing, and neither the `TelemetryEvent` trait nor
//! `register_telemetry_event!` exists here. So the pin's `impl TelemetryEvent`,
//! `impl TelemetryEventDesc` and `register_telemetry_event!` blocks — the parts
//! that name events for the wire, describe them for the changelog and gate them
//! on an `EnablementState` — are deliberately absent.
//!
//! The enum itself is kept verbatim so the index's `send_telemetry_from_ctx!`
//! call sites are the pin's, unchanged, and so a future maintainer restoring a
//! local metrics sink has the event vocabulary already in place.

use std::time::Duration;

use serde::Serialize;

#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
#[derive(Clone)]
pub enum AITelemetryEvent {
    MerkleTreeSnapshotRebuildSuccess {
        duration: Duration,
    },
    MerkleTreeSnapshotRebuildFailed {
        error: String,
    },
    MerkleTreeSnapshotDiffSuccess {
        duration: Duration,
    },
    MerkleTreeSnapshotDiffFailed {
        error: String,
    },
    SyncCodebaseContextSuccess {
        total_sync_duration: Duration,
        flushed_node_count: usize,
        flushed_fragment_count: usize,
        total_fragment_size_bytes: usize,
        sync_type: CodebaseContextSyncType,
        cache_population_error: Option<String>,
    },
    SyncCodebaseContextFailed {
        error: String,
        sync_type: CodebaseContextSyncType,
    },
    BuildTreeFailed {
        error: String,
    },
    BuildTreeSuccess {
        file_traversal_duration: Duration,
        merkle_tree_parse_duration: Duration,
    },
}

#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
#[derive(Clone, Serialize)]
pub enum CodebaseContextSyncType {
    Full,
    Initial,
    Incremental,
}
