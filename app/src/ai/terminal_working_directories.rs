//! The open terminals' working directories -- the single canonical answer to
//! "which directories does the app currently have a terminal in?".
//!
//! Zap: upstream Warp keeps `all_working_directories` as a free function at the
//! bottom of `app/src/ai/persisted_workspace.rs` (pin `02b53fcd8`, line 1273)
//! and imports it into `ai::outline::native`. This fork dropped
//! `persisted_workspace` along with codebase indexing, so `outline/native.rs`
//! carried a private copy. The function lives here instead: a sibling of both
//! `ai::outline` and a future restored `ai::persisted_workspace`, reachable
//! from either.
//!
//! **This module is the single source of truth.** When the codebase-indexing
//! seams are restored (Delta D2c -- see the `all_working_directories` bullet in
//! the "INDEXING SEAM" comment block of `ai/persisted_workspace.rs`), the
//! restoration must call [`all_working_directories`] here rather than
//! re-introducing the pin's copy. Two implementations of "which directories are
//! open" are free to drift, and the drift would be silent: every consumer is a
//! directory-scoped feature that would simply act on a slightly different set
//! of directories, with nothing to trace the difference back to.
//!
//! Not to be confused with `crate::pane_group::working_directories`, which also
//! exists at the pin and is a different thing: a per-`PaneGroup` model of
//! `LocalOrRemotePath` roots, maintained incrementally from terminal-cd events
//! and consumed by the file tree / code review / global search. This module is
//! the stateless, app-wide sweep -- no pane-group scoping, no host, plain
//! `PathBuf`s.

use std::collections::HashSet;
use std::path::PathBuf;

use warpui::AppContext;

use crate::terminal::view::TerminalView;

/// Collects the working directories of all open terminal views, across every
/// window.
///
/// Behaviour, preserved verbatim from the pin so that consumers cannot tell the
/// two apart:
///
/// - The result is a `HashSet`, so a directory open in several terminals
///   appears once and the iteration order is unspecified. No caller may depend
///   on ordering.
/// - Terminal views with no working directory yet -- no active block metadata,
///   e.g. a shell that has not reported one -- are skipped, rather than
///   contributing an empty path.
/// - Paths come verbatim from [`TerminalView::pwd`]. They are not canonicalized
///   and not filtered to local sessions: a remote session contributes its path
///   *on the remote host*, stored as a plain `PathBuf` with the host dropped.
///   (`TerminalView::pwd_if_local` is the local-only variant, and is
///   deliberately not what this uses -- the pin uses `pwd`.)
///
/// On `wasm` this has no callers -- `ai::outline` compiles its `wasm.rs`
/// variant there -- hence the `allow(dead_code)` on the `mod` declaration in
/// `ai/mod.rs`. The pin carries the same shape of allow for the same reason
/// (`#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]`).
pub(crate) fn all_working_directories(app: &AppContext) -> HashSet<PathBuf> {
    let mut working_directories = HashSet::new();
    for window_id in app.window_ids() {
        for terminal_view in app
            .views_of_type::<TerminalView>(window_id)
            .into_iter()
            .flatten()
            .map(|handle| handle.as_ref(app))
        {
            insert_working_directory(&mut working_directories, terminal_view.pwd());
        }
    }
    working_directories
}

/// Folds one terminal view's reported working directory into the accumulating
/// set.
///
/// Split out of [`all_working_directories`] purely so the part that needs no
/// running app -- skip `None`, de-duplicate, `String` -> `PathBuf` -- is
/// directly testable.
fn insert_working_directory(working_directories: &mut HashSet<PathBuf>, pwd: Option<String>) {
    if let Some(dir) = pwd {
        working_directories.insert(dir.into());
    }
}

#[cfg(test)]
#[path = "terminal_working_directories_tests.rs"]
mod tests;
