//! Unit tests for `persisted_workspace.rs`.
//!
//! These cover the pure data-structure invariants only (`root_for_workspace`,
//! `workspaces` ordering). Anything that reaches `RepoMetadataModel` or
//! `ProjectContextModel` needs a full `AppContext` with singletons wired up, so
//! it lives in `app/src/workspace/view_test.rs` instead.
//!
//! Note on provenance: the pin (`02b53fcd8`) ships
//! `app/src/ai/persisted_workspace_tests.rs` as a zero-byte placeholder, so
//! there is nothing to port from it. The `root_for_workspace_*` cases below are
//! this fork's own, recovered from `efcaa42b8^` and translated to English; the
//! `merge_enabled_and_auto_start` cases that sat beside them are not recovered,
//! because that helper is part of the LSP leg (see the LSP seam in
//! `persisted_workspace.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ai::workspace::WorkspaceMetadata;
use chrono::{TimeZone, Utc};

use super::{PersistedWorkspace, Workspace};

/// Builds an empty `PersistedWorkspace`. Equivalent to `new_for_test`, but
/// without needing a `ModelContext`.
fn empty_persisted_workspace() -> PersistedWorkspace {
    PersistedWorkspace {
        workspaces: HashMap::new(),
        model_event_sender: None,
        #[cfg(feature = "local_fs")]
        lsp_installation_status: Default::default(),
    }
}

/// Inserts a workspace entry directly, simulating a root that was previously
/// registered through `user_added_workspace` and restored from SQLite.
fn insert_workspace(pw: &mut PersistedWorkspace, path: &Path) {
    pw.workspaces.insert(
        path.to_path_buf(),
        Workspace {
            language_servers: Default::default(),
            metadata: WorkspaceMetadata {
                path: path.to_path_buf(),
                navigated_ts: None,
                modified_ts: None,
                queried_ts: None,
            },
        },
    );
}

#[test]
fn root_for_workspace_returns_none_when_unregistered() {
    // With nothing registered, `root_for_workspace` must return None so callers
    // fall back to repo-metadata detection rather than inventing a root.
    let pw = empty_persisted_workspace();
    let repo = PathBuf::from("/tmp/some-fresh-repo");

    assert!(pw.root_for_workspace(&repo).is_none());
}

#[test]
fn root_for_workspace_resolves_registered_ancestor() {
    // A path inside a registered root resolves to that root. This is what lets
    // a file deep in a repo be attributed to the repo the user actually added.
    let mut pw = empty_persisted_workspace();
    let repo = PathBuf::from("/tmp/registered-repo");
    insert_workspace(&mut pw, &repo);

    let nested = repo.join("src/foo/bar.rs");
    assert_eq!(pw.root_for_workspace(&nested), Some(repo.as_path()));
}

#[test]
fn root_for_workspace_ignores_unrelated_registered_workspace() {
    // Unrelated entries in `self.workspaces` must not pollute the lookup.
    let mut pw = empty_persisted_workspace();
    insert_workspace(&mut pw, &PathBuf::from("/tmp/some-other-repo"));

    let unrelated = PathBuf::from("/tmp/unrelated-repo/src/main.rs");
    assert!(pw.root_for_workspace(&unrelated).is_none());
}

#[test]
fn workspaces_skips_entries_that_were_never_persisted() {
    // An entry with no timestamps at all has no SQLite row behind it, so it must
    // not surface in the "recent repositories" list.
    let mut pw = empty_persisted_workspace();
    insert_workspace(&mut pw, &PathBuf::from("/tmp/never-persisted"));

    assert_eq!(pw.workspaces().count(), 0);
}

#[test]
fn workspaces_orders_by_most_recently_touched() {
    let mut pw = empty_persisted_workspace();

    let older = PathBuf::from("/tmp/older-repo");
    let newer = PathBuf::from("/tmp/newer-repo");

    pw.workspaces.insert(
        older.clone(),
        Workspace {
            language_servers: Default::default(),
            metadata: WorkspaceMetadata {
                path: older.clone(),
                navigated_ts: Some(Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()),
                modified_ts: None,
                queried_ts: None,
            },
        },
    );
    pw.workspaces.insert(
        newer.clone(),
        Workspace {
            language_servers: Default::default(),
            metadata: WorkspaceMetadata {
                path: newer.clone(),
                navigated_ts: Some(Utc.with_ymd_and_hms(2026, 6, 1, 0, 0, 0).unwrap()),
                modified_ts: None,
                queried_ts: None,
            },
        },
    );

    let ordered: Vec<PathBuf> = pw.workspaces().map(|ws| ws.path).collect();
    assert_eq!(ordered, vec![newer, older]);
}
