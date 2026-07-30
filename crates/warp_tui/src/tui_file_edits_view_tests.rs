use std::path::PathBuf;

use ai::diff_validation::{DiffDelta, DiffType};
use futures::channel::oneshot;
use warp::appearance::Appearance;
use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::tui_export::{
    AIAgentActionId, AIConversationId, BlocklistAIActionModel, DiffSessionType, FileDiff,
    RegisteredDiffStorage,
};
use warp_editor::content::buffer::InitialBufferState;
use warp_editor::model::CoreEditorModel;
use warpui::platform::WindowStyle;
use warpui::{AddWindowOptions, ModelHandle, ViewHandle, WindowInvalidation};
use warpui_core::elements::tui::TuiRect;
use warpui_core::keymap::Keystroke;
use warpui_core::presenter::tui::TuiPresenter;
use warpui_core::{App, TuiView as _};

use super::{
    SectionKey, SectionStates, ToolCallDisplayState, TuiFileEditsView, deltas_for,
    file_edit_header_label, verb_and_name,
};
use crate::test_fixtures::{TestHostView, add_test_action_model};
use crate::tui_diff_storage::TuiDiffStorageHandle;

fn delta(range: std::ops::Range<usize>, insertion: &str) -> DiffDelta {
    DiffDelta {
        replacement_line_range: range,
        insertion: insertion.to_owned(),
    }
}

#[test]
fn all_file_edit_sections_start_collapsed_and_toggle_independently() {
    let states = SectionStates::default();

    assert!(states.is_collapsed(SectionKey::Summary));
    assert!(states.is_collapsed(SectionKey::File(0)));
    assert!(states.is_collapsed(SectionKey::File(1)));

    states.toggle_collapsed(SectionKey::File(0));
    assert!(states.is_collapsed(SectionKey::Summary));
    assert!(!states.is_collapsed(SectionKey::File(0)));
    assert!(states.is_collapsed(SectionKey::File(1)));
}
#[test]
fn blocked_file_edit_headers_use_in_progress_wording() {
    assert_eq!(
        file_edit_header_label(ToolCallDisplayState::Blocked, "Edited", "2 files"),
        "Editing 2 files"
    );
    assert_eq!(
        file_edit_header_label(ToolCallDisplayState::Blocked, "Updated", "lib.rs"),
        "Editing lib.rs"
    );

    assert_eq!(
        file_edit_header_label(ToolCallDisplayState::Succeeded, "Edited", "2 files"),
        "Edited 2 files"
    );
    assert_eq!(
        file_edit_header_label(ToolCallDisplayState::Succeeded, "Updated", "lib.rs"),
        "Updated lib.rs"
    );
}

fn update_diff(path: &str, rename: Option<&str>) -> FileDiff {
    FileDiff::new(
        "old\n".to_owned(),
        path.to_owned(),
        DiffType::Update {
            deltas: vec![delta(1..2, "new\n")],
            rename: rename.map(PathBuf::from),
        },
    )
}

#[test]
fn verbs_follow_the_diff_op() {
    let create = FileDiff::new(
        String::new(),
        "/tmp/a/new.rs".to_owned(),
        DiffType::creation("fn main() {}\n".to_owned()),
    );
    assert_eq!(verb_and_name(&create), ("Created", "new.rs".to_owned()));

    assert_eq!(
        verb_and_name(&update_diff("/tmp/a/lib.rs", None)),
        ("Updated", "lib.rs".to_owned())
    );

    let delete = FileDiff::new(
        "gone\n".to_owned(),
        "/tmp/a/old.rs".to_owned(),
        DiffType::Delete {
            delta: delta(1..2, ""),
        },
    );
    assert_eq!(verb_and_name(&delete), ("Deleted", "old.rs".to_owned()));
}

