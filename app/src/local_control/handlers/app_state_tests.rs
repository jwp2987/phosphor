use ::local_control::protocol::{TabTarget, TargetSelector};
use ::local_control::{ActionKind, ErrorCode};

#[cfg(feature = "local_fs")]
use super::resolve_against_working_directory;
use super::{tab_move, validate_staged_input_text};
use crate::local_control::LocalControlBridge;
use crate::workspace::TabMovement;
use crate::workspace::view::tests::{initialize_app, mock_workspace};
use warp_core::features::FeatureFlag;

#[test]
fn staged_input_rejects_line_breaks_and_control_sequences() {
    assert!(validate_staged_input_text(ActionKind::InputInsert, "safe staged text").is_ok());

    for text in ["line\nbreak", "line\rbreak", "tab\tbreak", "\u{1b}[31m"] {
        let error = validate_staged_input_text(ActionKind::InputInsert, text).err();
        assert!(error.is_some_and(|error| error.code == ErrorCode::InvalidParams));
    }
}

#[cfg(feature = "local_fs")]
#[test]
fn file_open_resolves_relative_paths_against_the_session_working_directory() {
    use std::path::{Path, PathBuf};

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let working_directory = dunce::canonicalize(temp_dir.path()).expect("canonical temp dir");
    let nested = working_directory.join("docs");
    std::fs::create_dir(&nested).expect("nested dir");
    std::fs::write(working_directory.join("README.md"), "# hi").expect("readme");
    std::fs::write(nested.join("guide.md"), "# guide").expect("guide");

    assert_eq!(
        resolve_against_working_directory(Path::new("README.md"), &working_directory),
        working_directory.join("README.md")
    );
    assert_eq!(
        resolve_against_working_directory(Path::new("./README.md"), &working_directory),
        working_directory.join("README.md")
    );
    assert_eq!(
        resolve_against_working_directory(Path::new("docs/guide.md"), &working_directory),
        nested.join("guide.md")
    );
    assert_eq!(
        resolve_against_working_directory(Path::new("../README.md"), &nested),
        working_directory.join("README.md")
    );

    // Paths that do not exist still resolve against the session directory, so a genuine
    // failure reports the session-relative file rather than a process-relative one.
    assert_eq!(
        resolve_against_working_directory(Path::new("missing.md"), &working_directory),
        working_directory.join("missing.md")
    );

    let absolute = PathBuf::from(if cfg!(windows) {
        r"C:\tmp\absolute.md"
    } else {
        "/tmp/absolute.md"
    });
    assert_eq!(
        resolve_against_working_directory(&absolute, &working_directory),
        absolute
    );
}

/// Adapted from the pinned oracle's `unavailable_surface_open_returns_structured_error`
/// (02b53fcd8). The pin exercises `ensure_surface_available` against the
/// feature-flag-gated Agent Management surface; this fork's equivalent surface
/// is unconditionally unavailable (see `metadata_tests.rs`), so this instead
/// exercises the same `ensure_surface_available` structured-error contract
/// through `handle`'s dispatch of `SurfaceAgentManagementOpen`, which is
/// unreachable through the normal is_implemented() gate and returns
/// `UnsupportedAction` directly.
#[test]
fn agent_management_open_action_is_rejected_as_unsupported() {
    warpui::App::test((), |mut app| async move {
        let bridge = app.add_singleton_model(crate::local_control::LocalControlBridge::new);
        let error = bridge
            .update(&mut app, |_bridge, ctx| {
                super::handle(
                    &None,
                    ActionKind::SurfaceAgentManagementOpen,
                    &serde_json::json!({}),
                    &::local_control::protocol::TargetSelector::default(),
                    ctx,
                )
            })
            .expect_err("agent management open is not implemented");
        assert_eq!(error.code, ErrorCode::UnsupportedAction);
        assert!(error.message.contains("surface.agent_management.open"));
    });
}

