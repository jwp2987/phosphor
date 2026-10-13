//! Unit tests for [`BlocklistAIInputModel`] input handling.
//!
//! Covers [`resolve_history_match`] (#312), which pins down the NLD history-match
//! decision matrix between command history and agent prompt history, and the
//! policy-driven mechanism where the initial config, the locked-AI gate, and the
//! AI-settings reactive subscription all defer to the injected [`InputModePolicy`]
//! (#313).
//!
//! For [`resolve_history_match`], each [`HistoryMatch`] argument models one history
//! source: `NoMatch` means the source had no close match, `MatchedAt` carries the
//! matched entry's timestamp, and `MatchedWithoutTimestamp` is a match with no
//! timestamp (command-history-file entries may have no timestamp; agent prompt
//! entries always carry one). Ported verbatim from the pinned oracle
//! (`02b53fcd8:app/src/ai/blocklist/input_model_tests.rs`) -- these 9 tests only
//! exercise the pure decision function, so they need none of the fork's
//! `InputModePolicy` adaptations below.
//!
//! The #313 tests below are ported from the pinned oracle, adapted to the fork's
//! simpler `InputModePolicy` (no full `InputTypeAutoDetectionSource`, per
//! `input_mode_policy.rs`'s doc comment) and built via [`BlocklistAIInputModel::new_tui`]
//! rather than the pin's single `new`, since the policy seam under test
//! (`new_inner`) is shared by both surfaces and `new_tui` needs no
//! `AgentViewController` fixture.
//!
//! `conversation_events_apply_policy_updates` and
//! `conversation_events_with_inert_policy_leave_config_unchanged` pin the
//! conversation-selection half of that seam: `new_inner` subscribes to the surface's
//! `ConversationSelection` (read back from the context model, which both surfaces already
//! construct with it) and routes every `ConversationSelectionEvent` at
//! `InputModePolicy::config_on_conversation_selection_changed`, applying a config write only
//! when the policy returns `Some`. Both shipped policies currently return `None`
//! (`GuiInputModePolicy` deliberately, since the GUI still transitions on
//! `AgentViewControllerEvent`), so the stub policy here is what proves the route is live.

use std::rc::Rc;
use std::sync::Arc;

use chrono::Duration;
use parking_lot::FairMutex;
use settings::Setting as _;
use warpui::r#async::executor::Background;
use warpui::{App, AppContext, EntityId, ModelHandle, SingletonEntity};

use super::*;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::agent_view::AgentViewEntryOrigin;
use crate::ai::blocklist::conversation_selection::{
    ConversationSelectionEvent, ConversationSelectionHandle,
};
use crate::ai::blocklist::input_mode_policy::{InputModePolicy, PolicyConfigUpdate};
use crate::ai::blocklist::BlocklistAIContextModel;
use crate::settings::{AISettings, AISettingsChangedEvent};
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::terminal::color::{self, Colors};
use crate::terminal::event_listener::ChannelEventListener;
use crate::terminal::model::test_utils::block_size;
use crate::terminal::model::TerminalModel;
use crate::test_util::settings::initialize_settings_for_tests;

/// Returns a timestamp and a strictly-later timestamp, for ordering assertions.
fn earlier_and_later() -> (DateTime<Local>, DateTime<Local>) {
    let earlier = Local::now();
    let later = earlier + Duration::seconds(1);
    (earlier, later)
}

const HISTORY_MATCH_AI: Option<(InputType, InputTypeAutoDetectionSource)> =
    Some((InputType::AI, InputTypeAutoDetectionSource::HistoryMatch));
const HISTORY_MATCH_SHELL: Option<(InputType, InputTypeAutoDetectionSource)> =
    Some((InputType::Shell, InputTypeAutoDetectionSource::HistoryMatch));

#[test]
fn no_match_from_either_source_is_not_history_match() {
    // Neither command nor prompt history matched: the caller must fall through
    // to the classifier, so we cannot report a `HistoryMatch` decision.
    assert_eq!(
        resolve_history_match(HistoryMatch::NoMatch, HistoryMatch::NoMatch),
        None,
    );
}

#[test]
fn prompt_only_match_locks_to_ai_history_match() {
    let (_, prompt_ts) = earlier_and_later();
    assert_eq!(
        resolve_history_match(HistoryMatch::NoMatch, HistoryMatch::MatchedAt(prompt_ts)),
        HISTORY_MATCH_AI,
    );
}

