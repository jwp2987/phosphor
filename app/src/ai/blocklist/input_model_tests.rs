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
//! Two of the five blocked #313 pin tests are not ported here:
//! `conversation_events_with_inert_policy_leave_config_unchanged` and
//! `conversation_events_apply_policy_updates`. Both exercise config transitions
//! driven by `ConversationSelectionEvent`, which the fork's
//! `BlocklistAIInputModel` does not yet subscribe to — see `GuiInputModePolicy`'s
//! doc comment for why (the GUI has no `ConversationSelection` implementation
//! yet, only the TUI's `TuiConversationSelection` does). Porting that wiring is
//! a distinct, larger feature gap than #313, which is scoped to lock gating /
//! initial config / AI-settings transitions.

use std::rc::Rc;
use std::sync::Arc;

use chrono::Duration;
use parking_lot::FairMutex;
use settings::Setting as _;
use warpui::r#async::executor::Background;
use warpui::{App, AppContext, EntityId, ModelHandle, SingletonEntity};

use super::*;
use crate::ai::blocklist::conversation_selection::ConversationSelectionEvent;
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
    on_settings_changed: Option<InputConfig>,
}

impl StubPolicy {
    /// A policy with `initial` config that permits locked AI and never reacts
    /// to AI-settings events.
    fn inert(initial: InputConfig) -> Self {
        Self {
            initial,
            allows_locked_ai: true,
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
        false
    }

    fn config_on_conversation_selection_changed(
        &self,
        _event: &ConversationSelectionEvent,
        _current: InputConfig,
        _app: &AppContext,
    ) -> Option<PolicyConfigUpdate> {
        None
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

/// Builds a TUI-surface input model driven by `policy`. Uses `new_tui` (no
/// `AgentViewController` fixture needed) since the policy seam under test —
/// `new_inner`'s initial config, `set_input_config_internal`'s lock gate, and
/// the AI-settings subscription — is shared by both surfaces.
fn build_input_model(app: &mut App, policy: StubPolicy) -> ModelHandle<BlocklistAIInputModel> {
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
            conversation_selection,
        )
    });
    app.add_model(|ctx| {
        BlocklistAIInputModel::new_tui(
            terminal_model,
            ai_context_model,
            Rc::new(policy),
            terminal_view_id,
            ctx,
        )
    })
}

#[test]
fn initial_config_comes_from_policy() {
    App::test((), |mut app| async move {
        // A locked-AI initial config sticks — no hardcoded GUI/TUI gating overrides it.
        let input_model = build_input_model(&mut app, StubPolicy::inert(AI_LOCKED));
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
        let input_model = build_input_model(&mut app, policy);

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
fn settings_change_applies_policy_update() {
    App::test((), |mut app| async move {
        let policy = StubPolicy {
            on_settings_changed: Some(SHELL_LOCKED),
            ..StubPolicy::inert(AI_LOCKED)
        };
        let input_model = build_input_model(&mut app, policy);

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
