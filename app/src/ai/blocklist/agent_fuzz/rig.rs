//! The L1 rig: a real terminal view, controller, action model and queue in `App::test`, with the
//! BYOP provider pointed at a model id whose replies come from the stream seam.
//!
//! Everything the user does goes through the entry points the UI uses (see `actor.rs`); this file
//! only builds the app, observes it, and lets time pass.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use warpui::{App, ModelHandle, SingletonEntity, ViewHandle};

use crate::ai::agent::AIAgentActionId;
use crate::ai::agent::AIAgentActionType;
use crate::ai::agent::conversation::{AIConversationId, ConversationStatus};
use crate::ai::agent_providers::fake_stream_seam::{self, SeamHandle};
use crate::ai::agent_providers::mock_wiring::wire_mock_provider_config_with_model;
use crate::ai::blocklist::action_model::{PendingActionSnapshot, PendingStage};
use crate::ai::blocklist::controller::BlocklistAIController;
use crate::ai::blocklist::{
    BlocklistAIActionModel, BlocklistAIHistoryModel, QueuedQueryModel, QueuedQueryOrigin,
};
use crate::ai::llms::LLMPreferences;
use crate::terminal::TerminalView;
use crate::test_util::terminal::{add_window_with_terminal, initialize_app_for_terminal_view};

use super::env::{CmdOutcome, command_key, outcome};
use super::player::Player;
use fake_llm::scenario::Scenario;

static NEXT_MODEL: AtomicU64 = AtomicU64::new(0);

pub struct Rig {
    pub terminal: ViewHandle<TerminalView>,
    pub controller: ModelHandle<BlocklistAIController>,
    pub action_model: ModelHandle<BlocklistAIActionModel>,
    pub conversation_id: AIConversationId,
    pub player: Player,
    pub seam: SeamHandle,
    /// Action id -> the scenario command key its `run_shell_command` named.
    pub cmd_keys: HashMap<AIAgentActionId, String>,
    /// When each running command was first seen, for the stall-snapshot stand-in.
    running_since: HashMap<AIAgentActionId, Instant>,
    /// A command that never ends has been reported to the agent (as the executor's stall
    /// snapshot does) and is still alive in the terminal: a hand-over has something to hand over.
    pub lrc_alive: bool,
}

/// One queued row, as the oracle sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub text: String,
    pub origin: QueuedQueryOrigin,
}

/// Everything the oracle reads, taken at one instant.
pub struct Obs {
    pub status: Option<ConversationStatus>,
    pub stream_in_flight: bool,
    pub snapshot: Vec<PendingActionSnapshot>,
    pub cards: Vec<AIAgentActionId>,
    /// Actions whose card would take accept keys now (the card's own key routing).
    pub routed: Vec<AIAgentActionId>,
    pub rows: Vec<Row>,
}

impl Obs {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            Some(
                ConversationStatus::Success
                    | ConversationStatus::Error
                    | ConversationStatus::Cancelled
            )
        )
    }

    pub fn running(&self) -> bool {
        self.snapshot
            .iter()
            .any(|s| s.stage == PendingStage::Running)
    }

    pub fn nothing_held(&self) -> bool {
        self.snapshot.is_empty()
    }
}

/// Wires the app and sends nothing yet.
pub fn setup(app: &mut App, scenario: &Scenario, safe_allowlist: bool) -> Rig {
    initialize_app_for_terminal_view(app);
    // The agent turn persists conversation state through this provider.
    crate::test_util::settings::initialize_history_persistence_for_tests(app);
    // The rig has no SQLite writer, so conversations are not persisted (the same "never persisted
    // by design" case the BYOP preflight already supports); otherwise it refuses with
    // ByopPreflightPersistenceUnavailable and logs an ERROR.
    app.update(|ctx| {
        use settings::Setting as _;
        crate::terminal::general_settings::GeneralSettings::handle(ctx).update(
            ctx,
            |general, ctx| {
                let _ = general.persist_conversations.set_value(false, ctx);
            },
        );
    });
    if safe_allowlist {
        enable_safe_allowlist(app);
    }
    let terminal = add_window_with_terminal(app, None);
    let terminal_id = terminal.read(app, |view, _| view.id());

    let model_id = format!("fuzz-model-{}", NEXT_MODEL.fetch_add(1, Ordering::SeqCst));
    let llm_id = app.update(|ctx| {
        wire_mock_provider_config_with_model(ctx, "http://127.0.0.1:9/v1", &model_id)
    });
    app.update(|ctx| {
        LLMPreferences::handle(ctx).update(ctx, |prefs, ctx| {
            prefs.update_preferred_agent_mode_llm(&llm_id, terminal_id, ctx);
        });
    });

    let player = Player::new(scenario.clone());
    let script_player = player.clone();
    let seam = fake_stream_seam::install(&model_id, move |request| script_player.answer(request));

    let controller = terminal.read(app, |view, _| view.ai_controller().clone());
    let action_model = controller.read(app, |controller, _| controller.action_model_for_test());
    let conversation_id = BlocklistAIHistoryModel::handle(app).update(app, |history, ctx| {
        history.start_new_conversation(terminal_id, false, false, ctx)
    });

    Rig {
        terminal,
        controller,
        action_model,
        conversation_id,
        player,
        seam,
        cmd_keys: HashMap::new(),
        running_since: HashMap::new(),
        lrc_alive: false,
    }
}

