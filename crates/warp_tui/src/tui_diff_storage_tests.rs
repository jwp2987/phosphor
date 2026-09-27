use std::fs;

use warp_core::HostId;
use warpui::App;

use super::*;

/// Builds a [`TuiDiffStorage`] over `diffs`, registering the `FileModel` its
/// writes go through. Guarded so a test that calls this more than once (e.g.
/// accept then revert on the same app) does not hit `add_singleton_model`'s
/// "called twice" debug assertion.
fn add_tui_storage(
    app: &mut App,
    diffs: Vec<FileDiff>,
    session_type: DiffSessionType,
) -> ModelHandle<TuiDiffStorage> {
    if !app.read(|ctx| ctx.has_singleton_model::<FileModel>()) {
        app.add_singleton_model(FileModel::new);
    }
    app.add_model(|_| TuiDiffStorage::new(diffs, session_type))
}

/// Runs the shared accept flow for local diffs on a fresh app and awaits the result.
async fn accept_local(app: &mut App, diffs: Vec<FileDiff>) -> RequestFileEditsResult {
    let model = add_tui_storage(app, diffs, DiffSessionType::Local);
    let future = model.update(app, |model, ctx| model.accept_and_save(ctx));
    future.await
}

/// Runs `revert_file_diffs` for local diffs on a fresh (or already-`accept_local`'d)
/// app and awaits every outcome. Reuses `add_tui_storage`'s guarded `FileModel`
/// registration and a throwaway `TuiDiffStorage` purely to obtain a
/// `ModelContext`, which derefs to the `AppContext` `revert_file_diffs` needs.
async fn revert_local(app: &mut App, diffs: Vec<FileDiff>) -> Vec<FileRevertOutcome> {
    let model = add_tui_storage(app, Vec::new(), DiffSessionType::Local);
    let future = model.update(app, |_model, ctx| revert_file_diffs(diffs, ctx));
    future.await
}

#[test]
fn accept_creates_a_new_file() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.rs").to_string_lossy().to_string();

        let result = accept_local(
            &mut app,
            vec![FileDiff::new(
                String::new(),
                path.clone(),
                DiffType::creation("fn main() {}\n".to_owned()),
            )],
        )
        .await;

        let RequestFileEditsResult::Success {
            updated_files,
            deleted_files,
            lines_added,
            ..
        } = result
        else {
            panic!("expected create to succeed");
        };
        assert_eq!(fs::read_to_string(&path).unwrap(), "fn main() {}\n");
        assert_eq!(lines_added, 1);
        assert_eq!(deleted_files, Vec::<String>::new());
        assert_eq!(updated_files.len(), 1);
        assert_eq!(updated_files[0].file_context.file_name, path);
    });
}

#[test]
fn accept_applies_deltas_to_update_a_file() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs").to_string_lossy().to_string();
        fs::write(&path, "one\ntwo\nthree\n").unwrap();

        let result = accept_local(
            &mut app,
            vec![FileDiff::new(
                "one\ntwo\nthree\n".to_owned(),
                path.clone(),
                DiffType::update(
                    vec![DiffDelta {
                        replacement_line_range: 2..3,
                        // Production insertions omit the trailing newline.
                        insertion: "TWO".to_owned(),
                    }],
                    None,
                ),
            )],
        )
        .await;

        assert!(matches!(result, RequestFileEditsResult::Success { .. }));
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\nTWO\nthree\n");
    });
}

#[test]
fn accept_renames_and_reports_source_as_deleted() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("old.rs").to_string_lossy().to_string();
        let new_path = dir.path().join("new.rs").to_string_lossy().to_string();
        fs::write(&old_path, "content\n").unwrap();

        let result = accept_local(
            &mut app,
            vec![FileDiff::new(
                "content\n".to_owned(),
                old_path.clone(),
                DiffType::update(Vec::new(), Some(new_path.clone())),
            )],
        )
        .await;

        let RequestFileEditsResult::Success { deleted_files, .. } = result else {
            panic!("expected rename to succeed");
        };
        assert_eq!(fs::read_to_string(&new_path).unwrap(), "content\n");
        assert!(!Path::new(&old_path).exists());
        assert_eq!(deleted_files, vec![old_path]);
    });
}

#[test]
fn accept_deletes_a_file() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.rs").to_string_lossy().to_string();
        fs::write(&path, "delete me\n").unwrap();

        let result = accept_local(
            &mut app,
            vec![FileDiff::new(
                "delete me\n".to_owned(),
                path.clone(),
                DiffType::deletion(1),
            )],
        )
        .await;

        let RequestFileEditsResult::Success { deleted_files, .. } = result else {
            panic!("expected delete to succeed");
        };
        assert!(!Path::new(&path).exists());
        assert_eq!(deleted_files, vec![path]);
    });
}