#[test]
fn command_only_match_locks_to_shell_history_match() {
    let (command_ts, _) = earlier_and_later();
    assert_eq!(
        resolve_history_match(HistoryMatch::MatchedAt(command_ts), HistoryMatch::NoMatch),
        HISTORY_MATCH_SHELL,
    );
}

#[test]
fn command_only_match_without_timestamp_locks_to_shell_history_match() {
    // History-file commands can match without carrying a timestamp.
    assert_eq!(
        resolve_history_match(HistoryMatch::MatchedWithoutTimestamp, HistoryMatch::NoMatch),
        HISTORY_MATCH_SHELL,
    );
}

#[test]
fn both_match_prompt_newer_locks_to_ai() {
    let (command_ts, prompt_ts) = earlier_and_later();
    assert_eq!(
        resolve_history_match(
            HistoryMatch::MatchedAt(command_ts),
            HistoryMatch::MatchedAt(prompt_ts),
        ),
        HISTORY_MATCH_AI,
    );
}

#[test]
fn both_match_command_newer_locks_to_shell() {
    let (prompt_ts, command_ts) = earlier_and_later();
    assert_eq!(
        resolve_history_match(
            HistoryMatch::MatchedAt(command_ts),
            HistoryMatch::MatchedAt(prompt_ts),
        ),
        HISTORY_MATCH_SHELL,
    );
}

#[test]
fn both_match_equal_timestamps_prefer_shell() {
    // The newer-wins check is strict, so a tie cannot prove the prompt is more
    // recent and we preserve the Shell short-circuit.
    let ts = Local::now();
    assert_eq!(
        resolve_history_match(HistoryMatch::MatchedAt(ts), HistoryMatch::MatchedAt(ts)),
        HISTORY_MATCH_SHELL,
    );
}

#[test]
fn both_match_command_without_timestamp_locks_to_ai() {
    // A timestamped prompt match beats a command match with no timestamp
    // (e.g. a shell history-file entry): the prompt is the only entry whose
    // recency we can establish, so it is treated as more recent.
    let (_, prompt_ts) = earlier_and_later();
    assert_eq!(
        resolve_history_match(
            HistoryMatch::MatchedWithoutTimestamp,
            HistoryMatch::MatchedAt(prompt_ts),
        ),
        HISTORY_MATCH_AI,
    );
}

#[test]
fn both_match_prompt_without_timestamp_prefer_shell() {
    // Without a prompt timestamp we cannot prove the prompt is newer, so we
    // preserve the Shell short-circuit (prompt entries always carry a timestamp
    // in practice; this pins the defensive fallback).
    let (command_ts, _) = earlier_and_later();
    assert_eq!(
        resolve_history_match(
            HistoryMatch::MatchedAt(command_ts),
            HistoryMatch::MatchedWithoutTimestamp,
        ),
        HISTORY_MATCH_SHELL,
    );
    assert_eq!(
        resolve_history_match(
            HistoryMatch::MatchedWithoutTimestamp,
            HistoryMatch::MatchedWithoutTimestamp,
        ),
        HISTORY_MATCH_SHELL,
    );
}

const AI_LOCKED: InputConfig = InputConfig {
    input_type: InputType::AI,
    is_locked: true,
};
const SHELL_LOCKED: InputConfig = InputConfig {
    input_type: InputType::Shell,
    is_locked: true,
};
const SHELL_UNLOCKED: InputConfig = InputConfig {
    input_type: InputType::Shell,
    is_locked: false,
};

/// Configurable [`InputModePolicy`] stub.
struct StubPolicy {
    initial: InputConfig,
    allows_locked_ai: bool,
    autodetection_enabled: bool,
    on_conversation_activated: Option<InputConfig>,
    on_conversation_deactivated: Option<InputConfig>,
    on_settings_changed: Option<InputConfig>,
}

impl StubPolicy {
    /// A policy with `initial` config that permits locked AI and never reacts
    /// to conversation-selection or AI-settings events.
    fn inert(initial: InputConfig) -> Self {
        Self {
            initial,
            allows_locked_ai: true,
            autodetection_enabled: false,
            on_conversation_activated: None,
            on_conversation_deactivated: None,
            on_settings_changed: None,
        }
    }
}

