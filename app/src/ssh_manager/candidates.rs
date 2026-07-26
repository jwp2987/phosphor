//! View model for the "Candidates" section — flattens the results of
//! `warp_ssh_manager::load_candidates()` (plus the set of already-imported
//! aliases and collapsed state) into a UI-friendly [`CandidateRow`] list.
//!
//! Design notes (corresponding to
//! `specs/gh-110-ssh-config-import/{PRODUCT,TECH}.md`):
//!
//! - `rows()` is a **pure function**: it only depends on the view-model's
//!   current fields and doesn't touch IO / runtime, so unit tests can
//!   directly construct a `CandidatesViewModel` and assert on the output.
//!   This is exactly what the TDD discussion called for — PR 2's rendering
//!   layer warpui tests are too expensive, so pulling the "which rows should
//!   show" logic out into unit tests is enough to cover the key decisions.
//! - `refresh()` synchronously calls `warp_ssh_manager::load_candidates()`
//!   (a <10KB file — see the tradeoff discussion in TECH.md §3.1), storing
//!   the result into `state`.
//! - `on_tree_changed()` is called by the panel after subscribing to
//!   `SshTreeChangedNotifier` — it collects the `host` field of every server
//!   in the saved tree into a `HashSet`, used as the basis for deciding the
//!   "Added" badge (PRODUCT.md decision E).
//! - "Already imported" is determined by `host == alias`. The import logic
//!   on the panel side sets `server.host` to the candidate alias (PRODUCT.md
//!   decision I), so the comparison semantics here match the import
//!   semantics.
//!
//! All fields are `pub(crate)`, visible only to `panel.rs`;
//! `CandidatesViewModel` itself is exposed via `pub` for re-export in `mod.rs`.

use std::collections::HashSet;

use settings::Setting;
use warp_ssh_manager::{LoadOutcome, LoadResult, SshConfigCandidate, load_candidates};
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::settings::SshSettings;

/// Source + status view in the UI for a candidate server line from `~/.ssh/config`.
pub struct CandidatesViewModel {
    /// The most recent load result. `None` means the model was just created
    /// and no refresh has been triggered yet.
    state: Option<LoadResult>,
    /// The set of `host` fields for all servers in the saved tree. `rows()`
    /// uses it to determine `added`.
    added_aliases: HashSet<String>,
    /// Section collapsed state (PRODUCT.md UX table "Many candidates").
    /// Expanded by default.
    expanded: bool,
}

impl Default for CandidatesViewModel {
    fn default() -> Self {
        Self::new()
    }
}

impl CandidatesViewModel {
    /// All-empty constructor — used when the model is just added to the App
    /// via `add_model`. `refresh()` must be triggered by the caller at the
    /// appropriate time (calling it once immediately in panel's `new` is enough).
    pub fn new() -> Self {
        Self {
            state: None,
            added_aliases: HashSet::new(),
            expanded: true,
        }
    }

    /// Test-only constructor: explicitly injects internal state, bypassing
    /// runtime / IO, to directly drive the various branches of `rows()`.
    #[cfg(test)]
    pub fn with_state(
        state: Option<LoadResult>,
        added_aliases: HashSet<String>,
        expanded: bool,
    ) -> Self {
        Self {
            state,
            added_aliases,
            expanded,
        }
    }

    /// Synchronously re-reads `~/.ssh/config` and stores the result into `state`.
    ///
    /// By design, this doesn't return an error — `LoadOutcome::Error` already
    /// carries back the error message string, and the UI shows it as a red
    /// error row (see PRODUCT.md UX table "Parse / IO error").
    ///
    /// When the "auto-discover SSH hosts" setting is off, skips reading and
    /// clears the state.
    pub fn refresh(&mut self, ctx: &mut ModelContext<Self>) {
        let auto_discover = *SshSettings::as_ref(ctx).enable_ssh_auto_discovery.value();
        if !auto_discover {
            self.state = None;
            ctx.notify();
            return;
        }
        self.state = Some(load_candidates());
        ctx.notify();
    }

    /// Tree-changed callback — rebuilds `added_aliases` from the given server hosts.
    ///
    /// Accepting `impl IntoIterator<Item = String>` instead of
    /// `&SshRepository` means tests don't have to set up a real SQLite
    /// connection; the caller (panel) is responsible for collecting the
    /// `host` field from `list_nodes` + `get_server` into an iterator before
    /// passing it in.
    pub fn on_tree_changed<I>(&mut self, hosts: I, ctx: &mut ModelContext<Self>)
    where
        I: IntoIterator<Item = String>,
    {
        self.added_aliases = hosts.into_iter().collect();
        ctx.notify();
    }

    /// Toggles the "section collapsed" state.
    pub fn toggle_expanded(&mut self, ctx: &mut ModelContext<Self>) {
        self.expanded = !self.expanded;
        ctx.notify();
    }

    /// Whether it's expanded (the panel uses this to decide whether to render body rows).
    pub fn is_expanded(&self) -> bool {
        self.expanded
    }