/// The regression: `tab_move` dispatched `MoveTabLeft`/`MoveTabRight` and acked
/// unconditionally, even though `move_tab` itself silently no-ops a refused move
/// (a pinned/group boundary, or the tab already at the edge) -- so a scripted
/// caller could not tell a performed move from a refused one, and the
/// `can_move_tab` port (2026-08-21) substantially enlarged the refused set.
/// `tab_move` must now check the same predicate `move_tab` checks and return
/// `TargetStateConflict` instead of acking, matching the other three
/// `TargetStateConflict` uses in this file.
#[test]
fn tab_move_refuses_a_move_the_workspace_cannot_perform() {
    let _pinned_guard = FeatureFlag::PinnedTabs.override_enabled(true);
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);
        let tab_ids_before = workspace.update(&mut app, |workspace, ctx| {
            workspace.add_terminal_tab(false, ctx);
            workspace.add_terminal_tab(false, ctx);
            assert_eq!(workspace.tab_count(), 3);
            // [P0, U1, U2]: the pinned/unpinned boundary sits between tabs 0 and
            // 1, so moving tab 1 left would evict the pinned tab -- the same
            // scenario `view.rs`'s own `can_move_tab` pinned-boundary tests use.
            workspace.tabs[0].pinned = true;
            assert!(
                !workspace.can_move_tab(1, TabMovement::Left),
                "test setup is wrong: this must be a move `can_move_tab` refuses"
            );
            workspace
                .tabs
                .iter()
                .map(|tab| tab.pane_group.id())
                .collect::<Vec<_>>()
        });

        let bridge = app.add_singleton_model(LocalControlBridge::new);
        let error = bridge
            .update(&mut app, |_bridge, ctx| {
                tab_move(
                    &None,
                    &serde_json::json!({ "direction": "left" }),
                    &TargetSelector {
                        tab: Some(TabTarget::Index { index: 1 }),
                        ..Default::default()
                    },
                    ctx,
                )
            })
            .expect_err("a move can_move_tab refuses must not ack success");
        assert_eq!(error.code, ErrorCode::TargetStateConflict);

        let tab_ids_after = workspace.read(&app, |workspace, _| {
            workspace
                .tabs
                .iter()
                .map(|tab| tab.pane_group.id())
                .collect::<Vec<_>>()
        });
        assert_eq!(
            tab_ids_before, tab_ids_after,
            "the refused move must not have reordered any tab"
        );
    });
}

/// The mirror of the refusal test: a move `can_move_tab` allows must still ack
/// success and actually perform the move, so the fix does not turn every
/// `tab.move` into a refusal.
#[test]
fn tab_move_performs_and_acks_a_move_the_workspace_can_perform() {
    warpui::App::test((), |mut app| async move {
        initialize_app(&mut app);
        let workspace = mock_workspace(&mut app);
        let (first_id, second_id) = workspace.update(&mut app, |workspace, ctx| {
            workspace.add_terminal_tab(false, ctx);
            assert_eq!(workspace.tab_count(), 2);
            assert!(workspace.can_move_tab(1, TabMovement::Left));
            (
                workspace.tabs[0].pane_group.id(),
                workspace.tabs[1].pane_group.id(),
            )
        });

        let bridge = app.add_singleton_model(LocalControlBridge::new);
        bridge
            .update(&mut app, |_bridge, ctx| {
                tab_move(
                    &None,
                    &serde_json::json!({ "direction": "left" }),
                    &TargetSelector {
                        tab: Some(TabTarget::Index { index: 1 }),
                        ..Default::default()
                    },
                    ctx,
                )
            })
            .expect("a legal move must ack success, not refuse");

        workspace.read(&app, |workspace, _| {
            assert_eq!(
                workspace
                    .tabs
                    .iter()
                    .map(|tab| tab.pane_group.id())
                    .collect::<Vec<_>>(),
                vec![second_id, first_id],
                "the move must actually have swapped the two tabs, not merely acked"
            );
        });
    });
}