#[test]
fn accept_reports_write_dispatch_failure() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        // Create a regular file, then target a path *under* it. Creating the parent
        // directory fails because a file exists where a directory is expected.
        let blocking_file = dir.path().join("not_a_dir");
        fs::write(&blocking_file, "x").unwrap();
        let path = blocking_file.join("child.rs").to_string_lossy().to_string();

        let result = accept_local(
            &mut app,
            vec![FileDiff::new(
                String::new(),
                path,
                DiffType::creation("data\n".to_owned()),
            )],
        )
        .await;

        assert!(matches!(
            result,
            RequestFileEditsResult::DiffApplicationFailed { .. }
        ));
    });
}

#[test]
fn persist_action_renames_only_on_local_sessions() {
    let rename_op = DiffType::update(Vec::new(), Some("/tmp/new.rs".to_owned()));

    assert!(matches!(
        PersistAction::resolve(&rename_op, &DiffSessionType::Local, "/tmp/old.rs"),
        PersistAction::Rename(_)
    ));
    // Remote sessions have no rename primitive: the file is written in place.
    assert!(matches!(
        PersistAction::resolve(
            &rename_op,
            &DiffSessionType::Remote(HostId::new("host".to_owned())),
            "/tmp/old.rs"
        ),
        PersistAction::Write
    ));
    // A "rename" to the same path is just a write.
    assert!(matches!(
        PersistAction::resolve(&rename_op, &DiffSessionType::Local, "/tmp/new.rs"),
        PersistAction::Write
    ));
}

#[test]
fn remote_rename_outcome_reports_update_at_original_path() {
    // The reported outcome must match the actual write: a remote rename falls
    // back to writing the original path, so nothing is deleted and the update
    // is reported at the original path — not the rename target.
    let op = DiffType::update(Vec::new(), Some("/tmp/new.rs".to_owned()));
    let diff = FileDiff::new("content\n".to_owned(), "/tmp/old.rs".to_owned(), op.clone());
    let action = PersistAction::resolve(
        &op,
        &DiffSessionType::Remote(HostId::new("host".to_owned())),
        "/tmp/old.rs",
    );

    let state = persist_outcome(&action, &diff, "/tmp/old.rs", "content!\n");

    let updated = state.updated.expect("expected an updated file");
    assert_eq!(updated.path, "/tmp/old.rs");
    assert_eq!(state.deleted_paths, Vec::<String>::new());
}

#[test]
fn final_content_from_op_applies_deltas() {
    // No surface-supplied content (no editor buffers): final content is
    // derived from the diff's deltas.
    let op = DiffType::update(
        vec![DiffDelta {
            replacement_line_range: 2..3,
            insertion: "TWO\n".to_owned(),
        }],
        None,
    );

    let final_content = final_content_from_op("one\ntwo\nthree\n", &op).unwrap();

    assert_eq!(final_content, "one\nTWO\nthree\n");
}

#[test]
fn apply_deltas_normalizes_newline_less_insertions() {
    // Insertions commonly omit the trailing newline (e.g. search/replace
    // blocks are joined with "\n"); a raw splice would run the replacement
    // into the next preserved line ("one\nTWOthree\n").
    let deltas = vec![DiffDelta {
        replacement_line_range: 2..3,
        insertion: "TWO\nTWO-AND-A-HALF".to_owned(),
    }];

    let final_content = apply_deltas_to_content("one\ntwo\nthree\n", &deltas).unwrap();

    assert_eq!(final_content, "one\nTWO\nTWO-AND-A-HALF\nthree\n");
}

// --- `revert_plan` (pure): the write(s) a revert dispatches for each diff shape ---

#[test]
fn revert_plan_for_create_deletes_the_file() {
    // The accept wrote the insertion; undoing it deletes the file, guarded on
    // the insertion still being there (the accept's own content, not the base).
    let diff = FileDiff::new(
        String::new(),
        "/tmp/new.rs".to_owned(),
        DiffType::creation("fn main() {}\n".to_owned()),
    );

    let steps = revert_plan(&diff, "/tmp/new.rs").unwrap();

    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].path, "/tmp/new.rs");
    assert!(matches!(steps[0].action, PersistAction::Delete));
    assert_eq!(
        steps[0].expected,
        ExpectedDiskState::Content("fn main() {}\n".to_owned())
    );
}