#[test]
fn renames_display_old_and_new_names() {
    assert_eq!(
        verb_and_name(&update_diff("/tmp/a/old.rs", Some("/tmp/a/new.rs"))),
        ("Updated", "old.rs → new.rs".to_owned())
    );
    // A rename to the same file name (e.g. a directory move) shows one name.
    assert_eq!(
        verb_and_name(&update_diff("/tmp/a/lib.rs", Some("/tmp/b/lib.rs"))),
        ("Updated", "lib.rs".to_owned())
    );
}

/// Drives the full body pipeline headlessly: seed a char-cell editor with base
/// content, apply deltas (buffer becomes post-edit and the diff recomputes),
/// expand the hunks, and assert the added-line ranges and the removed-line
/// ghost blocks that the diff body renders from.
#[test]
fn diff_pipeline_computes_added_lines_and_ghost_blocks() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        let editor = app.add_model(|ctx| CodeEditorModel::new_tui(80, ctx));

        let (tx, rx) = oneshot::channel();
        app.update(|ctx| {
            let mut tx = Some(tx);
            ctx.subscribe_to_model(&editor, move |_, event, _| {
                if matches!(event, CodeEditorModelEvent::DiffUpdated)
                    && let Some(tx) = tx.take()
                {
                    let _ = tx.send(());
                }
            });
            editor.update(ctx, |editor, ctx| {
                editor.reset_content(InitialBufferState::plain_text("a\nold\nc\n"), ctx);
                // Replace line 2 ("old") with "new"; delta line ranges are
                // 1-indexed like the executor's resolved deltas.
                editor.apply_diffs(
                    vec![DiffDelta {
                        replacement_line_range: 2..3,
                        insertion: "new\n".to_owned(),
                    }],
                    ctx,
                );
            });
        });
        rx.await.expect("diff computation should complete");

        editor.update(&mut app, |editor, ctx| editor.expand_diffs(ctx));

        // Ghost blocks land via the render state's async layout channel, which
        // is drained on a background thread before the foreground handler stores
        // them. Await the render state's layout-complete signal (outstanding
        // layout actions draining to zero) rather than busy-polling a fixed
        // number of no-op yields, which races that background thread and flakes
        // under load.
        app.read(|app| {
            editor
                .as_ref(app)
                .render_state()
                .as_ref(app)
                .layout_complete()
        })
        .await;

        let ghosts = app.read(|app| {
            editor
                .as_ref(app)
                .render_state()
                .as_ref(app)
                .char_cell()
                .expect("TUI editor renders in char-cell mode")
                .display_lattice(&[])
                .ghosts()
                .to_vec()
        });

        assert_eq!(ghosts.len(), 1);
        assert_eq!(ghosts[0].content, "old\n");
        // The ghost interleaves before the replacement line (0-based line 1).
        assert_eq!(ghosts[0].insert_before.as_u32(), 1);

        app.read(|app| {
            let editor = editor.as_ref(app);
            let diff = editor.diff().as_ref(app);
            let added: Vec<_> = diff.added_or_changed_lines().collect();
            assert_eq!(added, vec![1..2]);
            // Header counts read from this same computed diff, so they always
            // agree with the rendered body (one line replaced by one line).
            assert_eq!(diff.diff_status().get_diff_lines(), (1, 1));
        });
    });
}

#[test]
fn deltas_cover_every_diff_op() {
    let d = delta(1..2, "x\n");
    assert_eq!(
        deltas_for(&DiffType::Create { delta: d.clone() }),
        vec![d.clone()]
    );
    assert_eq!(
        deltas_for(&DiffType::Delete { delta: d.clone() }),
        vec![d.clone()]
    );
    assert_eq!(
        deltas_for(&DiffType::Update {
            deltas: vec![d.clone(), delta(4..5, "y\n")],
            rename: None,
        }),
        vec![d, delta(4..5, "y\n")]
    );
}

