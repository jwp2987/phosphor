use std::io;
use std::path::PathBuf;

use super::*;

/// The exact shape of a refusal from the pre-write conflict check in
/// `warp_files::FileModel::save_if_unchanged`: a complete user-facing sentence
/// that names the file, states that nothing was written, and says the user's
/// own edits survived. Every such refusal arrives as `FileSaveError::Other`.
fn conflict_refusal(path: &str) -> FileSaveError {
    FileSaveError::Other(format!(
        "{path} changed on disk after this change was proposed, so it was not overwritten \
         and your edits are intact. Re-run the request to work from the current file."
    ))
}

/// Regression: the toast used to be a fixed `Failed to save file {path}`, with
/// `error` reaching only the log line. The conflict refusal — the entire
/// user-facing product of the guarded write — never reached the user, who could
/// not tell a refused overwrite (their edits are safe, re-run the request) from
/// a failed write (something is wrong with the disk).
#[test]
fn toast_message_carries_the_conflict_refusal_verbatim() {
    let error = conflict_refusal("/work/src/main.rs");

    let message = save_failure_toast_message("/work/src/main.rs", &error);

    assert_eq!(
        message,
        "/work/src/main.rs changed on disk after this change was proposed, so it was not \
         overwritten and your edits are intact. Re-run the request to work from the current file."
    );
    // The refusal already names the file; it must not be prefixed with the
    // same path a second time.
    assert!(!message.starts_with("Failed to save file"));
}

/// The other `Other` producer: `InlineDiffView::expected_disk_state` refuses
/// when the diff's base content is gone, so there is nothing to compare
/// against. Same requirement — the reason has to survive.
#[test]
fn toast_message_carries_any_other_reason_verbatim() {
    let error = FileSaveError::Other(
        "/work/src/lib.rs was not written: the original contents this edit was based on are \
         no longer available. Nothing was changed."
            .to_owned(),
    );

    let message = save_failure_toast_message("/work/src/lib.rs", &error);

    assert!(
        message.contains("the original contents this edit was based on are no longer available"),
        "reason dropped from toast: {message}"
    );
}

/// `FileSaveError::IOError`'s own `Display` is the constant "IO error when
/// saving file." and drops both the path and the underlying cause, so this
/// variant is the one that needs wrapping. The wrapper is localized.
#[test]
fn toast_message_wraps_io_errors_with_file_and_cause() {
    crate::i18n::init(Some("en"));
    let error = FileSaveError::IOError {
        error: io::Error::new(io::ErrorKind::PermissionDenied, "permission denied"),
        path: PathBuf::from("/work/src/main.rs"),
    };

    let message = save_failure_toast_message("/work/src/main.rs", &error);

    assert_eq!(
        message,
        "Failed to save file /work/src/main.rs: permission denied"
    );
}

// ── Revert settlement (#684) ─────────────────────────────────────────────
//
// `begin_revert` queues each file in a `RevertingDiffs`,
// `dispatch_file_revert` / `abandon_file_revert` record what each file's
// `restore_diff_base` returned (or that it was given up on),
// `handle_save_completed` feeds each write's
// outcome to `CodeDiffState::record_revert_write`, and all of them then call
// `CodeDiffState::settle_revert`, marking the action reverted in the
// conversation if and only if it returns `true`. These tests drive exactly
// those calls; the view around them is glue.

/// A revert whose pending diffs `dispatched` each have a write in flight.
fn reverting(dispatched: &[usize]) -> CodeDiffState {
    let mut reverting = RevertingDiffs::default();
    for &idx in dispatched {
        reverting.write_dispatched(idx);
    }
    CodeDiffState::Reverting(reverting)
}

fn is_accepted(state: &CodeDiffState) -> bool {
    matches!(state, CodeDiffState::Accepted(None))
}

/// The defect: the card entered `Reverted`, and the action was marked
/// reverted, before the guarded write resolved — so a write the guard refused
/// (the file changed after the accept) was recorded as a revert.
#[test]
fn a_revert_refused_at_write_time_leaves_the_card_accepted_and_the_action_unmarked() {
    let mut state = reverting(&[0]);
    assert!(
        !state.settle_revert(),
        "must not settle while the write is in flight"
    );
    assert!(matches!(state, CodeDiffState::Reverting(_)));

    assert!(state.record_revert_write(0, false));
    assert!(
        !state.settle_revert(),
        "a refused revert must not mark the action reverted"
    );
    assert!(is_accepted(&state), "got {state:?}");
}

