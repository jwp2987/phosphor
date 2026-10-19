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

/// A history entry for `command` that exited with `exit_code` (`None`: a line imported from the
/// shell's history file that this app never ran, so no exit code and no timestamp).
fn history_entry(command: &str, exit_code: Option<i32>) -> HistoryEntry {
    let mut entry = HistoryEntry::command_only(command);
    if let Some(code) = exit_code {
        entry.exit_code = Some(warp_core::command::ExitCode::from(code));
        entry.start_ts = Some(Local::now());
    }
    entry
}

/// The command-history half of `detect_and_set_input_type`, end to end: filter the entries
/// (`shell_history_candidates`), match the buffer newest-first at the production cutoff, and
/// resolve with no prompt-history match. `None` means history had no say and the buffer goes on
/// to the classifier.
fn command_history_verdict(entries: &[HistoryEntry], buffer: &str) -> Option<InputType> {
    let candidates = shell_history_candidates(entries.iter(), buffer);
    let command_match = futures::executor::block_on(most_recent_close_match(
        buffer,
        candidates
            .iter()
            .rev()
            .map(|(command, start_ts)| (command.as_str(), *start_ts)),
        HISTORY_ENTRY_MATCH_CUTOFF,
    ));
    resolve_history_match(command_match, HistoryMatch::NoMatch).map(|(input_type, _)| input_type)
}

#[test]
fn mis_run_prose_in_history_does_not_force_shell() {
    // #832: the sentence ran in the shell once; `find` exited 1, not 127, so it was kept. It and
    // its near-variants used to match at the 0.9 cutoff and route to the shell forever.
    let history = [
        history_entry("git status", Some(0)),
        history_entry("find the bug in main.rs", Some(1)),
    ];
    for buffer in [
        "find the bug in main.rs",
        "find the bug in main.py",
        "find the bugs in main.rs",
    ] {
        assert_eq!(
            command_history_verdict(&history, buffer),
            None,
            "{buffer:?} must be left to the classifier",
        );
    }
}

#[test]
fn prose_from_the_history_file_or_interrupted_does_not_force_shell() {
    // No exit code: a line read from `.bash_history` that this app never ran. That is how one
    // mistake persists across restarts, so it is not evidence of a command either.
    let histfile = [history_entry("make the tests pass please", None)];
    assert_eq!(
        command_history_verdict(&histfile, "make the tests pass please"),
        None
    );
    // ctrl-c (130) counts as "successful" for block decorations, but interrupting a sentence
    // that hung proves nothing about it.
    let interrupted = [history_entry("yes please", Some(130))];
    assert_eq!(command_history_verdict(&interrupted, "yes please"), None);
}

#[test]
fn re_typed_commands_still_route_to_shell_from_history() {
    let history = [
        // A real command that failed last time.
        history_entry("cargo test --workspace", Some(101)),
        // A real command from the shell's history file (no exit code).
        history_entry("ssh deploy@build-box", None),
        // A prose-shaped line that genuinely ran and succeeded.
        history_entry("echo is it the cache", Some(0)),
    ];
    for buffer in [
        "cargo test --workspace",
        // Near-variants still match.
        "cargo test --workspaces",
        "ssh deploy@build-box",
        "ssh deploy@build-box2",
        "echo is it the cache",
    ] {
        assert_eq!(
            command_history_verdict(&history, buffer),
            Some(InputType::Shell),
            "{buffer:?} must still route to Shell from history",
        );
    }
}

#[test]
fn command_not_found_entries_never_match() {
    let history = [history_entry("carg build", Some(127))];
    assert_eq!(command_history_verdict(&history, "carg build"), None);
    // 9009 is CMD's "not recognized as an internal or external command".
    let history = [history_entry("carg build", Some(9009))];
    assert_eq!(command_history_verdict(&history, "carg build"), None);
}

