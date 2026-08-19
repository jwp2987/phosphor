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
    /// Drives a real PTY shell to command completion. Runs by DEFAULT since
    /// 2026-08-12 — measured across CI, all five such scenarios pass cleanly
    /// in ~3.5s each. The shell-preexec race that once gated them is a
    /// property of the maintainer's sandbox; `--exclude-real-shell` opts out
    /// there.
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A duplicated name is not a duplicated run: the GUI loop dispatches by
    /// name and the TUI runner keys its outcome map by name, so a second row
    /// with the same name reports the *first* row's result twice. That reads
    /// as extra coverage while actually being none.
    #[test]
    fn every_scenario_name_is_unique_across_both_surfaces() {
        let mut names: Vec<&str> = all_scenarios().iter().map(|s| s.name).collect();
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(
            names.len(),
            total,
            "duplicate scenario name in the manifest"
        );
    }

    /// The TUI runner selects tests with the nextest filter
    /// `test(/(^|::)usage_tui_/)`. A manifest row whose name does not match
    /// that pattern can never be selected, so it would be reported as a
    /// missing test forever — or, worse, silently dropped.
    #[test]
    fn every_tui_scenario_name_carries_the_prefix_the_runner_filters_on() {
        for scenario in TUI_SCENARIOS {
            assert!(
                scenario.name.starts_with("usage_tui_"),
                "{} cannot be selected by the runner's nextest filter",
                scenario.name
            );
        }
    }

    /// The `surface` field decides which runner a row goes to and which
    /// summary bucket it lands in; a row filed in the wrong constant with the
    /// other surface's tag would be run by neither loop.
    #[test]
    fn each_bucket_declares_its_own_surface() {
        assert!(GUI_SCENARIOS.iter().all(|s| s.surface == Surface::Gui));
        assert!(TUI_SCENARIOS.iter().all(|s| s.surface == Surface::Tui));
    }

    /// An untagged scenario is never skipped and never gated — which sounds
    /// harmless until a scenario that needs a real provider is added without
    /// its tag and starts failing every default run.
    #[test]
    fn every_scenario_declares_at_least_one_tag() {
        for scenario in all_scenarios() {
            assert!(
                !scenario.tags.is_empty(),
                "{} declares no tags",
                scenario.name
            );
        }
    }

    /// `reliable-here` means "runs by default in this sandbox". Pairing it
    /// with a gating tag makes the row skip by default while still claiming
    /// to be reliable — the manifest would be saying two different things
    /// about the same scenario, and the gate would silently win.
    #[test]
    fn a_reliable_here_scenario_carries_no_gating_tag() {
        for scenario in all_scenarios() {
            if scenario.has_tag(Tag::ReliableHere) {
                assert_eq!(
                    scenario.tags.len(),
                    1,
                    "{} is tagged reliable-here alongside a gating tag",
                    scenario.name
                );
            }
        }
    }

    /// `all_scenarios` is what `--list` prints and what a caller iterating the
    /// manifest sees; it must be every row, GUI first, with nothing dropped.
    #[test]
    fn all_scenarios_is_every_row_gui_first() {
        let all = all_scenarios();
        assert_eq!(all.len(), GUI_SCENARIOS.len() + TUI_SCENARIOS.len());
        assert!(
            all[..GUI_SCENARIOS.len()]
                .iter()
                .all(|s| s.surface == Surface::Gui)
        );
        assert!(
            all[GUI_SCENARIOS.len()..]
                .iter()
                .all(|s| s.surface == Surface::Tui)
        );
    }

    /// These strings are the NDJSON contract (SCOPE.md §3): they are what a
    /// CI log scraper matches on, so renaming one is a breaking change and
    /// has to be a deliberate edit here rather than a rename side-effect.
    #[test]
    fn surface_and_tag_strings_are_the_reported_wire_values() {
        assert_eq!(Surface::Gui.as_str(), "gui");
        assert_eq!(Surface::Tui.as_str(), "tui");
        assert_eq!(Tag::ReliableHere.as_str(), "reliable-here");
        assert_eq!(Tag::NeedsRealShell.as_str(), "needs-real-shell");
        assert_eq!(Tag::NeedsDesktop.as_str(), "needs-desktop");
        assert_eq!(Tag::NeedsByopProvider.as_str(), "needs-byop-provider");
    }

    #[test]
    fn has_tag_and_tag_strs_report_exactly_the_declared_tags() {
        let scenario = Scenario {
            surface: Surface::Gui,
            name: "usage_example",
            tags: &[Tag::NeedsRealShell, Tag::NeedsDesktop],
        };
        assert!(scenario.has_tag(Tag::NeedsRealShell));
        assert!(scenario.has_tag(Tag::NeedsDesktop));
        assert!(!scenario.has_tag(Tag::ReliableHere));
        assert!(!scenario.has_tag(Tag::NeedsByopProvider));
        assert_eq!(
            scenario.tag_strs(),
            vec!["needs-real-shell", "needs-desktop"]
        );
    }

    /// The manifest exists to be run; an empty bucket means a whole surface
    /// silently contributes nothing while the suite still exits 0.
    #[test]
    fn neither_surface_bucket_is_empty() {
        assert!(!GUI_SCENARIOS.is_empty());
        assert!(!TUI_SCENARIOS.is_empty());
    }
}