impl InputModePolicy for StubPolicy {
    fn initial_config(&self, _app: &AppContext) -> InputConfig {
        self.initial
    }

    fn allows_locked_ai_input(&self, _app: &AppContext) -> bool {
        self.allows_locked_ai
    }

    fn is_autodetection_enabled(&self, _app: &AppContext) -> bool {
        self.autodetection_enabled
    }

    fn config_on_conversation_selection_changed(
        &self,
        event: &ConversationSelectionEvent,
        _current: InputConfig,
        _app: &AppContext,
    ) -> Option<PolicyConfigUpdate> {
        match event {
            ConversationSelectionEvent::Changed => None,
            ConversationSelectionEvent::Activated { .. } => {
                self.on_conversation_activated.map(PolicyConfigUpdate::new)
            }
            ConversationSelectionEvent::Deactivated { .. } => self
                .on_conversation_deactivated
                .map(PolicyConfigUpdate::new),
        }
    }

    fn config_on_ai_settings_changed(
        &self,
        _event: &AISettingsChangedEvent,
        _current: InputConfig,
        _is_autodetection_enabled_for_current_context: bool,
        _app: &AppContext,
    ) -> Option<PolicyConfigUpdate> {
        self.on_settings_changed.map(PolicyConfigUpdate::new)
    }
}

/// Builds a TUI-surface input model driven by `policy`, returning the conversation-selection
/// handle so tests can emit selection events. Uses `new_tui` (no `AgentViewController` fixture
/// needed) since the policy seam under test — `new_inner`'s initial config,
/// `set_input_config_internal`'s lock gate, and the AI-settings and conversation-selection
/// subscriptions — is shared by both surfaces.
fn build_input_model(
    app: &mut App,
    policy: StubPolicy,
) -> (
    ModelHandle<BlocklistAIInputModel>,
    ConversationSelectionHandle,
) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| CLIAgentSessionsModel::new());

    let terminal_model = Arc::new(FairMutex::new(TerminalModel::new_for_test(
        block_size(),
        color::List::from(&Colors::default()),
        ChannelEventListener::new_for_test(),
        Arc::new(Background::default()),
        false, /* should_show_bootstrap_block */
        None,  /* restored_blocks */
        false, /* honor_ps1 */
        false, /* is_inverted */
        None,  /* session_startup_path */
    )));
    let terminal_view_id = EntityId::new();
    let conversation_selection = app
        .add_model(|_| Box::new(MockConversationSelection) as Box<dyn ConversationSelection>);
    let ai_context_model = app.add_model(|_| {
        BlocklistAIContextModel::mock_agent_view_less(
            terminal_model.clone(),
            terminal_view_id,
            conversation_selection.clone(),
        )
    });
    let input_model = app.add_model(|ctx| {
        BlocklistAIInputModel::new_tui(
            terminal_model,
            ai_context_model,
            Rc::new(policy),
            terminal_view_id,
            ctx,
        )
    });
    (input_model, conversation_selection)
}

#[test]
fn initial_config_comes_from_policy() {
    App::test((), |mut app| async move {
        // A locked-AI initial config sticks — no hardcoded GUI/TUI gating overrides it.
        let (input_model, _) = build_input_model(&mut app, StubPolicy::inert(AI_LOCKED));
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_LOCKED);
        });
    });
}

#[test]
fn locked_ai_write_requires_policy_permission() {
    App::test((), |mut app| async move {
        let policy = StubPolicy {
            allows_locked_ai: false,
            ..StubPolicy::inert(SHELL_UNLOCKED)
        };
        let (input_model, _) = build_input_model(&mut app, policy);

        // Rejected: the policy forbids locking to AI.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_config(AI_LOCKED, true, ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), SHELL_UNLOCKED);
        });

        // Locked shell (and unlocked AI) writes are not gated.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_config(SHELL_LOCKED, true, ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), SHELL_LOCKED);
        });
    });
}

#[test]
fn conversation_events_with_inert_policy_leave_config_unchanged() {
    App::test((), |mut app| async move {
        // The subscription must route events at the policy, not rewrite the config itself: a
        // policy that answers `None` to every selection event leaves the initial config intact.
        let (input_model, conversation_selection) =
            build_input_model(&mut app, StubPolicy::inert(AI_LOCKED));

        conversation_selection.update(&mut app, |_, ctx| {
            ctx.emit(ConversationSelectionEvent::Activated {
                is_fullscreen: true,
                origin: AgentViewEntryOrigin::Cli,
            });
            ctx.emit(ConversationSelectionEvent::Deactivated {
                conversation_id: AIConversationId::new(),
                final_exchange_count: 0,
                is_exit_before_new_entrance: false,
            });
        });

        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_LOCKED);
        });
    });
}