#[test]
fn shell_syntax_with_function_words_needs_an_exit_0_entry() {
    // Pinned trade-off: these read as prose, so only an entry that exited 0 gives them the
    // history shortcut; a failed or history-file entry leaves them to the classifier.
    for command in [
        "for i in $(seq 3); do echo $i; done",
        "if true; then echo y; fi",
        "scp f.txt host:",
        "grep -r for src",
    ] {
        for exit_code in [Some(1), None] {
            let history = [history_entry(command, exit_code)];
            assert_eq!(
                command_history_verdict(&history, command),
                None,
                "{command:?} with exit code {exit_code:?} must be left to the classifier",
            );
        }
        let history = [history_entry(command, Some(0))];
        assert_eq!(
            command_history_verdict(&history, command),
            Some(InputType::Shell),
            "{command:?} that exited 0 must still route to Shell",
        );
    }
}

#[test]
fn prose_needs_an_exact_exit_0_entry_not_a_near_variant() {
    // A prose-shaped line that genuinely ran still re-runs from history, but only verbatim
    // (whitespace runs aside): its near-variants go to the classifier instead of riding on it.
    let history = [history_entry("echo is it the cache", Some(0))];
    assert_eq!(
        command_history_verdict(&history, "echo  is it the cache "),
        Some(InputType::Shell),
    );
    assert_eq!(
        command_history_verdict(&history, "echo is it the caches"),
        None
    );
    // Same for a sentence one word away from an exit-0 entry: `main.py` once ran fine, the
    // `main.rs` buffer is left to the classifier.
    let history = [history_entry("echo find the bug in main.py", Some(0))];
    assert_eq!(
        command_history_verdict(&history, "echo find the bug in main.rs"),
        None
    );
}

#[test]
fn prose_matches_an_older_exit_0_entry_past_a_newer_failure() {
    // Entries are filtered one by one, before matching: the newest entry's failure removes only
    // that entry, so an older exit-0 entry with the same text is still a candidate and wins
    // (Shell) here. In production the case does not arise: `History::commands` dedupes identical
    // text down to its newest run (and the history-file join keeps the latest run's exit code),
    // so the verdict there follows the newest run -- a failure, so the classifier decides.
    let history = [
        history_entry("echo find the bug in main.rs", Some(0)),
        history_entry("echo find the bug in main.rs", Some(1)),
    ];
    assert_eq!(
        command_history_verdict(&history, "echo find the bug in main.rs"),
        Some(InputType::Shell),
    );
    // With only the failure in history, the same buffer is left to the classifier.
    assert_eq!(
        command_history_verdict(&history[1..], "echo find the bug in main.rs"),
        None,
    );
}

#[test]
fn failed_article_prose_does_not_force_shell() {
    // `make a PR` reads as prose through the #831 article veto, shared with the pre-filter.
    let history = [history_entry("make a PR", Some(2))];
    assert_eq!(command_history_verdict(&history, "make a PR"), None);
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
// #833: input-type detection is tracked per buffer.
//
// The view records every user edit that requests detection (`note_detection_requested`) and only
// classifies a parse whose text is still requested (`is_detection_requested_for`). These tests pin
// the model half of that contract, the narrowed explicit-set window (F7), the withdrawal of
// submitted text from detection, and the typing-speed-independent one-off shortcut (F8). F6
// (Enter routing before the verdict lands) is not fixed.
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
fn submit_withdraws_the_submitted_text_from_detection() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let _agent_view = FeatureFlag::AgentView.override_enabled(false);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_autodetection_enabled_internal
                .set_value(true, ctx)
                .unwrap();
        });

        // Typed, then submitted before its parse landed: the late parse must not be
        // classified (the view checks `is_detection_requested_for` before classifying).
        input_model.update(&mut app, |model, ctx| {
            model.note_detection_requested(PREFILTERED_COMMAND.to_owned());
            model.handle_input_buffer_submitted(ctx);
        });
        input_model.read(&app, |model, ctx| {
            assert!(!model.is_detection_requested_for(PREFILTERED_COMMAND));
            assert!(!model.is_detection_pending_for(PREFILTERED_COMMAND, ctx));
            assert_eq!(model.input_config(), AI_UNLOCKED);
        });
    });
}

