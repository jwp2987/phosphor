//! Scenario manifest — the single source of truth for the usage suite.
//!
//! See `specs/usage-test-suite/SCOPE.md` §6. The runner (`main.rs`) drives the
//! rows below across both surfaces:
//! * GUI `usage_*` scenarios live in `crates/integration/src/test/usage.rs` and
//!   are dispatched through the `integration` binary.
//! * TUI `usage_tui_*` view-harness tests live in
//!   `crates/warp_tui/src/usage_smoke_tests.rs` and are selected via a nextest
//!   `test(/(^|::)usage_tui_/)` filter.
//!
//! This file is the single place tags are declared; keep each scenario's tag
//! here in sync with the doc comment on its test function.

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
    /// Wants a real window / pixel result. Skipped only when the host has no
    /// desktop session — see `has_desktop_session()`. This was formerly an
    /// unconditional skip, which became wrong once the suite ran on macOS and
    /// Windows runners (and it was always wrong on a maintainer workstation).
    NeedsDesktop,
    /// Needs a real BYOP provider (key + network). Only runs with
    /// `--include-byop`.
    NeedsByopProvider,
}

impl Tag {
    pub fn as_str(self) -> &'static str {
        match self {
            Tag::ReliableHere => "reliable-here",
            Tag::NeedsRealShell => "needs-real-shell",
            Tag::NeedsDesktop => "needs-desktop",
            Tag::NeedsByopProvider => "needs-byop-provider",
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
    /// nextest filter `test(/(^|::)usage_tui_/)` selects them.
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
// Real `usage_*` rows registered in `crates/integration/src/bin/integration.rs`
// and defined in `crates/integration/src/test/usage.rs` — see SCOPE.md §4.1.
// Add a row here when a new `usage_*` GUI scenario lands.
// ---------------------------------------------------------------------------
pub const GUI_SCENARIOS: &[Scenario] = &[
    // In-process, no real shell / provider / GPU — trustworthy in the sandbox
    // and run by default.
    Scenario {
        surface: Surface::Gui,
        name: "usage_launch_bootstrap",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_open_close_settings",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_open_command_palette",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_tabs_add_switch_close",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_theme_creator_modal",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_agent_block_render",
        tags: &[Tag::ReliableHere],
    },
    // Drives a real PTY shell to command completion; subject to the
    // bash-preexec race in this sandbox. Only runs with `--include-flaky`.
    // Block selection and find-in-block both operate over the real block list,
    // so they need a genuine command block (an injected AI block is not a
    // selectable/searchable participant) — hence the real-shell tag.
    Scenario {
        surface: Surface::Gui,
        name: "usage_block_navigation_select",
        tags: &[Tag::NeedsRealShell],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_find_in_block",
        tags: &[Tag::NeedsRealShell],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_run_command_output_block",
        tags: &[Tag::NeedsRealShell],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_run_command_exit_code",
        tags: &[Tag::NeedsRealShell],
    },
    Scenario {
        surface: Surface::Gui,
        name: "usage_secret_redaction",
        tags: &[Tag::NeedsRealShell],
    },
    // Genuine agent round-trip needs a real BYOP provider (key + network);
    // only runs with `--include-byop`. The Chunk-D provider mock
    // (`app/src/integration_testing/mock_provider`) exists and is self-tested;
    // wiring it into this scenario for a true no-key round-trip is a follow-up.
    Scenario {
        surface: Surface::Gui,
        name: "usage_agent_roundtrip",
        tags: &[Tag::NeedsByopProvider],
    },
    // Wants a real GPU window / pixel geometry; always skipped by this runner.
    Scenario {
        surface: Surface::Gui,
        name: "usage_font_size_window_resize",
        tags: &[Tag::NeedsDesktop],
    },
];

// ---------------------------------------------------------------------------
// TUI scenarios (`warp_tui` `usage_tui_*` tests region)
//
// Real rows live in `crates/warp_tui/src/usage_smoke_tests.rs` — see SCOPE.md
// §4.2. Every name MUST start with `usage_tui_` so the runner's nextest filter
// `test(/(^|::)usage_tui_/)` selects it. Add a row here when a new one lands.
// ---------------------------------------------------------------------------
pub const TUI_SCENARIOS: &[Scenario] = &[
    // In-process `warp_tui` view-harness renders — no shell/provider/GPU, so
    // all are reliable-here and run by default. Live in
    // `crates/warp_tui/src/usage_smoke_tests.rs`.
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_zero_state_render",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_transcript_render",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_permission_prompt",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_completions_menu",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_conversation_menu",
        tags: &[Tag::ReliableHere],
    },
    Scenario {
        surface: Surface::Tui,
        name: "usage_tui_slash_command_palette",
        tags: &[Tag::ReliableHere],
    },
];

/// All scenarios across both surfaces, in manifest order.
pub fn all_scenarios() -> Vec<&'static Scenario> {
    GUI_SCENARIOS.iter().chain(TUI_SCENARIOS.iter()).collect()
}