/// Turns the safe read-only allowlist on, adding `echo` to the defaults: `Args::Valid`'s
/// `echo hi` is then a listed line (runs without a card) while the scenario's `fake-cmd cN` lines
/// are not (they still ask), so both paths are exercised.
fn enable_safe_allowlist(app: &mut App) {
    use settings::Setting as _;
    app.update(|ctx| {
        crate::settings::AISettings::handle(ctx).update(ctx, |settings, ctx| {
            let mut entries = crate::ai::blocklist::safe_commands::default_safe_commands();
            entries.push("echo".to_owned());
            let _ = settings.safe_command_allowlist.set_value(entries, ctx);
            let _ = settings.safe_command_allowlist_enabled.set_value(true, ctx);
        });
    });
}

impl Rig {
    /// The user submits `text` in the conversation.
    pub fn send(&self, app: &mut App, text: &str) {
        let conversation_id = self.conversation_id;
        let text = text.to_owned();
        self.controller.update(app, |controller, ctx| {
            controller.send_user_query_in_conversation(text, conversation_id, None, ctx);
        });
    }

    pub fn observe(&self, app: &App) -> Obs {
        let conversation_id = self.conversation_id;
        app.read(|ctx| {
            let status = BlocklistAIHistoryModel::as_ref(ctx)
                .conversation(&conversation_id)
                .map(|conversation| conversation.status().clone());
            let stream_in_flight = self
                .controller
                .as_ref(ctx)
                .has_active_stream_for_conversation(conversation_id, ctx);
            let snapshot = self
                .action_model
                .as_ref(ctx)
                .pending_snapshot_for_test()
                .into_iter()
                .filter(|s| s.conversation_id == conversation_id)
                .collect();
            let cards = self.terminal.as_ref(ctx).actionable_card_ids_for_test(ctx);
            let routed = self
                .terminal
                .as_ref(ctx)
                .key_routed_card_actions_for_test(ctx);
            let rows = QueuedQueryModel::as_ref(ctx)
                .queue(conversation_id)
                .iter()
                .map(|row| Row {
                    text: row.text().to_owned(),
                    origin: row.origin(),
                })
                .collect();
            Obs {
                status,
                stream_in_flight,
                snapshot,
                cards,
                routed,
                rows,
            }
        })
    }

    /// The front pending action, with its type.
    pub fn front_action(&self, app: &App) -> Option<crate::ai::agent::AIAgentAction> {
        let conversation_id = self.conversation_id;
        app.read(|ctx| {
            self.action_model
                .as_ref(ctx)
                .get_pending_actions_for_conversation(&conversation_id)
                .next()
                .cloned()
        })
    }

    /// Remembers which scenario command each pending command action names, while it is still
    /// pending (a running action no longer carries its type).
    pub fn note_commands(&mut self, app: &App) {
        let conversation_id = self.conversation_id;
        let pending: Vec<_> = app.read(|ctx| {
            self.action_model
                .as_ref(ctx)
                .get_pending_actions_for_conversation(&conversation_id)
                .cloned()
                .collect()
        });
        for action in pending {
            if let AIAgentActionType::RequestCommandOutput { command, .. } = &action.action {
                if let Some(key) = command_key(command) {
                    self.cmd_keys.insert(action.id.clone(), key.to_owned());
                }
            }
        }
    }

    /// Finishes the running commands whose behaviour ends them, and reports the ones that never
    /// end to the agent after a short wait (what the executor's stall snapshot does), leaving
    /// them alive.
    pub fn drive_commands(&mut self, app: &mut App, stall_after: Duration) {
        let obs = self.observe(app);
        let running: Vec<AIAgentActionId> = obs
            .snapshot
            .iter()
            .filter(|s| s.stage == PendingStage::Running)
            .map(|s| s.action_id.clone())
            .collect();
        self.running_since.retain(|id, _| running.contains(id));
        let executor = self
            .action_model
            .read(app, |model, ctx| model.shell_command_executor(ctx));
        for id in running {
            // Only commands the executor is waiting on are ours to finish; a read or a grep
            // finishes by itself.
            let waiting = executor.read(app, |executor, _| {
                executor.is_waiting_on_command_for_test(&id)
            });
            if !waiting {
                continue;
            }
            // An unnamed command (`echo hi`) is a quick one.
            let key = self.cmd_keys.get(&id).cloned().unwrap_or_default();
            let behaviour = self.player.scenario().env.commands.get(&key).cloned();
            let plan = behaviour
                .as_ref()
                .map(outcome)
                .unwrap_or(CmdOutcome::Completes {
                    output: String::new(),
                    exit_code: 0,
                });
            let since = *self
                .running_since
                .entry(id.clone())
                .or_insert_with(Instant::now);
            let (output, exit_code, due) = match &plan {
                CmdOutcome::Completes { output, exit_code } => (output.clone(), *exit_code, true),
                CmdOutcome::CompletesAfter {
                    after_ms,
                    output,
                    exit_code,
                } => (
                    output.clone(),
                    *exit_code,
                    since.elapsed() >= Duration::from_millis(*after_ms),
                ),
                CmdOutcome::Stays => (String::new(), 0, since.elapsed() >= stall_after),
            };
            if !due {
                continue;
            }
            if plan == CmdOutcome::Stays {
                self.lrc_alive = true;
            }
            executor.update(app, |executor, _| {
                executor.complete_command_for_test(&id, &output, exit_code);
            });
        }
    }
}

/// Lets the app run for up to `budget`, returning early once `done` holds. Yields to the
/// foreground executor between looks and sleeps a couple of milliseconds so timers and
/// background tasks advance.
pub async fn settle_until(
    app: &mut App,
    budget: Duration,
    mut done: impl FnMut(&mut App) -> bool,
) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        for _ in 0..16 {
            futures_lite::future::yield_now().await;
        }
        if done(app) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        warpui::r#async::Timer::after(Duration::from_millis(2)).await;
    }
}

/// Lets the app run for exactly `span`.
pub async fn pump_for(app: &mut App, span: Duration) {
    let _ = settle_until(app, span, |_| false).await;
}