#[test]
fn explicit_lock_is_never_pending() {
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
// #886: Enter settles a pending detection Shell-ward from the whole text, and shell typeahead
// defaults to Shell.
//
// The burst torture (`echo bN >> ~/blk.txt` + Return, ~1 ms per key) never let the verdict for a
// line land before its Enter, so after one agent submit the inherited `AI` routed whole runs of
// complete commands to the agent. The buffers below are the ones the agent received, and the
// split/merged shapes the shell handed back as typeahead, taken from the bisect logs.
// ---------------------------------------------------------------------------------------------

/// Complete (or merged-but-headed) lines from the #886 runs that were sent to the agent.
const BURST_LINES_SENT_TO_AGENT: &[&str] = &[
    "echo b30 >> ~/blk.txt",
    "echo b134 >> ~/blk.txt",
    "echo b196 >> ~/blk.txtecho b199 >> ~/blk.txt",
    "echo b129 b131 >> ~/blk.txt",
];

/// Headless typeahead pieces from the #886 runs: no head the pre-filter recognises.
const TYPEAHEAD_FRAGMENTS: &[&str] = &[
    "ho b199 >> ~/blk.txt",
    "b22 b23",
    "b11 blk.txtecho b12",
    "xtecho b5 >> ~/blk.txt",
    "b29 >> ~/blk.txt",
];

fn snapshot_without_parse(buffer_text: &str) -> ParsedTokensSnapshot {
    // Enter does not wait for the debounced parse; the settle works from the text alone.
    ParsedTokensSnapshot {
        buffer_text: buffer_text.to_owned(),
        parsed_tokens: Vec::new(),
    }
}

fn settle_for_submit(
    app: &mut App,
    input_model: &ModelHandle<BlocklistAIInputModel>,
    buffer_text: &str,
) -> bool {
    input_model.update(app, |model, ctx| {
        model.settle_pending_detection_for_submit(&snapshot_without_parse(buffer_text), ctx)
    })
}

/// Puts the model back to "`AI` inherited from an earlier verdict", with nothing pending, the
/// state an agent submit leaves for the next command typed into the same input.
fn inherit_ai(app: &mut App, input_model: &ModelHandle<BlocklistAIInputModel>) {
    input_model.update(app, |model, ctx| {
        model.apply_autodetected_input_type(InputType::AI, ctx);
        model.note_buffer_edited("");
    });
}

#[test]
fn pending_burst_line_settles_to_shell_at_submit() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        for line in BURST_LINES_SENT_TO_AGENT {
            // The line's own verdict has not landed when its Enter arrives.
            inherit_ai(&mut app, &input_model);
            input_model.update(&mut app, |model, _| {
                model.note_detection_requested((*line).to_owned());
            });
            assert!(settle_for_submit(&mut app, &input_model, line), "{line}");
            input_model.read(&app, |model, ctx| {
                assert_eq!(model.input_type(), InputType::Shell, "{line}");
                assert!(!model.is_detection_pending_for(line, ctx), "{line}");
                // A late parse of the line is no longer requested, so the view drops it.
                assert!(!model.is_detection_requested_for(line), "{line}");
            });
        }
    });
}

#[test]
fn pending_prose_with_shell_head_keeps_inherited_type_at_submit() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        // #831's holes: prose that starts with a command word must never be settled to Shell at
        // Enter. Only the Shell-ward pre-filter runs here, so the inherited `AI` stands.
        for prose in [
            "find the bug in main.rs",
            "git status of the repo",
            "cat the config file and summarize it",
            "ls files > list.txt please",
        ] {
            inherit_ai(&mut app, &input_model);
            input_model.update(&mut app, |model, _| {
                model.note_detection_requested(prose.to_owned());
            });
            assert!(!settle_for_submit(&mut app, &input_model, prose), "{prose}");
            input_model.read(&app, |model, ctx| {
                assert_eq!(model.input_type(), InputType::AI, "{prose}");
                assert!(model.is_detection_pending_for(prose, ctx), "{prose}");
            });
        }
    });
}