#[test]
fn conversation_events_apply_policy_updates() {
    App::test((), |mut app| async move {
        let policy = StubPolicy {
            on_conversation_activated: Some(SHELL_LOCKED),
            on_conversation_deactivated: Some(AI_LOCKED),
            ..StubPolicy::inert(SHELL_UNLOCKED)
        };
        let (input_model, conversation_selection) = build_input_model(&mut app, policy);

        conversation_selection.update(&mut app, |_, ctx| {
            ctx.emit(ConversationSelectionEvent::Activated {
                is_fullscreen: false,
                origin: AgentViewEntryOrigin::Cli,
            });
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), SHELL_LOCKED);
        });

        conversation_selection.update(&mut app, |_, ctx| {
            ctx.emit(ConversationSelectionEvent::Deactivated {
                conversation_id: AIConversationId::new(),
                final_exchange_count: 0,
                is_exit_before_new_entrance: false,
            });
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_LOCKED);
        });
    });
}

#[test]
fn settings_change_applies_policy_update() {
    App::test((), |mut app| async move {
        let policy = StubPolicy {
            on_settings_changed: Some(SHELL_LOCKED),
            ..StubPolicy::inert(AI_LOCKED)
        };
        let (input_model, _) = build_input_model(&mut app, policy);

        // Default is opt-in (off), so flipping to `true` guarantees a changed event.
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_autodetection_enabled_internal
                .set_value(true, ctx)
                .unwrap();
        });

        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), SHELL_LOCKED);
        });
    });
}

/// Puts the input model's active block into the "agent-requested command, CLI subagent not yet
/// spawned" state: `is_agent_driving_command()` is true, but `is_agent_in_control_or_tagged_in()`
/// is false because no `long_running_control_state` has been set yet.
fn mark_active_block_as_agent_requested_command(
    app: &mut App,
    input_model: &ModelHandle<BlocklistAIInputModel>,
) {
    input_model.update(app, |model, _| {
        let mut terminal = model.model.lock();
        let block = terminal.block_list_mut().active_block_mut();
        block.set_agent_interaction_mode_for_requested_command(
            String::from("requested-command").into(),
            None,
            AIConversationId::new(),
        );
        assert!(block.is_agent_driving_command());
        assert!(!block.is_agent_in_control_or_tagged_in());
    });
}

#[test]
fn submission_keeps_ai_locked_while_agent_drives_requested_command() {
    App::test((), |mut app| async move {
        // Regression: the GUI's submit handler used the narrower
        // `is_agent_in_control_or_tagged_in()`, so a submission made while an agent-requested
        // command was waiting for its CLI subagent fell through to the unlock branch and left
        // the input in Shell mode -- a follow-up meant for the agent could then be queued as a
        // shell command. The pin keeps the input locked to AI via
        // `is_terminal_use_active_or_pending()`.
        let (input_model, _) = build_input_model(&mut app, StubPolicy::inert(SHELL_UNLOCKED));
        mark_active_block_as_agent_requested_command(&mut app, &input_model);

        input_model.update(&mut app, |model, ctx| {
            model.handle_input_buffer_submitted(ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_LOCKED);
        });
    });
}

#[test]
fn autodetection_disabled_while_agent_drives_requested_command() {
    App::test((), |mut app| async move {
        let policy = StubPolicy {
            autodetection_enabled: true,
            ..StubPolicy::inert(SHELL_UNLOCKED)
        };
        let (input_model, _) = build_input_model(&mut app, policy);

        // Control: with no agent involvement, the policy's setting decides.
        input_model.read(&app, |model, ctx| {
            assert!(model.is_autodetection_enabled_for_current_context(ctx));
        });

        // Regression: NLD must stay off while the agent drives a command, even before the CLI
        // subagent has taken formal control, so it cannot reclassify a line meant for the agent
        // as Shell.
        mark_active_block_as_agent_requested_command(&mut app, &input_model);
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_autodetection_enabled_for_current_context(ctx));
        });
    });
}