    /// Looks up a candidate by alias — used when handling the
    /// `ImportCandidate { alias }` action, after which the full fields are
    /// used to call `SshRepository::create_server`.
    pub fn find_candidate(&self, alias: &str) -> Option<&SshConfigCandidate> {
        let state = self.state.as_ref()?;
        match &state.outcome {
            LoadOutcome::Loaded(v) => v.iter().find(|c| c.alias == alias),
            LoadOutcome::NotFound | LoadOutcome::Error(_) => None,
        }
    }

    /// A human-readable string of the current `~/.ssh/config` path (used for
    /// `notes = "Imported from {}"`). `None` means it hasn't been loaded
    /// yet, or the home directory couldn't even be resolved.
    pub fn path_display(&self) -> Option<String> {
        self.state
            .as_ref()
            .and_then(|s| s.path.as_ref())
            .map(|p| p.display().to_string())
    }

    /// Flattens the current state into a row list — see the module docs'
    /// "pure function" contract.
    ///
    /// Output semantics (corresponding to the PRODUCT.md §5 UX table):
    /// - Not yet refreshed: returns an empty Vec (the panel doesn't render
    ///   the section when `state == None`).
    /// - `NotFound`: Header + one `NotFound` row.
    /// - `Error`: Header + one `Error` row (can_refresh=true lets the user
    ///   retry after fixing the config).
    /// - `Loaded(empty)`: Header + one `Empty` row.
    /// - `Loaded(non-empty)`: Header (count = N) + N `Candidate` rows, each
    ///   with `added` decided by `added_aliases.contains(alias)`.
    pub fn rows(&self) -> Vec<CandidateRow> {
        let Some(state) = self.state.as_ref() else {
            return Vec::new();
        };

        let path_display = state
            .path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();

        let mut out = Vec::new();
        let count = match &state.outcome {
            LoadOutcome::Loaded(v) => v.len(),
            LoadOutcome::NotFound | LoadOutcome::Error(_) => 0,
        };
        // Header is always the first row — even when the section is
        // collapsed, the panel still draws the header (that's the toggle
        // entry point). `can_refresh = true` always holds: any state allows
        // the user to click Refresh to re-read.
        out.push(CandidateRow::Header {
            path_display: path_display.clone(),
            count,
            can_refresh: true,
        });

        // When the section is collapsed, only the header is kept; the body isn't rendered.
        if !self.expanded {
            return out;
        }

        match &state.outcome {
            LoadOutcome::NotFound => {
                out.push(CandidateRow::NotFound { path_display });
            }
            LoadOutcome::Error(msg) => {
                out.push(CandidateRow::Error {
                    path_display,
                    message: msg.clone(),
                });
            }
            LoadOutcome::Loaded(v) if v.is_empty() => {
                out.push(CandidateRow::Empty { path_display });
            }
            LoadOutcome::Loaded(v) => {
                for c in v {
                    out.push(CandidateRow::Candidate {
                        alias: c.alias.clone(),
                        hostname: c.hostname.clone(),
                        user: c.user.clone(),
                        port: c.port,
                        identity_file: c.identity_file.as_ref().map(|p| p.display().to_string()),
                        added: self.added_aliases.contains(&c.alias),
                    });
                }
            }
        }

        out
    }
}

/// A UI-friendly row. Header is always first, followed by either a single
/// status row (NotFound / Empty / Error) or a run of Candidate rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CandidateRow {
    Header {
        path_display: String,
        count: usize,
        can_refresh: bool,
    },
    NotFound {
        path_display: String,
    },
    Empty {
        path_display: String,
    },
    Error {
        path_display: String,
        message: String,
    },
    Candidate {
        alias: String,
        hostname: Option<String>,
        user: Option<String>,
        port: Option<u16>,
        identity_file: Option<String>,
        added: bool,
    },
}

impl Entity for CandidatesViewModel {
    type Event = ();
}

#[cfg(test)]
#[path = "candidates_tests.rs"]
mod tests;

// Lets test code not worry about the exact on-disk PathBuf — these helpers
// build a fixed display string via `LoadResult`. Also used inside the test
// module, so they're placed at the outer level for easy #[cfg(test)] reuse.
#[cfg(test)]
pub(crate) fn fake_load_result_loaded(path: &str, cands: Vec<SshConfigCandidate>) -> LoadResult {
    LoadResult {
        path: Some(std::path::PathBuf::from(path)),
        outcome: LoadOutcome::Loaded(cands),
    }
}

#[cfg(test)]
pub(crate) fn fake_load_result_not_found(path: &str) -> LoadResult {
    LoadResult {
        path: Some(std::path::PathBuf::from(path)),
        outcome: LoadOutcome::NotFound,
    }
}

#[cfg(test)]
pub(crate) fn fake_load_result_error(path: &str, msg: &str) -> LoadResult {
    LoadResult {
        path: Some(std::path::PathBuf::from(path)),
        outcome: LoadOutcome::Error(msg.to_string()),
    }
}