#[test]
fn a_revert_whose_write_lands_reverts_the_card_and_marks_the_action() {
    let mut state = reverting(&[0]);
    assert!(state.record_revert_write(0, true));
    assert!(state.settle_revert());
    assert!(matches!(state, CodeDiffState::Reverted), "got {state:?}");
}

/// A refusal decided before any write (no record of the accept, no base) or
/// a diff with no backing file: nothing is in flight, so it settles at once —
/// and not as a revert. The latter used to count as success.
#[test]
fn a_revert_with_no_write_dispatched_settles_immediately_as_not_reverted() {
    let mut reverting = RevertingDiffs::default();
    reverting.file_not_reverted(0);
    let mut state = CodeDiffState::Reverting(reverting);
    assert!(!state.settle_revert());
    assert!(is_accepted(&state), "got {state:?}");
}

/// Every file must be reverted for the action to be. One refused file keeps
/// the whole card accepted, whichever order the outcomes arrive in.
#[test]
fn one_refused_file_keeps_a_multi_file_revert_from_being_recorded() {
    for (first, second) in [((0, true), (1, false)), ((1, false), (0, true))] {
        let mut state = reverting(&[0, 1]);
        assert!(state.record_revert_write(first.0, first.1));
        assert!(!state.settle_revert());
        assert!(matches!(state, CodeDiffState::Reverting(_)));
        assert!(state.record_revert_write(second.0, second.1));
        assert!(!state.settle_revert());
        assert!(is_accepted(&state), "got {state:?}");
    }

    let mut state = reverting(&[0, 1]);
    assert!(state.record_revert_write(1, true));
    assert!(!state.settle_revert());
    assert!(state.record_revert_write(0, true));
    assert!(state.settle_revert());
    assert!(matches!(state, CodeDiffState::Reverted));
}

/// A write outcome this revert is not waiting on — a duplicate, or an index it
/// never dispatched — must not resolve the revert or be counted as a success.
#[test]
fn an_outcome_for_a_write_not_in_flight_is_ignored() {
    let mut state = reverting(&[0]);
    assert!(!state.record_revert_write(1, true));
    assert!(!state.settle_revert());
    assert!(matches!(state, CodeDiffState::Reverting(_)));

    assert!(state.record_revert_write(0, false));
    assert!(!state.record_revert_write(0, true), "already resolved");
    assert!(!state.settle_revert());
    assert!(is_accepted(&state));
}

/// Outside `Reverting` a write outcome is not a revert's, and settling is a
/// no-op: an accepted card is not turned into a reverted one, nor a reverted
/// one back.
#[test]
fn outside_a_revert_nothing_settles() {
    let mut accepted = CodeDiffState::Accepted(None);
    assert!(!accepted.record_revert_write(0, true));
    assert!(!accepted.settle_revert());
    assert!(is_accepted(&accepted));

    let mut reverted = CodeDiffState::Reverted;
    assert!(!reverted.record_revert_write(0, false));
    assert!(!reverted.settle_revert());
    assert!(matches!(reverted, CodeDiffState::Reverted));
}

/// A retry skips the files an earlier, partly refused attempt already
/// reverted. If those were all of them, nothing is dispatched and nothing
/// was refused: the card is reverted.
#[test]
fn a_retry_with_nothing_left_to_write_is_a_revert() {
    let mut state = CodeDiffState::Reverting(RevertingDiffs::default());
    assert!(state.settle_revert());
    assert!(matches!(state, CodeDiffState::Reverted));
}

/// A revert in flight is still a finished action (so a late
/// `FinishedAction` does not reopen it), and has not undone anything yet.
#[test]
fn a_revert_in_flight_is_complete() {
    assert!(reverting(&[0]).is_complete());
}

// ── Queued files (#686) ──────────────────────────────────────────────────
//
// A rewind queues every file at `begin_revert` and dispatches each one only
// when the newer reverts of the same file have settled. A queued file is
// outstanding: the card must not settle past it.