// ---------------------------------------------------------------------------------------------
// #833: Enter must route on the verdict for the text being submitted.
//
// The view records every user edit that requests detection (`note_detection_requested`), and its
// Enter handler waits -- bounded -- while `is_detection_pending_for` says the buffer's verdict is
// still out. These tests pin the model half of that contract, the narrowed explicit-set window
// (F7) and the typing-speed-independent one-off shortcut (F8).
// ---------------------------------------------------------------------------------------------

const AI_UNLOCKED: InputConfig = InputConfig {
    input_type: InputType::AI,
    is_locked: false,
};

/// A buffer the #808 pre-filter settles as Shell synchronously, with no classifier involved.
const PREFILTERED_COMMAND: &str = "cat foo.txt | grep -i error";

/// Builds an input model with autodetection live (unlocked `initial`, policy enabled), plus the
/// singletons the async classification path reads.
fn build_autodetecting_input_model(
    app: &mut App,
    initial: InputConfig,
) -> ModelHandle<BlocklistAIInputModel> {
    let policy = StubPolicy {
        autodetection_enabled: true,
        ..StubPolicy::inert(initial)
    };
    let (input_model, _) = build_input_model(app, policy);
    app.add_singleton_model(crate::input_classifier::InputClassifierModel::new);
    app.add_singleton_model(crate::PrivacySettings::mock);
    input_model
}

async fn parsed(buffer_text: &str) -> ParsedTokensSnapshot {
    warp_completer::util::parse_current_commands_and_tokens(
        buffer_text.to_owned(),
        &crate::completer::EmptyCompletionContext::new(),
    )
    .await
}

/// Requests detection for a parsed buffer the way the view does for a user edit: note it, then
/// run detection on its parse. Synchronous, so tests that exercise the 250 ms window can parse
/// up front and stay well inside it.
fn request_detection(
    app: &mut App,
    input_model: &ModelHandle<BlocklistAIInputModel>,
    snapshot: ParsedTokensSnapshot,
) {
    input_model.update(app, |model, ctx| {
        model.note_detection_requested(snapshot.buffer_text.clone());
        model.detect_and_set_input_type(
            snapshot,
            crate::completer::EmptyCompletionContext::new(),
            None,
            ctx,
        );
    });
}

/// Pumps the executors until detection for `buffer_text` is no longer pending (2 s cap).
async fn wait_until_settled(
    app: &mut App,
    input_model: &ModelHandle<BlocklistAIInputModel>,
    buffer_text: &str,
) {
    for _ in 0..400 {
        let is_pending = input_model.read(&*app, |model, ctx| {
            model.is_detection_pending_for(buffer_text, ctx)
        });
        if !is_pending {
            return;
        }
        warpui::r#async::Timer::after(std::time::Duration::from_millis(5)).await;
    }
    panic!("detection for {buffer_text:?} never settled");
}

#[test]
fn edited_buffer_is_pending_until_its_own_verdict_lands() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);

        // The edit makes this buffer's input type undecided; other text is not affected.
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested(PREFILTERED_COMMAND.to_owned());
        });
        input_model.read(&app, |model, ctx| {
            assert!(model.is_detection_pending_for(PREFILTERED_COMMAND, ctx));
            assert!(!model.is_detection_pending_for("something else", ctx));
            // The live type is still the previous buffer's verdict -- exactly what Enter used to
            // route on.
            assert_eq!(model.input_type(), InputType::AI);
        });

        // The detection for the new text settles it, and the verdict is for the new text.
        let snapshot = parsed(PREFILTERED_COMMAND).await;
        request_detection(&mut app, &input_model, snapshot);
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_pending_for(PREFILTERED_COMMAND, ctx));
            assert_eq!(model.input_type(), InputType::Shell);
        });
    });
}

#[test]
fn waiter_fires_when_in_flight_classification_settles() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        // AI before the edit: the one-off shortcut settles `yes please` as AI on the async path
        // without depending on which classifier is compiled in.
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        let buffer_text = "yes please";

        let snapshot = parsed(buffer_text).await;
        request_detection(&mut app, &input_model, snapshot);
        let mut settled = input_model.update(&mut app, |model, _| {
            assert!(model.is_detection_in_flight_for(buffer_text));
            model.detection_settled_receiver()
        });
        input_model.read(&app, |model, ctx| {
            assert!(model.is_detection_pending_for(buffer_text, ctx));
        });

        wait_until_settled(&mut app, &input_model, buffer_text).await;
        assert_eq!(settled.try_recv(), Ok(Some(())));
        input_model.read(&app, |model, _| {
            assert!(!model.is_detection_in_flight_for(buffer_text));
            assert_eq!(model.input_type(), InputType::AI);
        });
    });
}

