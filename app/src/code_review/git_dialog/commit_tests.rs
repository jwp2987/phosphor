//! Tests for the commit-dialog Confirm gate.
//!
//! Warp has no `git_dialog` tests at all (`warp/master:app/src/code_review/git_dialog/`
//! contains no test files), so there is nothing to port here and these are
//! fork-authored. They exist because this gate has already regressed once: a
//! remote bypass (`is_remote || !file_changes.is_empty()`) was added, which let
//! Confirm go live on a remote repo with an empty Changes list and fire a
//! commit the daemon then rejected. Warp gates local and remote identically and
//! treats the daemon's empty-commit rejection as a backstop, not a substitute.

use std::cell::RefCell;
use std::rc::Rc;

use warp_core::ui::appearance::Appearance;
use warpui::elements::{ClippedScrollStateHandle, MouseStateHandle};
use warpui::platform::WindowStyle;
use warpui::ui_components::switch::SwitchStateHandle;
use warpui::{App, Element};

use super::{confirm_tooltip, is_ready_to_confirm, CommitIntent, CommitState};
use crate::auth::AuthStateProvider;
use crate::editor::EditorView;
use crate::settings_view::keybindings::KeybindingChangedNotifier;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::util::git::FileChangeEntry;
use crate::view_components::action_button::{ActionButton, SecondaryTheme};
use crate::vim_registers::VimRegisters;
use crate::workspace::sync_inputs::SyncedInputState;
use crate::workspace::ToastStack;
use crate::workspaces::user_workspaces::UserWorkspaces;

/// Minimal window root; the dialog itself is not rendered, only its state is
/// built, so the window just needs some view to own.
#[derive(Default)]
struct TestView;

impl warpui::Entity for TestView {
    type Event = ();
}

impl warpui::View for TestView {
    fn render(&self, _: &warpui::AppContext) -> Box<dyn Element> {
        warpui::elements::Empty::new().finish()
    }

    fn ui_name() -> &'static str {
        "TestView"
    }
}

impl warpui::TypedActionView for TestView {
    type Action = ();
}

fn file_change(path: &str) -> FileChangeEntry {
    FileChangeEntry {
        path: path.to_string(),
        additions: 1,
        deletions: 0,
    }
}

/// Builds a real `CommitState` inside a window, so the gate is exercised
/// against actual editor/button views rather than a stand-in. `message` is
/// written into the message editor; empty means "user has typed nothing yet".
fn commit_state(app: &mut App, file_changes: Vec<FileChangeEntry>, message: &str) -> CommitState {
    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| ToastStack);
    app.add_singleton_model(|_| SyncedInputState::mock());
    app.add_singleton_model(|_| VimRegisters::new());
    app.add_singleton_model(|_| KeybindingChangedNotifier::mock());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    #[cfg(feature = "voice_input")]
    app.add_singleton_model(voice_input::VoiceInput::new);
    app.add_singleton_model(|ctx| UserWorkspaces::mock(vec![], ctx));

    let holder: Rc<RefCell<Option<CommitState>>> = Rc::new(RefCell::new(None));
    let sink = holder.clone();
    let message = message.to_string();

    app.add_window(WindowStyle::NotStealFocus, move |ctx| {
        let message_editor = ctx.add_typed_action_view(|ctx| {
            EditorView::new_with_base_text(message.as_str(), Default::default(), ctx)
        });
        let commit_button = ctx
            .add_typed_action_view(|_ctx| ActionButton::new("Commit".to_string(), SecondaryTheme));
        let commit_and_push_button = ctx.add_typed_action_view(|_ctx| {
            ActionButton::new("Commit and push".to_string(), SecondaryTheme)
        });

        *sink.borrow_mut() = Some(CommitState {
            intent: CommitIntent::CommitOnly,
            include_unstaged: true,
            file_changes,
            changes_expanded: true,
            switch_state: SwitchStateHandle::default(),
            summary_mouse_state: MouseStateHandle::default(),
            changes_scroll_state: ClippedScrollStateHandle::default(),
            message_editor,
            commit_button,
            commit_and_push_button,
            commit_and_create_pr_button: None,
        });
        TestView
    });

    holder.borrow_mut().take().expect("commit state built")
}

#[test]
fn confirm_requires_both_changes_and_a_message() {
    App::test((), |mut app| async move {
        let state = commit_state(&mut app, vec![file_change("a.rs")], "add a.rs");
        assert!(
            app.update(|ctx| is_ready_to_confirm(&state, ctx)),
            "changes plus a message should enable Confirm"
        );
        assert_eq!(app.update(|ctx| confirm_tooltip(&state, ctx)), None);
    });
}

#[test]
fn confirm_disabled_when_message_is_empty() {
    App::test((), |mut app| async move {
        // Mirrors the open-time autogen window: changes exist, the editor is
        // still empty while the draft is in flight.
        let state = commit_state(&mut app, vec![file_change("a.rs")], "");
        assert!(!app.update(|ctx| is_ready_to_confirm(&state, ctx)));
        assert_eq!(
            app.update(|ctx| confirm_tooltip(&state, ctx)),
            Some("Enter a commit message"),
            "with changes present, the tooltip must point at the missing message"
        );
    });
}

#[test]
fn confirm_disabled_when_message_is_only_whitespace() {
    App::test((), |mut app| async move {
        let state = commit_state(&mut app, vec![file_change("a.rs")], "   \n  ");
        assert!(
            !app.update(|ctx| is_ready_to_confirm(&state, ctx)),
            "a whitespace-only message is not a commit message"
        );
    });
}

/// The regression guard. `file_changes` is empty for BOTH local and remote —
/// the gate has no remote bypass. If someone reintroduces one, this fails.
#[test]
fn confirm_disabled_when_there_is_nothing_to_commit() {
    App::test((), |mut app| async move {
        let state = commit_state(&mut app, Vec::new(), "a message with nothing to commit");
        assert!(
            !app.update(|ctx| is_ready_to_confirm(&state, ctx)),
            "Confirm must stay disabled with an empty Changes list, on remote as well as \
             local: the daemon's empty-commit rejection is a backstop, not a replacement \
             for this guard"
        );
    });
}

/// No changes AND no message: still disabled, and the tooltip stays silent
/// rather than blaming the message, since the message is not the blocker.
#[test]
fn no_changes_and_no_message_shows_no_tooltip() {
    App::test((), |mut app| async move {
        let state = commit_state(&mut app, Vec::new(), "");
        assert!(!app.update(|ctx| is_ready_to_confirm(&state, ctx)));
        assert_eq!(app.update(|ctx| confirm_tooltip(&state, ctx)), None);
    });
}