/// Every queued file must still be dispatched (or given up on) before the
/// card settles — otherwise the first file to land would mark the whole
/// action reverted while an older file had not been written yet.
#[test]
fn a_queued_file_keeps_the_revert_open_until_it_lands() {
    let mut reverting = RevertingDiffs::default();
    reverting.write_queued(0);
    reverting.write_queued(1);
    reverting.write_dispatched(0);
    let mut state = CodeDiffState::Reverting(reverting);

    assert!(state.record_revert_write(0, true));
    assert!(
        !state.settle_revert(),
        "file 1 is queued, not reverted: the card must not settle"
    );
    assert!(matches!(state, CodeDiffState::Reverting(_)));

    let CodeDiffState::Reverting(reverting) = &mut state else {
        unreachable!()
    };
    reverting.write_dispatched(1);
    assert!(!state.settle_revert());
    assert!(state.record_revert_write(1, true));
    assert!(state.settle_revert());
    assert!(matches!(state, CodeDiffState::Reverted));
}

/// A queued file given up on (a newer revert of the same file did not land)
/// was not reverted, so neither is the card — even if every other file was.
#[test]
fn an_abandoned_queued_file_leaves_the_card_accepted() {
    let mut reverting = RevertingDiffs::default();
    reverting.write_queued(0);
    reverting.write_queued(1);
    reverting.write_dispatched(0);
    let mut state = CodeDiffState::Reverting(reverting);
    assert!(state.record_revert_write(0, true));

    let CodeDiffState::Reverting(reverting) = &mut state else {
        unreachable!()
    };
    reverting.file_not_reverted(1);
    assert!(!state.settle_revert());
    assert!(is_accepted(&state), "got {state:?}");
}

/// A write outcome for a file that is queued but was never dispatched is not
/// one this revert is waiting on.
#[test]
fn an_outcome_for_a_queued_file_not_yet_dispatched_is_ignored() {
    let mut reverting = RevertingDiffs::default();
    reverting.write_queued(0);
    let mut state = CodeDiffState::Reverting(reverting);
    assert!(!state.record_revert_write(0, true));
    assert!(!state.settle_revert());
    assert!(matches!(state, CodeDiffState::Reverting(_)));
}

/// A file whose accept write was refused or failed was never written, so a
/// revert must not touch it: its guard would report a false "changed on
/// disk" and, in a rewind, abandon every older revert of that file — and for
/// a refused creation whose text happens to match, the guarded delete would
/// remove a file the agent never created. Files an earlier attempt already
/// reverted are skipped the same way.
#[test]
fn a_revert_skips_files_the_accept_never_wrote() {
    let none = HashSet::new();
    assert_eq!(
        files_to_revert(3, &none, &none).collect::<Vec<_>>(),
        [0, 1, 2]
    );

    let accept_failed = HashSet::from([1]);
    assert_eq!(
        files_to_revert(3, &none, &accept_failed).collect::<Vec<_>>(),
        [0, 2]
    );

    let reverted = HashSet::from([0]);
    assert_eq!(
        files_to_revert(3, &reverted, &accept_failed).collect::<Vec<_>>(),
        [2]
    );

    // Nothing the accept wrote: nothing to write, so the card settles as
    // reverted at once (see `a_retry_with_nothing_left_to_write_is_a_revert`).
    let all_failed = HashSet::from([0, 1]);
    assert!(files_to_revert(2, &none, &all_failed).next().is_none());
}

// ── Accept reporting: `rename_report` (#688) ──────────────────────────────
//
// `try_emit_diffs_saved` used to decide a file's report from its raw
// `DiffType`, so a remote session's in-place rename fallback (no rename
// primitive there — see `InlineDiffView::write_action`) was still reported as
// a move: the original path went into `deleted_files` and the destination
// into `updated_files`, even though the write never touched either path
// differently from an ordinary update. `rename_report` is the fix: it reports
// what `write_action` says actually happened.

/// A local rename reports the destination as the update target and the
/// original path as the one to mark deleted.
#[test]
fn rename_report_moves_the_reported_path_and_marks_the_original_deleted() {
    let action = FileWriteAction::Rename(PathBuf::from("/work/new.rs"));
    let (reported_path, renamed_from) = rename_report(&action, "/work/old.rs");
    assert_eq!(reported_path, "/work/new.rs");
    assert_eq!(renamed_from, Some("/work/old.rs".to_owned()));
}