#[test]
fn explicit_lock_is_never_pending_at_enter() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);

        // ctrl-I (or any explicit lock): detection does not run, so Enter has nothing to wait
        // for and routes on the locked type.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_config(AI_LOCKED, true, ctx);
            model.note_detection_requested("find the bug in main.rs".to_owned());
        });
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_pending_for("find the bug in main.rs", ctx));
            assert_eq!(model.input_config(), AI_LOCKED);
        });
    });
}

#[test]
fn explicit_toggle_covers_the_buffer_it_was_made_for() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);

        let new_text = "cat foo.txt | grep -i errors";
        let explicit_snapshot = parsed(PREFILTERED_COMMAND).await;
        let new_snapshot = parsed(new_text).await;

        // An unlocked explicit write (a slash command, a history pick) followed by the edit that
        // accompanied it: Enter must not wait, and detection must not overrule the choice.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_type(InputType::AI, ctx);
        });
        request_detection(&mut app, &input_model, explicit_snapshot);
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_pending_for(PREFILTERED_COMMAND, ctx));
            assert_eq!(model.input_type(), InputType::AI);
        });

        // New text inside the same 250 ms window is a new question (F7).
        request_detection(&mut app, &input_model, new_snapshot);
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_pending_for(new_text, ctx));
            assert_eq!(model.input_type(), InputType::Shell);
        });
    });
}

#[test]
fn submit_window_does_not_swallow_pasted_text() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        // Pin the unlock path `handle_input_buffer_submitted` takes on this surface.
        let _agent_view = FeatureFlag::AgentView.override_enabled(false);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_autodetection_enabled_internal
                .set_value(true, ctx)
                .unwrap();
        });

        let pasted = parsed(PREFILTERED_COMMAND).await;

        // Submitting resets the input for the now-empty buffer and opens the 250 ms window.
        input_model.update(&mut app, |model, ctx| {
            model.handle_input_buffer_submitted(ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_UNLOCKED);
        });

        // A paste inside the window is classified rather than dropped: before #833 the mode
        // stayed AI here until the next keystroke.
        request_detection(&mut app, &input_model, pasted);
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_pending_for(PREFILTERED_COMMAND, ctx));
            assert_eq!(model.input_type(), InputType::Shell);
        });
    });
}

#[test]
fn one_off_shortcut_keys_off_the_last_non_autodetected_type() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);

        // An autodetected guess for an intermediate keystroke does not move the baseline (F8:
        // `yes please` must not route by how fast it was typed)...
        input_model.update(&mut app, |model, ctx| {
            model.apply_autodetected_input_type(InputType::AI, ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_type(), InputType::AI);
            assert_eq!(model.non_autodetected_input_type, InputType::Shell);
        });

        // ...but a choice the user made does.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_type(InputType::AI, ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.non_autodetected_input_type, InputType::AI);
        });
    });
}

// ---------------------------------------------------------------------------------------------
// #833 F6: `settle_detection_synchronously`, the Enter-time evaluation of a pending detection.
// ---------------------------------------------------------------------------------------------

fn undescribed(buffer_text: &str) -> ParsedTokensSnapshot {
    warp_completer::util::parse_current_commands_and_tokens_without_descriptions(
        buffer_text.to_owned(),
        &crate::completer::EmptyCompletionContext::new(),
    )
}

fn settle_synchronously(
    app: &mut App,
    input_model: &ModelHandle<BlocklistAIInputModel>,
    buffer_text: &str,
    described: Option<ParsedTokensSnapshot>,
    budget: std::time::Duration,
) -> SyncDetectionOutcome {
    input_model.update(app, |model, ctx| {
        model.settle_detection_synchronously(
            buffer_text,
            described,
            undescribed(buffer_text),
            crate::completer::EmptyCompletionContext::new(),
            None,
            budget,
            ctx,
        )
    })
}

