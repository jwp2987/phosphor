//! Telemetry event types for the LSP subsystem.
//!
//! **Data types only — the pin's registration machinery is deliberately absent.**
//! Zap's telemetry *sending* has been physically removed: the
//! `send_telemetry_from_ctx!` family in `crates/warp_core/src/telemetry.rs` are
//! untyped compatibility shims that reference `$event` inside an `if false`
//! branch, with no trait bound on `$event` at all. Consequently the traits the
//! pin's version implements — `warp_core::telemetry::{TelemetryEvent,
//! TelemetryEventDesc, EnablementState}`, `warp_core::telemetry::enum_events`
//! and the `warp_core::register_telemetry_event!` macro — **do not exist
//! anywhere in this fork**. The pin's `impl TelemetryEvent for
//! LspTelemetryEvent`, `impl TelemetryEventDesc for
//! LspTelemetryEventDiscriminants` and the trailing
//! `register_telemetry_event!(LspTelemetryEvent)` are therefore dropped here,
//! along with the `EnumDiscriminants`/`EnumIter` derives that existed only to
//! feed them.
//!
//! This mirrors what `app/src/ai/blocklist/telemetry.rs` already does for the
//! orchestration pill bar, and matches `DECLINED.md`'s "Telemetry and crash
//! reporting" decision. Call sites are unchanged: they still construct these
//! variants and hand them to `send_telemetry_from_ctx!`, so restoring a
//! telemetry transport later only needs the trait impls put back — the event
//! catalog, its field names and its `serde` renames are preserved verbatim from
//! the pin so the wire payloads would be identical.

use serde::{Deserialize, Serialize};

/// The source from which the user enabled an LSP server.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub enum LspEnablementSource {
    #[serde(rename = "init_flow")]
    InitFlow,
    #[serde(rename = "footer_button")]
    FooterButton,
    #[serde(rename = "settings")]
    Settings,
}

/// The control action the user performed on an LSP server.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub enum LspControlActionType {
    #[serde(rename = "open_logs")]
    OpenLogs,
    #[serde(rename = "restart")]
    Restart,
    #[serde(rename = "stop")]
    Stop,
    #[serde(rename = "start")]
    Start,
    #[serde(rename = "restart_all")]
    RestartAll,
    #[serde(rename = "stop_all")]
    StopAll,
}

/// The LSP telemetry event catalog, carried over verbatim from the pin.
///
/// At the pin each variant maps to an event name (`Lsp.ServerEnabled`,
/// `Lsp.HoverShown`, …) and a JSON payload through the `TelemetryEvent` /
/// `TelemetryEventDesc` impls. Those impls are not portable here — see the
/// module docs — so the variants exist purely as the shape the call sites pass
/// to `send_telemetry_from_ctx!`.
#[derive(Debug)]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
#[allow(dead_code)]
pub enum LspTelemetryEvent {
    /// User enabled an LSP server for a workspace.
    ServerEnabled {
        server_type: String,
        source: LspEnablementSource,
        needed_install: bool,
    },
    /// User skipped LSP enablement during /init.
    ServerEnablementSkipped,
    /// An LSP server installation finished (success or failure).
    ServerInstallCompleted { server_type: String, success: bool },
    /// User removed an LSP server.
    ServerRemoved {
        server_type: String,
        source: LspEnablementSource,
    },
    /// Hover tooltip displayed with content.
    HoverShown {
        server_type: String,
        had_content: bool,
        had_diagnostics: bool,
    },
    /// User triggered goto definition.
    GotoDefinition {
        server_type: String,
        had_result: bool,
    },
    /// Find references card displayed.
    FindReferencesShown {
        server_type: String,
        num_references: usize,
    },
    /// User performed an LSP control action from the footer menu.
    ControlAction {
        action: LspControlActionType,
        server_type: Option<String>,
    },
    /// Server successfully started and is available.
    ServerStarted { server_type: String },
    /// Server failed to start.
    ServerFailed { server_type: String },
}