#[test]
fn ctrl_t_toggles_the_primary_section_like_the_mouse_click_handler() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| Appearance::mock());
        app.update(super::init);
        // `record_applied_diffs` (run when the storage below is seeded)
        // records into this singleton for `/rewind`; normally registered by
        // `session.rs` at TUI startup.
        app.update(crate::tui_revert_registry::TuiFileEditRevertRegistry::register);
        let action_model = add_test_action_model(&mut app);
        let action_id = AIAgentActionId::from("edit-1".to_owned());
        let conversation_id = AIConversationId::new();
        let view = add_file_edits_view(&mut app, action_id.clone(), conversation_id, &action_model);

        // Seed the diff storage directly (bypassing the executor, which
        // never ran since no action was queued) so the section exists to
        // toggle, and focus the view directly rather than routing focus
        // through a blocked permission prompt — both are orthogonal to what
        // this test cares about: whether ctrl-t reaches `ToggleExpanded`.
        let storage = app.read(|ctx| view.as_ref(ctx).storage.clone());
        app.update(|ctx| {
            TuiDiffStorageHandle::new(storage).set_candidate_diffs(
                vec![update_diff("/tmp/a/lib.rs", None)],
                DiffSessionType::Local,
                ctx,
            );
        });
        view.update(&mut app, |_, ctx| ctx.focus_self());

        app.read(|ctx| {
            let view = view.as_ref(ctx);
            assert_eq!(view.sections.len(), 1);
            assert!(view.section_states.is_collapsed(SectionKey::File(0)));
        });

        present_file_edits_view(&mut app, &view);
        assert!(dispatch_focused_key(&mut app, &view, "ctrl-t"));

        app.read(|ctx| {
            assert!(
                !view
                    .as_ref(ctx)
                    .section_states
                    .is_collapsed(SectionKey::File(0)),
                "ctrl-t should toggle the sole file section, like clicking its header"
            );
        });

        present_file_edits_view(&mut app, &view);
        assert!(dispatch_focused_key(&mut app, &view, "ctrl-t"));
        app.read(|ctx| {
            assert!(
                view.as_ref(ctx)
                    .section_states
                    .is_collapsed(SectionKey::File(0)),
                "a second ctrl-t should collapse it again"
            );
        });
    });
}

fn add_file_edits_view(
    app: &mut App,
    action_id: AIAgentActionId,
    conversation_id: AIConversationId,
    action_model: &ModelHandle<BlocklistAIActionModel>,
) -> ViewHandle<TuiFileEditsView> {
    let action_model = action_model.clone();
    app.update(|ctx| {
        let (window_id, _) = ctx.add_tui_window(
            AddWindowOptions {
                window_style: WindowStyle::NotStealFocus,
                ..Default::default()
            },
            |_| TestHostView,
        );
        ctx.add_typed_action_tui_view(window_id, move |ctx| {
            TuiFileEditsView::new(action_id, conversation_id, &action_model, ctx)
        })
    })
}

fn present_file_edits_view(app: &mut App, view: &ViewHandle<TuiFileEditsView>) {
    let mut presenter = TuiPresenter::new();
    app.update(|ctx| {
        let view_ref = view.as_ref(ctx);
        let prompt = &view_ref.permission_prompt;
        let mut invalidation = WindowInvalidation::default();
        invalidation.updated.insert(view.id());
        invalidation.updated.insert(prompt.id());
        invalidation
            .updated
            .extend(prompt.as_ref(ctx).child_view_ids(ctx));
        presenter.invalidate(&invalidation, ctx, view.window_id(ctx));
        presenter.present(ctx, view, TuiRect::new(0, 0, 80, 20));
    });
}

fn dispatch_focused_key(app: &mut App, view: &ViewHandle<TuiFileEditsView>, key: &str) -> bool {
    let (window_id, responder_chain) = app.read(|ctx| {
        let window_id = view.window_id(ctx);
        let focused = ctx
            .focused_view_id(window_id)
            .expect("file-edits permission interaction has a focused view");
        (window_id, ctx.view_ancestors(window_id, focused))
    });
    app.dispatch_keystroke(
        window_id,
        &responder_chain,
        &Keystroke::parse(key).expect("valid keystroke"),
        false,
    )
    .expect("keystroke dispatch succeeds")
}