/// An ordinary write — including a remote rename's in-place fallback, which
/// `write_action` has already resolved to `Write` before this ever sees it —
/// reports an update at the registered path and nothing as deleted.
#[test]
fn rename_report_leaves_an_ordinary_write_at_its_own_path() {
    let (reported_path, renamed_from) = rename_report(&FileWriteAction::Write, "/work/old.rs");
    assert_eq!(reported_path, "/work/old.rs");
    assert_eq!(renamed_from, None);
}

/// A delete never reaches `rename_report` in `try_emit_diffs_saved` (it is
/// handled in its own branch), but the function itself must still treat it
/// like an ordinary write rather than panicking or mis-reporting a rename.
#[test]
fn rename_report_treats_a_delete_like_an_ordinary_write() {
    let (reported_path, renamed_from) = rename_report(&FileWriteAction::Delete, "/work/gone.rs");
    assert_eq!(reported_path, "/work/gone.rs");
    assert_eq!(renamed_from, None);
}

// ── Remote rename UI: `get_rename_target` / `is_rename_without_changes` /
//    `is_remote_rename_fallback` (#688 review, finding 4) ──────────────────
//
// The tab label and the "renamed without changes" placeholder used to read
// the raw `DiffType`, so a remote session -- which has no rename primitive
// and falls back to an in-place write, per `InlineDiffView::write_action` --
// still showed "old -> new" and claimed a rename that never happened. These
// three functions are now driven by `write_action`, not the raw diff.

fn rename_diff(deltas: Vec<DiffDelta>, to: &str) -> DiffType {
    DiffType::update(deltas, Some(to.to_owned()))
}

fn non_empty_delta() -> DiffDelta {
    DiffDelta {
        replacement_line_range: 0..1,
        insertion: "changed\n".to_owned(),
    }
}

#[test]
fn get_rename_target_is_none_for_a_write_or_delete_action() {
    assert_eq!(
        CodeDiffView::get_rename_target(&FileWriteAction::Write),
        None
    );
    assert_eq!(
        CodeDiffView::get_rename_target(&FileWriteAction::Delete),
        None
    );
}

#[test]
fn get_rename_target_returns_the_destination_for_a_local_rename() {
    let action = FileWriteAction::Rename(PathBuf::from("/work/new.rs"));
    assert_eq!(
        CodeDiffView::get_rename_target(&action),
        Some(Path::new("/work/new.rs"))
    );
}

/// The regression: a remote session's fallback is `Write`, not `Rename`, so
/// the tab must not show an arrow to a path the accept will never create.
#[test]
fn is_remote_rename_fallback_is_true_only_for_a_write_with_a_proposed_rename() {
    let diff = rename_diff(vec![], "/work/new.rs");
    assert!(CodeDiffView::is_remote_rename_fallback(
        &FileWriteAction::Write,
        Some(&diff)
    ));
    assert!(!CodeDiffView::is_remote_rename_fallback(
        &FileWriteAction::Rename(PathBuf::from("/work/new.rs")),
        Some(&diff)
    ));
    let plain_update = DiffType::update(vec![], None);
    assert!(!CodeDiffView::is_remote_rename_fallback(
        &FileWriteAction::Write,
        Some(&plain_update)
    ));
}

#[test]
fn is_rename_without_changes_requires_both_a_rename_action_and_no_deltas() {
    let no_deltas = rename_diff(vec![], "/work/new.rs");
    let action = FileWriteAction::Rename(PathBuf::from("/work/new.rs"));
    assert!(CodeDiffView::is_rename_without_changes(
        &action,
        Some(&no_deltas)
    ));

    let with_deltas = rename_diff(vec![non_empty_delta()], "/work/new.rs");
    assert!(!CodeDiffView::is_rename_without_changes(
        &action,
        Some(&with_deltas)
    ));
}

/// The regression, directly: a remote fallback with no content changes must
/// NOT show the "renamed without changes" placeholder -- the write really
/// does touch the file (an ordinary, if byte-identical, write), so the real
/// editor is what is honest to show, not a claim that a move happened.
#[test]
fn is_rename_without_changes_is_false_for_a_remote_fallback_even_with_no_deltas() {
    let no_deltas = rename_diff(vec![], "/work/new.rs");
    assert!(!CodeDiffView::is_rename_without_changes(
        &FileWriteAction::Write,
        Some(&no_deltas)
    ));
}