#[test]
fn revert_plan_for_delete_recreates_the_file() {
    // The accept removed the file; undoing it writes the base content back,
    // guarded on the path still being free.
    let diff = FileDiff::new(
        "gone\n".to_owned(),
        "/tmp/gone.rs".to_owned(),
        DiffType::deletion(1),
    );

    let steps = revert_plan(&diff, "/tmp/gone.rs").unwrap();

    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].path, "/tmp/gone.rs");
    assert!(matches!(steps[0].action, PersistAction::Write));
    assert_eq!(steps[0].content, "gone\n");
    assert_eq!(steps[0].expected, ExpectedDiskState::Absent);
}

#[test]
fn revert_plan_for_rename_restores_original_and_deletes_target() {
    // The accept moved the file; undoing it is two guarded steps with two
    // different pre-images, not a single `PersistAction::Rename`.
    let diff = FileDiff::new(
        "content\n".to_owned(),
        "/tmp/old.rs".to_owned(),
        DiffType::update(Vec::new(), Some("/tmp/new.rs".to_owned())),
    );

    let steps = revert_plan(&diff, "/tmp/old.rs").unwrap();

    assert_eq!(steps.len(), 2);
    assert_eq!(steps[0].path, "/tmp/old.rs");
    assert!(matches!(steps[0].action, PersistAction::Write));
    assert_eq!(steps[0].content, "content\n");
    assert_eq!(steps[0].expected, ExpectedDiskState::Absent);
    assert_eq!(steps[1].path, "/tmp/new.rs");
    assert!(matches!(steps[1].action, PersistAction::Delete));
    assert_eq!(
        steps[1].expected,
        ExpectedDiskState::Content("content\n".to_owned())
    );
}

#[test]
fn revert_plan_for_in_place_update_writes_the_base_back() {
    let op = DiffType::update(
        vec![DiffDelta {
            replacement_line_range: 2..3,
            insertion: "TWO".to_owned(),
        }],
        None,
    );
    let diff = FileDiff::new(
        "one\ntwo\nthree\n".to_owned(),
        "/tmp/main.rs".to_owned(),
        op,
    );

    let steps = revert_plan(&diff, "/tmp/main.rs").unwrap();

    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].path, "/tmp/main.rs");
    assert!(matches!(steps[0].action, PersistAction::Write));
    assert_eq!(steps[0].content, "one\ntwo\nthree\n");
    assert_eq!(
        steps[0].expected,
        ExpectedDiskState::Content("one\nTWO\nthree\n".to_owned())
    );
}

// --- `revert_file_diffs` (end-to-end through `FileModel`) ---

#[test]
fn revert_undoes_an_accepted_create() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.rs").to_string_lossy().to_string();
        let diff = FileDiff::new(
            String::new(),
            path.clone(),
            DiffType::creation("fn main() {}\n".to_owned()),
        );

        let accept_result = accept_local(&mut app, vec![diff.clone()]).await;
        assert!(matches!(
            accept_result,
            RequestFileEditsResult::Success { .. }
        ));
        assert!(Path::new(&path).exists());

        let outcomes = revert_local(&mut app, vec![diff]).await;

        assert_eq!(outcomes, vec![FileRevertOutcome::Reverted]);
        assert!(!Path::new(&path).exists());
    });
}

#[test]
fn revert_undoes_an_accepted_update() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs").to_string_lossy().to_string();
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let diff = FileDiff::new(
            "one\ntwo\nthree\n".to_owned(),
            path.clone(),
            DiffType::update(
                vec![DiffDelta {
                    replacement_line_range: 2..3,
                    insertion: "TWO".to_owned(),
                }],
                None,
            ),
        );

        accept_local(&mut app, vec![diff.clone()]).await;
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\nTWO\nthree\n");

        let outcomes = revert_local(&mut app, vec![diff]).await;

        assert_eq!(outcomes, vec![FileRevertOutcome::Reverted]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\ntwo\nthree\n");
    });
}

#[test]
fn revert_undoes_an_accepted_delete() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.rs").to_string_lossy().to_string();
        fs::write(&path, "delete me\n").unwrap();
        let diff = FileDiff::new(
            "delete me\n".to_owned(),
            path.clone(),
            DiffType::deletion(1),
        );

        accept_local(&mut app, vec![diff.clone()]).await;
        assert!(!Path::new(&path).exists());

        let outcomes = revert_local(&mut app, vec![diff]).await;

        assert_eq!(outcomes, vec![FileRevertOutcome::Reverted]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "delete me\n");
    });
}

#[test]
fn revert_undoes_an_accepted_rename() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("old.rs").to_string_lossy().to_string();
        let new_path = dir.path().join("new.rs").to_string_lossy().to_string();
        fs::write(&old_path, "content\n").unwrap();
        let diff = FileDiff::new(
            "content\n".to_owned(),
            old_path.clone(),
            DiffType::update(Vec::new(), Some(new_path.clone())),
        );

        accept_local(&mut app, vec![diff.clone()]).await;
        assert!(!Path::new(&old_path).exists());
        assert!(Path::new(&new_path).exists());

        let outcomes = revert_local(&mut app, vec![diff]).await;

        assert_eq!(outcomes, vec![FileRevertOutcome::Reverted]);
        assert!(Path::new(&old_path).exists());
        assert_eq!(fs::read_to_string(&old_path).unwrap(), "content\n");
        assert!(!Path::new(&new_path).exists());
    });
}

