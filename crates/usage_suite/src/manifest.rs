//! Scenario manifest — the single source of truth for the usage suite.
//!
//! This is Chunk A's file (`specs/usage-test-suite/SCOPE.md` §6). It ships a
//! small **stub manifest** so the runner (`main.rs`) works end-to-end before
//! Chunk B (GUI `usage_*` scenarios in `crates/integration/src/test/usage.rs`)
//! and Chunk C (TUI `usage_tui_*` tests in `crates/warp_tui/src/usage_tests.rs`)
//! land real coverage.
//!
//! Chunks B and C append rows **only** inside the clearly-marked
//! `GUI_SCENARIOS` / `TUI_SCENARIOS` regions below — no other part of this
//! file should change as part of those chunks.

use serde::Serialize;

/// Which surface (GUI app or TUI view harness) a scenario exercises.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Surface {
    Gui,
    Tui,
}

impl Surface {
    pub fn as_str(self) -> &'static str {
        match self {
            Surface::Gui => "gui",
            Surface::Tui => "tui",
        }
    }
}

/// Tags used to decide default-skip behavior; see
/// `specs/usage-test-suite/SCOPE.md` §4 for the full rationale of each.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tag {
    /// In-process, no shell/provider — trustworthy in this sandbox. Runs by
    /// default.
    ReliableHere,
    /// Drives a real PTY shell to command completion; subject to the
    /// shell-preexec race. Only runs with `--include-flaky`.
    NeedsRealShell,
    /// Wants a real GPU window / pixel result. Always skipped by this
    /// runner (documented-but-skipped, per SCOPE §4.1).
    NeedsDesktop,
    /// Needs a real BYOP provider (key + network). Only runs with
    /// `--include-byop`.
    NeedsByopProvider,
    /// Chunk-A placeholder scenario that exercises the runner's plumbing
    /// without asserting real app behavior. Removed once B/C replace it
    /// with real coverage for that surface.
    Stub,
}

impl Tag {
    pub fn as_str(self) -> &'static str {
        match self {
            Tag::ReliableHere => "reliable-here",
            Tag::NeedsRealShell => "needs-real-shell",
            Tag::NeedsDesktop => "needs-desktop",
            Tag::NeedsByopProvider => "needs-byop-provider",
            Tag::Stub => "stub",
        }
    }
}

/// One row of the manifest: a named, taggable usage scenario for one surface.
#[derive(Debug, Clone)]
pub struct Scenario {
    pub surface: Surface,
    /// For GUI: the `integration` binary scenario name, i.e. a key
    /// registered in `register_tests()`
    /// (`crates/integration/src/bin/integration.rs`).
    ///
    /// For TUI: the `#[test]` function name in `warp_tui`. Real (non-stub)
    /// TUI scenario names MUST start with `usage_tui_` so the runner's
    /// nextest filter `test(/^usage_tui_/)` selects them.
    pub name: &'static str,
    pub tags: &'static [Tag],
}

impl Scenario {
    pub fn has_tag(&self, tag: Tag) -> bool {
        self.tags.contains(&tag)
    }

    pub fn tag_strs(&self) -> Vec<&'static str> {
        self.tags.iter().map(|t| t.as_str()).collect()
    }
}

// ---------------------------------------------------------------------------
// GUI scenarios (`integration` binary region)
//
// Chunk B appends real `usage_*` rows here as they land in
// `crates/integration/src/test/usage.rs` — see SCOPE.md §4.1 for the target
// catalog. Keep new rows inside this array literal; do not touch code above.
// ---------------------------------------------------------------------------
pub const GUI_SCENARIOS: &[Scenario] = &[
    // Chunk-A stub: reuses an existing, already-registered `integration`
    // scenario (not a `usage_*` name) purely so the runner's GUI path is
    // exercised end-to-end before Chunk B lands the real `usage_*` GUI
    // scenarios. Replace/supplement with real entries in Chunk B.
    Scenario {
        surface: Surface::Gui,
        name: "test_open_and_close_settings",
        tags: &[Tag::ReliableHere, Tag::Stub],
    },
];

// ---------------------------------------------------------------------------
// TUI scenarios (`warp_tui` `usage_tui_*` tests region)
//
// Chunk C appends real rows here as they land in
// `crates/warp_tui/src/usage_tests.rs` — see SCOPE.md §4.2 for the target
// catalog. Every real (non-stub) name MUST start with `usage_tui_`. Keep new
// rows inside this array literal; do not touch code above.
// ---------------------------------------------------------------------------
pub const TUI_SCENARIOS: &[Scenario] = &[
    // Chunk-A stub: placeholder name. No `usage_tui_*` test exists in
    // `warp_tui` yet (Chunk C adds them), so the runner's nextest filter
    // matches zero tests today; the runner reports this row as `skip` with
    // a clear "not landed yet" reason rather than failing the suite.
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_stub_placeholder",
        tags: &[Tag::Stub],
    },
];

/// All scenarios across both surfaces, in manifest order.
pub fn all_scenarios() -> Vec<&'static Scenario> {
    GUI_SCENARIOS.iter().chain(TUI_SCENARIOS.iter()).collect()
}