#[test]
fn settled_or_locked_type_is_not_overridden_at_submit() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let line = BURST_LINES_SENT_TO_AGENT[0];
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);

        // Nothing requested a detection for this text (no edit since its verdict): the live type
        // is its verdict and is left alone.
        assert!(!settle_for_submit(&mut app, &input_model, line));
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_type(), InputType::AI);
        });

        // ctrl-I: the user locked AI, so even an unambiguous command goes to the agent.
        input_model.update(&mut app, |model, ctx| {
            model.set_input_config(AI_LOCKED, true, ctx);
            model.note_detection_requested(line.to_owned());
        });
        assert!(!settle_for_submit(&mut app, &input_model, line));
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_config(), AI_LOCKED);
        });
    });
}

/// #833 F6, agent-ward: prose pasted or typed fast right after a shell command inherits `Shell`;
/// Enter must move it to the agent when it reads as a request, and leave real commands alone.
#[test]
fn pending_prose_inheriting_shell_settles_to_ai_at_submit() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, SHELL_UNLOCKED);
        for prose in [
            "help me",
            "watch the logs for errors",
            "docker vs podman?",
            "why is the build slow",
            "can you fix this",
        ] {
            input_model.update(&mut app, |model, ctx| {
                model.apply_autodetected_input_type(InputType::Shell, ctx);
                model.note_detection_requested(prose.to_owned());
            });
            // Only the Shell-ward settle reports `true`.
            assert!(!settle_for_submit(&mut app, &input_model, prose), "{prose}");
            input_model.read(&app, |model, ctx| {
                assert_eq!(model.input_type(), InputType::AI, "{prose}");
                assert!(!model.is_detection_pending_for(prose, ctx), "{prose}");
            });
        }
        for kept in [
            "ls -la",
            "echo this and that",
            "cargo build",
            "ls -la | grep to",
            "grep -r for src",
            "kubectl get pods -n the",
            "cd my",
            "ls me",
            "cd x && if test -f y; then echo z; fi",
            "ls file?",
            "rm -rf tmp?",
            "echo is it ok?",
            "sudo apt install it",
            "grep For file",
            "cat <<EOF\nto is and\nEOF",
            "git commit -m \"fix the bug\"",
        ] {
            input_model.update(&mut app, |model, ctx| {
                model.apply_autodetected_input_type(InputType::Shell, ctx);
                model.note_detection_requested(kept.to_owned());
            });
            settle_for_submit(&mut app, &input_model, kept);
            input_model.read(&app, |model, _| {
                assert_eq!(model.input_type(), InputType::Shell, "{kept}");
            });
        }
    });
}

/// Questions a person might type while a command runs, which reach the editor as typeahead. The
/// provenance must not decide any of them: they go through the normal pipeline.
const TYPEAHEAD_PROSE: &[&str] = &[
    "exit vim",
    "rm the temp files",
    "mv files to backup",
    "rm -rf node_modules and reinstall",
    "kill server process",
    "echo this is prose",
    "cargo build fails why",
];

#[test]
fn typeahead_fragments_default_to_shell() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        for fragment in TYPEAHEAD_FRAGMENTS {
            // At Enter, before any verdict.
            inherit_ai(&mut app, &input_model);
            input_model.update(&mut app, |model, _| {
                model.note_typeahead_inserted((*fragment).to_owned());
                model.note_detection_requested((*fragment).to_owned());
            });
            assert!(
                settle_for_submit(&mut app, &input_model, fragment),
                "{fragment}"
            );
            input_model.read(&app, |model, _| {
                assert_eq!(model.input_type(), InputType::Shell, "{fragment}");
            });

            // And in detection: settled synchronously, the classifier never sees it.
            inherit_ai(&mut app, &input_model);
            let snapshot = parsed(fragment).await;
            input_model.update(&mut app, |model, _| {
                model.note_typeahead_inserted((*fragment).to_owned())
            });
            request_detection(&mut app, &input_model, snapshot);
            input_model.read(&app, |model, ctx| {
                assert_eq!(model.input_type(), InputType::Shell, "{fragment}");
                assert!(!model.is_detection_pending_for(fragment, ctx), "{fragment}");
            });
        }
    });
}