#[test]
fn revert_refuses_when_line_endings_were_converted_after_the_accept() {
    // `ExpectedDiskState::Content` compares LF-normalised text, but a guarded write
    // also refuses when it would change the file's line-ending convention
    // (`warp_files::compare_pre_image` / `line_ending_style`): a file converted to
    // CRLF after the accept has had every line changed, and writing the LF original
    // back would silently revert all of them. The revert must refuse and leave the
    // converted file alone.
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs").to_string_lossy().to_string();
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let diff = FileDiff::new(
            "one\ntwo\nthree\n".to_owned(),
            path.clone(),
            DiffType::update(
                vec![DiffDelta {
                    replacement_line_range: 2..3,
                    insertion: "TWO".to_owned(),
                }],
                None,
            ),
        );

        accept_local(&mut app, vec![diff.clone()]).await;
        // Simulate a CRLF conversion of the exact content the accept just wrote.
        fs::write(&path, "one\r\nTWO\r\nthree\r\n").unwrap();

        let outcomes = revert_local(&mut app, vec![diff]).await;

        assert_eq!(
            outcomes,
            vec![FileRevertOutcome::Refused { path: path.clone() }]
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\r\nTWO\r\nthree\r\n");
    });
}

#[test]
fn revert_is_refused_when_the_file_changed_since_accept() {
    // A formatter, a build step, or the user editing the file between the
    // accept and the `/rewind` must refuse the revert rather than clobbering
    // their work — and the refusal must be visible in the returned outcome,
    // not just the log.
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs").to_string_lossy().to_string();
        fs::write(&path, "one\ntwo\nthree\n").unwrap();
        let diff = FileDiff::new(
            "one\ntwo\nthree\n".to_owned(),
            path.clone(),
            DiffType::update(
                vec![DiffDelta {
                    replacement_line_range: 2..3,
                    insertion: "TWO".to_owned(),
                }],
                None,
            ),
        );

        accept_local(&mut app, vec![diff.clone()]).await;
        assert_eq!(fs::read_to_string(&path).unwrap(), "one\nTWO\nthree\n");
        // Something touches the file after the accept, before the rewind.
        fs::write(&path, "one\nTWO\nthree\nFOUR\n").unwrap();

        let outcomes = revert_local(&mut app, vec![diff]).await;

        assert_eq!(
            outcomes,
            vec![FileRevertOutcome::Refused { path: path.clone() }]
        );
        // The refused write must not have touched the file.
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "one\nTWO\nthree\nFOUR\n"
        );
    });
}

#[test]
fn revert_reports_one_outcome_per_diff_mixing_success_and_refusal() {
    App::test((), |mut app| async move {
        let dir = tempfile::tempdir().unwrap();
        let ok_path = dir.path().join("ok.rs").to_string_lossy().to_string();
        let blocked_path = dir.path().join("blocked.rs").to_string_lossy().to_string();
        fs::write(&ok_path, "one\n").unwrap();
        fs::write(&blocked_path, "one\n").unwrap();
        let ok_diff = FileDiff::new(
            "one\n".to_owned(),
            ok_path.clone(),
            DiffType::update(
                vec![DiffDelta {
                    replacement_line_range: 1..2,
                    insertion: "ONE".to_owned(),
                }],
                None,
            ),
        );
        let blocked_diff = FileDiff::new(
            "one\n".to_owned(),
            blocked_path.clone(),
            DiffType::update(
                vec![DiffDelta {
                    replacement_line_range: 1..2,
                    insertion: "ONE".to_owned(),
                }],
                None,
            ),
        );

        accept_local(&mut app, vec![ok_diff.clone(), blocked_diff.clone()]).await;
        // Only the second file is touched after accept.
        fs::write(&blocked_path, "tampered\n").unwrap();

        let outcomes = revert_local(&mut app, vec![ok_diff, blocked_diff]).await;

        assert_eq!(
            outcomes,
            vec![
                FileRevertOutcome::Reverted,
                FileRevertOutcome::Refused {
                    path: blocked_path.clone()
                },
            ]
        );
        assert_eq!(fs::read_to_string(&ok_path).unwrap(), "one\n");
        assert_eq!(fs::read_to_string(&blocked_path).unwrap(), "tampered\n");
    });
}
