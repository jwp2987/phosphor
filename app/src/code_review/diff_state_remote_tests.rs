//! Tests for [`RemoteDiffStateModel`].
//!
//! Harness shape (`new_for_test` + metadata builders + `warpui::App::test`) is
//! ported from Warp's `warp/master:app/src/code_review/diff_state/remote_tests.rs`,
//! which the fork had never taken. Constructing the model directly rather than
//! through `new` deliberately skips the `GetDiffState` subscription RPC, so the
//! state-folding logic can be exercised without a connected host.

use std::cell::Cell;
use std::rc::Rc;

use warp_util::standardized_path::StandardizedPath;

use super::{InternalRemoteDiffState, RemoteDiffStateModel};
use crate::code::buffer_location::RemotePath;
use crate::code_review::diff_state::{
    DiffMetadata, DiffMetadataAgainstBase, DiffMode, DiffStats, DiffStateModelEvent,
};
use crate::util::git::Commit;

impl RemoteDiffStateModel {
    fn new_for_test(
        mode: DiffMode,
        state: InternalRemoteDiffState,
        metadata: Option<DiffMetadata>,
    ) -> Self {
        Self {
            remote_path: RemotePath::new(
                remote_server::HostId::new("test-host".to_string()),
                StandardizedPath::try_new("/test/repo")
                    .expect("test repo path should be valid and absolute"),
            ),
            mode,
            state,
            metadata,
        }
    }
}

fn commit(subject: &str, hash: &str) -> Commit {
    Commit {
        hash: hash.to_string(),
        subject: subject.to_string(),
        files_changed: 0,
        additions: 0,
        deletions: 0,
    }
}

/// Metadata for a branch that is `unpushed.len()` commits ahead of
/// `upstream_ref`, i.e. the state the UI shows before a push.
fn metadata_with(unpushed: Vec<Commit>, upstream_ref: Option<&str>) -> DiffMetadata {
    DiffMetadata {
        main_branch_name: "main".to_string(),
        current_branch_name: "feature".to_string(),
        against_head: DiffMetadataAgainstBase {
            aggregate_stats: DiffStats::default(),
        },
        against_base_branch: None,
        has_head_commit: true,
        unpushed_commits: unpushed,
        upstream_ref: upstream_ref.map(str::to_string),
        pr_info: None,
    }
}

#[test]
fn apply_git_op_delta_replaces_unpushed_commits_and_upstream() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(
                    vec![commit("local change", "aaa1111")],
                    Some("origin/feature"),
                )),
            )
        });

        // The daemon reports the post-push state: nothing left unpushed.
        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(Vec::new(), Some("origin/feature".to_string()), ctx)
        });

        let (unpushed, upstream) = handle.read(&app, |model, _| {
            (
                model.unpushed_commits().to_vec(),
                model.upstream_ref().map(str::to_string),
            )
        });
        assert!(
            unpushed.is_empty(),
            "a successful push must clear the unpushed set, got {unpushed:?}"
        );
        assert_eq!(upstream.as_deref(), Some("origin/feature"));
    });
}

#[test]
fn apply_git_op_delta_records_a_newly_created_upstream() {
    warpui::App::test((), |mut app| async move {
        // Branch has never been published: no upstream, one local commit.
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(vec![commit("first", "bbb2222")], None)),
            )
        });

        // `git push --set-upstream` creates the tracking ref; the delta carries it.
        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(Vec::new(), Some("origin/feature".to_string()), ctx)
        });

        let upstream = handle.read(&app, |model, _| model.upstream_ref().map(str::to_string));
        assert_eq!(
            upstream.as_deref(),
            Some("origin/feature"),
            "publishing a branch must record the newly created upstream ref"
        );
    });
}

#[test]
fn apply_git_op_delta_keeps_commits_still_unpushed_after_a_commit_only_chain() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(
                    vec![commit("older", "ccc3333")],
                    Some("origin/feature"),
                )),
            )
        });

        // A commit-only chain adds to the unpushed set rather than clearing it.
        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(
                vec![commit("newer", "ddd4444"), commit("older", "ccc3333")],
                Some("origin/feature".to_string()),
                ctx,
            )
        });

        let subjects: Vec<String> = handle.read(&app, |model, _| {
            model
                .unpushed_commits()
                .iter()
                .map(|c| c.subject.clone())
                .collect()
        });
        assert_eq!(subjects, vec!["newer".to_string(), "older".to_string()]);
    });
}

#[test]
fn apply_git_op_delta_emits_metadata_changed() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                Some(metadata_with(
                    vec![commit("local change", "aaa1111")],
                    Some("origin/feature"),
                )),
            )
        });

        // The code-review UI repaints off this event; without it the view keeps
        // rendering the pre-operation unpushed count even though the model moved.
        // Subscriptions live on a context rather than on `App`, so the model
        // subscribes to itself from inside an update (the pattern used by
        // `settings::ai_tests`).
        let saw_metadata_changed = Rc::new(Cell::new(false));
        let observed = saw_metadata_changed.clone();
        let subscribed_handle = handle.clone();
        handle.update(&mut app, move |_, ctx| {
            ctx.subscribe_to_model(
                &subscribed_handle,
                move |_, event: &DiffStateModelEvent, _| {
                    if matches!(event, DiffStateModelEvent::DiffMetadataChanged(_)) {
                        observed.set(true);
                    }
                },
            );
        });

        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(Vec::new(), Some("origin/feature".to_string()), ctx)
        });

        assert!(
            saw_metadata_changed.get(),
            "applying a git-op delta must emit DiffMetadataChanged so the view repaints"
        );
    });
}

#[test]
fn apply_git_op_delta_before_any_snapshot_seeds_metadata() {
    warpui::App::test((), |mut app| async move {
        // No metadata yet: the initial GetDiffState snapshot has not landed.
        // Warp seeds default metadata (`metadata.get_or_insert_with`) so the
        // delta is never dropped; the fork must do the same.
        let handle = app.add_model(|_ctx| {
            RemoteDiffStateModel::new_for_test(
                DiffMode::Head,
                InternalRemoteDiffState::Loading,
                None,
            )
        });

        handle.update(&mut app, |model, ctx| {
            model.apply_git_op_delta(
                vec![commit("orphan", "eee5555")],
                Some("origin/feature".to_string()),
                ctx,
            )
        });

        let (subjects, upstream) = handle.read(&app, |model, _| {
            (
                model
                    .unpushed_commits()
                    .iter()
                    .map(|c| c.subject.clone())
                    .collect::<Vec<_>>(),
                model.upstream_ref().map(str::to_string),
            )
        });
        assert_eq!(subjects, vec!["orphan".to_string()]);
        assert_eq!(upstream.as_deref(), Some("origin/feature"));
    });
}