#[test]
fn typeahead_prose_is_not_forced_to_shell() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        for prose in TYPEAHEAD_PROSE {
            // At Enter: not settled, the inherited type stands.
            inherit_ai(&mut app, &input_model);
            input_model.update(&mut app, |model, _| {
                model.note_typeahead_inserted((*prose).to_owned());
                model.note_detection_requested((*prose).to_owned());
            });
            assert!(!settle_for_submit(&mut app, &input_model, prose), "{prose}");
            input_model.read(&app, |model, ctx| {
                assert_eq!(model.input_type(), InputType::AI, "{prose}");
                assert!(model.is_detection_pending_for(prose, ctx), "{prose}");
                // The provenance is kept (the buffer still starts with it); it just does not
                // decide prose.
                assert!(model.typeahead_prefix.is_some(), "{prose}");
            });
        }
    });
}

#[test]
fn typeahead_text_extended_with_prose_is_not_forced_to_shell() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);
        // The whole buffer is checked, not just the typeahead at its front.
        let buffer = "b22 b23 what does this do";
        input_model.update(&mut app, |model, _| {
            model.note_typeahead_inserted("b22 b23".to_owned());
            model.note_detection_requested(buffer.to_owned());
        });
        assert!(!settle_for_submit(&mut app, &input_model, buffer));
        input_model.read(&app, |model, _| {
            assert_eq!(model.input_type(), InputType::AI);
        });
    });
}

#[test]
fn typeahead_provenance_ends_with_its_buffer() {
    App::test((), |mut app| async move {
        let _agent_mode = FeatureFlag::AgentMode.override_enabled(true);
        let _agent_view = FeatureFlag::AgentView.override_enabled(false);
        let fragment = TYPEAHEAD_FRAGMENTS[1];
        let input_model = build_autodetecting_input_model(&mut app, AI_UNLOCKED);

        // Without typeahead the fragment is not decided at Enter (it has no command head).
        input_model.update(&mut app, |model, _| {
            model.note_detection_requested(fragment.to_owned());
        });
        assert!(!settle_for_submit(&mut app, &input_model, fragment));

        // Typing after the typeahead, or choosing Shell, keeps the provenance...
        input_model.update(&mut app, |model, ctx| {
            model.note_typeahead_inserted(fragment.to_owned());
            model.note_detection_requested(format!("{fragment} b24"));
            model.set_input_type(InputType::Shell, ctx);
        });
        input_model.read(&app, |model, _| {
            assert_eq!(model.typeahead_prefix.as_deref(), Some(fragment))
        });

        // ...a select-all and paste that replaces it drops it, so the pasted text is classified
        // as the user's own...
        input_model.update(&mut app, |model, _| {
            model.note_typeahead_inserted(fragment.to_owned());
            model.note_detection_requested("b9 b10".to_owned());
        });
        input_model.read(&app, |model, _| assert!(model.typeahead_prefix.is_none()));
        // ...and choosing AI, emptying the buffer, or submitting it each end it.
        let ends: [fn(&mut BlocklistAIInputModel, &mut ModelContext<BlocklistAIInputModel>); 3] = [
            |model, ctx| model.set_input_type(InputType::AI, ctx),
            |model, _| model.note_buffer_edited(""),
            |model, ctx| model.handle_input_buffer_submitted(ctx),
        ];
        for end in ends {
            input_model.update(&mut app, |model, ctx| {
                model.note_typeahead_inserted(fragment.to_owned());
                end(model, ctx);
            });
            input_model.read(&app, |model, _| assert!(model.typeahead_prefix.is_none()));
        }

        // Whitespace-only typeahead is no provenance.
        input_model.update(&mut app, |model, _| {
            model.note_typeahead_inserted("  ".to_owned())
        });
        input_model.read(&app, |model, _| assert!(model.typeahead_prefix.is_none()));
    });
}