const ENTER_BUDGET: std::time::Duration = std::time::Duration::from_millis(60);

#[test]
fn enter_settles_a_pending_buffer_by_its_own_verdict() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        // The previous buffer left the input in AI mode; a command was pasted and its detection
        // has not run yet (no parse either).
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested(PREFILTERED_COMMAND.to_owned());
        });

        let outcome = settle_synchronously(
            &mut app,
            &input_model,
            PREFILTERED_COMMAND,
            None,
            ENTER_BUDGET,
        );
        assert_eq!(outcome, SyncDetectionOutcome::Settled(InputType::Shell));
        input_model.read(&app, |model, ctx| {
            assert_eq!(model.input_type(), InputType::Shell);
            assert!(!model.is_detection_pending_for(PREFILTERED_COMMAND, ctx));
        });
    });
}

#[test]
fn enter_text_only_steps_settle_without_a_parse() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested("yes please".to_owned());
        });

        // The one-off shortcut needs only the buffer text.
        let outcome =
            settle_synchronously(&mut app, &input_model, "yes please", None, ENTER_BUDGET);
        assert_eq!(outcome, SyncDetectionOutcome::Settled(InputType::AI));
    });
}

#[test]
fn enter_without_descriptions_does_not_guess_with_the_classifier() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);
        let prose = "find the bug in main.rs";
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested(prose.to_owned());
        });

        // No matching parse: the classifier would read missing descriptions as missing command
        // evidence, so it is skipped and the input stays in the current mode.
        let outcome = settle_synchronously(&mut app, &input_model, prose, None, ENTER_BUDGET);
        assert_eq!(outcome, SyncDetectionOutcome::Undecided);
        input_model.read(&app, |model, ctx| {
            assert_eq!(model.input_type(), InputType::Shell);
            assert!(model.is_detection_pending_for(prose, ctx));
        });
    });
}

#[test]
fn enter_runs_the_classifier_on_a_matching_parse() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);
        let prose = "find the bug in main.rs";
        let described = parsed(prose).await;
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested(prose.to_owned());
        });

        // A generous budget: this asserts the path completes, not how fast.
        let outcome = settle_synchronously(
            &mut app,
            &input_model,
            prose,
            Some(described),
            std::time::Duration::from_secs(5),
        );
        assert!(
            matches!(outcome, SyncDetectionOutcome::Settled(_)),
            "unexpected outcome {outcome:?}"
        );
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_pending_for(prose, ctx));
        });
    });
}

#[test]
fn enter_budget_exhaustion_routes_in_the_current_mode() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);
        let prose = "find the bug in main.rs";
        let described = parsed(prose).await;
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested(prose.to_owned());
        });

        // With no budget the pipeline is abandoned at its first yield (before the classifier).
        let outcome = settle_synchronously(
            &mut app,
            &input_model,
            prose,
            Some(described),
            std::time::Duration::ZERO,
        );
        assert!(
            matches!(outcome, SyncDetectionOutcome::BudgetExhausted { .. }),
            "unexpected outcome {outcome:?}"
        );
        input_model.read(&app, |model, ctx| {
            assert_eq!(model.input_type(), InputType::Shell);
            assert!(model.is_detection_pending_for(prose, ctx));
        });
    });
}

#[test]
fn enter_never_reevaluates_locked_or_explicit_input() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);

        // Locked (ctrl-I): a command typed in locked-AI mode stays AI.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_config(AI_LOCKED, true, ctx);
            model.note_detection_requested(PREFILTERED_COMMAND.to_owned());
        });
        let outcome = settle_synchronously(
            &mut app,
            &input_model,
            PREFILTERED_COMMAND,
            None,
            ENTER_BUDGET,
        );
        assert_eq!(outcome, SyncDetectionOutcome::NotPending);
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_LOCKED);
        });

        // Unlocked explicit choice for this buffer: also left alone.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_config(AI_UNLOCKED, true, ctx);
            model.set_input_type(InputType::AI, ctx);
            model.note_detection_requested(PREFILTERED_COMMAND.to_owned());
        });
        let outcome = settle_synchronously(
            &mut app,
            &input_model,
            PREFILTERED_COMMAND,
            None,
            ENTER_BUDGET,
        );
        assert_eq!(outcome, SyncDetectionOutcome::NotPending);
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_type(), InputType::AI);
        });
    });
}
