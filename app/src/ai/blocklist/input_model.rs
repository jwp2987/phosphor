//! Model-layer AI input state management logic.
//!
//! The primary export of this module is `BlocklistAIInputModel`, which is a terminal pane-scoped
//! model managing input "type" state (whether the input is in AI or shell mode). This model also
//! exposes methods for running query autodetection, where an algorithm determines if the current
//! input contents are an AI query or shell command, which is then used to update the input mode.

use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use chrono::{DateTime, Local};
use futures::stream::AbortHandle;
use input_classifier::util::routing::{
    self as routing, Decision, Route, RoutingContext, ShellFamily, Surface as RoutingSurface,
    Typeahead as RoutingTypeahead,
};
use input_classifier::util::{is_agent_follow_up_input, is_one_off_natural_language_word};
use instant::Instant;
use parking_lot::FairMutex;
use serde::{Deserialize, Serialize};
use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warpui::{AppContext, Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

pub use input_classifier::InputType;

use super::agent_view::{AgentViewController, AgentViewControllerEvent, AgentViewEntryOrigin};
use super::compute_clock::ComputeClock;
use super::context_model::BlocklistAIContextModel;
use super::conversation_selection::ConversationSelectionEvent;
use super::history_model::BlocklistAIHistoryModel;
use super::misroute_recovery::{MisrouteMemory, MisrouteOffer, RouteReason, SubmitRoute};
#[cfg(any(test, feature = "test-util"))]
use super::conversation_selection::{ConversationSelection, MockConversationSelection};
use super::input_mode_policy::{InputModePolicy, InputModePolicyHandle, PolicyConfigUpdate};
use crate::terminal::cli_agent_sessions::{
    CLIAgentInputState, CLIAgentSessionsModel, CLIAgentSessionsModelEvent,
};
use warp_completer::completer::CompletionContext;
use warp_terminal::shell::ShellType;

use crate::{
    input_classifier::InputClassifierModel,
    report_if_error, send_telemetry_from_ctx,
    settings::{AISettings, AISettingsChangedEvent, InputBoxType, InputSettings},
    terminal::{
        history::RanBefore,
        input::decorations::ParsedTokensSnapshot,
        model::{rich_content::RichContentType, session::SessionId},
        History, HistoryEntry, TerminalModel,
    },
    TelemetryEvent,
};

/// Cutoff score for deciding an user input matches a history command entry.
const HISTORY_ENTRY_MATCH_CUTOFF: f32 = 0.9;

/// Duration to temporarily disable autodetection during operations like history selection.
const AUTODETECTION_DISABLE_DURATION_MS: u64 = 250;

/// An explicit (non-autodetected) write of the input type, which suppresses autodetection for
/// [`AUTODETECTION_DISABLE_DURATION_MS`] -- but only for the buffer it was made for (#833).
///
/// Upstream suppresses *every* detection inside the window. That swallowed content the user
/// produced right after the explicit write: prose pasted within 250 ms of submitting a command
/// stayed in shell mode and Enter ran it in the shell. The window exists so the explicit choice
/// is not immediately reclassified, and that choice was made for one buffer; new text is a new
/// question.
#[derive(Clone, Debug)]
struct ExplicitInputTypeSet {
    at: Instant,
    /// The buffer text the explicit choice applies to. `None` until known: most explicit writers
    /// do not know the buffer, so the first detection request inside the window (the echo of the
    /// edit that accompanied the explicit write) claims it.
    buffer_text: Option<String>,
}

impl ExplicitInputTypeSet {
    /// Whether the explicit choice still stands. Once a detection request has claimed it, it
    /// stands for exactly that buffer text and ends when a request for different text arrives (an
    /// event, see [`InputModel::is_detection_suppressed_by_explicit_set`]); no clock is involved
    /// (#1375, INP-A2). Only an UNCLAIMED write is time-bounded: nothing ties the edit that
    /// accompanies an explicit write to it except arriving right after, so
    /// [`AUTODETECTION_DISABLE_DURATION_MS`] is the give-up after which a request is a new
    /// question rather than the accompanying echo. That edit has no event of its own.
    fn is_within_window(&self) -> bool {
        self.buffer_text.is_some()
            || Instant::now()
                < self.at + std::time::Duration::from_millis(AUTODETECTION_DISABLE_DURATION_MS)
    }
}

/// Configuration for the terminal pane's input.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputConfig {
    /// The type of the terminal input.
    pub input_type: InputType,

    /// If `true`, we will not attempt to auto-detect the best input type.
    pub is_locked: bool,
}

impl InputConfig {
    /// Create a sensible default InputConfig based on user's auto-detection setting.
    pub fn new(app: &AppContext) -> Self {
        let ai_settings = AISettings::as_ref(app);
        let is_autodetection_enabled = ai_settings.is_ai_autodetection_enabled(app);

        InputConfig {
            input_type: InputType::Shell,
            is_locked: !is_autodetection_enabled, // Locked if auto-detection disabled
        }
    }

    pub fn with_toggled_type(self) -> Self {
        let input_type = if self.input_type.is_ai() {
            InputType::Shell
        } else {
            InputType::AI
        };
        Self { input_type, ..self }
    }

    pub fn with_shell_type(self) -> Self {
        Self {
            input_type: InputType::Shell,
            ..self
        }
    }

    pub fn with_input_type(self, input_type: InputType) -> Self {
        Self { input_type, ..self }
    }

    pub fn unlocked_if_autodetection_enabled(
        self,
        is_in_fullscreen_agent_view: bool,
        app: &AppContext,
    ) -> Self {
        Self {
            is_locked: if !FeatureFlag::AgentView.is_enabled() || is_in_fullscreen_agent_view {
                !AISettings::as_ref(app).is_ai_autodetection_enabled(app)
            } else {
                !AISettings::as_ref(app).is_nld_in_terminal_enabled(app)
            },
            ..self
        }
    }

    pub fn locked(self) -> Self {
        Self {
            is_locked: true,
            ..self
        }
    }

    pub fn is_ai(&self) -> bool {
        self.input_type == InputType::AI
    }

    pub fn is_shell(&self) -> bool {
        self.input_type == InputType::Shell
    }
}

/// Terminal pane-scoped model responsible for managing AI input state.
/// An `AtomicBool` that the model's `Clone` derive can copy: a clone starts with the flag's
/// current value instead of sharing it.
#[derive(Default)]
struct WakeFlag(AtomicBool);

impl Clone for WakeFlag {
    fn clone(&self) -> Self {
        Self(AtomicBool::new(self.0.load(AtomicOrdering::Relaxed)))
    }
}

impl std::ops::Deref for WakeFlag {
    type Target = AtomicBool;

    fn deref(&self) -> &AtomicBool {
        &self.0
    }
}

#[derive(Clone)]
pub struct BlocklistAIInputModel {
    input_config: InputConfig,

    /// The timestamp of the last time the input mode was switched, if the switch was to AI mode and
    /// it was autodetected. Else, `None`.
    last_ai_autodetection_ts: Option<Instant>,

    /// The last explicit write of the input type, see [`ExplicitInputTypeSet`].
    explicit_input_type_set: Option<ExplicitInputTypeSet>,

    /// The input type last written by anything *other than* autodetection (initial config,
    /// explicit toggles, submit resets, agent-view transitions). The one-off natural-language
    /// shortcut keys off this rather than the live input type, so the verdict for a buffer does
    /// not depend on what autodetection guessed for the intermediate states while it was typed
    /// (#833, F8: `yes please` routed to the agent when typed slowly and to the shell when typed
    /// fast or pasted).
    non_autodetected_input_type: InputType,

    /// Buffer text whose detection the view has requested (#833) and which has not been settled
    /// yet. Submitting exactly this text now would race its verdict (see
    /// [`Self::is_detection_pending_for`]).
    pending_detection_buffer: Option<String>,

    /// Set when something asked "is a verdict still pending?" and got yes, i.e. an Enter is (or
    /// may be) held behind the verdict. Only then does settling a detection notify observers
    /// (#1181): an unconditional notify re-rendered the message bars, status bar and terminal
    /// view after every keystroke's verdict, an extra frame per key with nothing visible changed.
    settle_wake_wanted: WakeFlag,

    /// The input config when the pending detection was requested. A verdict that left a
    /// different config (type or lock) changed what views render, so settling notifies even
    /// when nobody waits: not every view that draws the mode (placeholder, colouring, icon)
    /// is told by `InputTypeChanged` alone.
    config_when_requested: InputConfig,

    /// Incremented for every spawned classification task, so a task that completed just before
    /// being replaced cannot apply its verdict (or clear the handle) over its replacement.
    detection_generation: u64,

    /// The shell typeahead the view last inserted at the front of the buffer, while the buffer
    /// still starts with it (#886): keys typed while a command was running, or before the editor
    /// regained focus, went to the shell's tty and were reported back into the editor. When a
    /// burst splits or merges that text, the pieces (`ho b199 >> ~/blk.txt`, `b22 b23`,
    /// `xtecho b5 >> ~/blk.txt`) carry no command head the pre-filter could recognise, so the
    /// classifier read them as prose. See [`Self::is_shell_typeahead`] for what this decides.
    /// Dropped by a submit, an emptied buffer, an edit that removes the typeahead from the front
    /// (select-all and paste), or the user choosing AI explicitly.
    typeahead_prefix: Option<String>,
    /// Whether that typeahead began right after a command, before the app knew the shell was
    /// back at its prompt (#933): not typed ahead for a running program, so it is classified like
    /// anything typed in the editor.
    typeahead_young: bool,

    /// Whether the input buffer was empty at the time the lock was set.  This will be true
    /// if a persistent lock is in place and a buffer is submitted.
    was_lock_set_with_empty_buffer: bool,

    /// The last submit and how it was routed, for the one-click misroute recovery (#835).
    misroute: MisrouteMemory,

    /// `None` on the TUI surface, which has no agent-view controller and is always treated as
    /// agent-view-active and never fullscreen.
    agent_view_controller: Option<ModelHandle<AgentViewController>>,

    /// Handle to the per-pane context model. Used to read pending attachments / blocks when
    /// deciding whether to force-lock the input to AI mode (see
    /// [`BlocklistAIContextModel::has_locking_attachment`]).
    ai_context_model: ModelHandle<BlocklistAIContextModel>,

    terminal_view_id: EntityId,

    /// View-supplied policy for decisions the model cannot make view-agnostically:
    /// lock gating, the autodetection setting for the surface's current context,
    /// and reactive config transitions driven by AI-settings and
    /// conversation-selection events.
    policy: InputModePolicyHandle,

    autodetect_abort_handle: Option<AbortHandle>,
    /// Compute the jobs of the current detection request have spent (#1351). Replaced by each
    /// new request.
    detection_clock: ComputeClock,
    model: Arc<FairMutex<TerminalModel>>,
}

/// #1183: whether a command-history close match may decide the input type. Not in an agent
/// conversation for a line with no positive command evidence: `make commit port` is a close
/// match of an earlier `make ...` line, and a history verdict returns before the classifier's
/// safety gate runs.
fn history_close_match_allowed(
    in_active_agent_view: bool,
    snapshot: &ParsedTokensSnapshot,
) -> bool {
    !(in_active_agent_view
        && input_classifier::util::lacks_command_evidence_in_agent_conversation(snapshot))
}

/// The #1234 decision procedure (`input_classifier::util::routing::route`) is the default.
/// `PHOSPHOR_ROUTING_V1=1` keeps the rule set it replaced (the pre-filter, the typeahead rule and
/// the Enter-time settle as separate steps), to rule the redesign in or out when a line routes
/// unexpectedly. Read once.
fn routing_v2_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        std::env::var("PHOSPHOR_ROUTING_V1")
            .ok()
            .as_deref()
            .is_none_or(|flag| flag == "0")
    })
}

/// What the view knows about the session a buffer is typed into, for routing (#1234). Cheap to
/// build on every keystroke: nothing here walks the session's command list.
#[derive(Clone, Copy, Debug, Default)]
pub struct RoutingSessionFacts {
    pub shell: ShellFamily,
}

/// The syntax family routing reasons about for a session's shell.
pub fn shell_family(shell_type: ShellType) -> ShellFamily {
    match shell_type {
        ShellType::Bash | ShellType::Zsh => ShellFamily::Posix,
        ShellType::Fish => ShellFamily::Fish,
        ShellType::PowerShell => ShellFamily::PowerShell,
        ShellType::Nu => ShellFamily::Nu,
    }
}

/// The deciding rule, at debug level (#1234): route, rule, surface and the first word only, since
/// the rest of the buffer may hold a secret.
fn log_routing_decision(
    stage: &str,
    surface: RoutingSurface,
    buffer_text: &str,
    decision: &Decision,
) {
    if log::log_enabled!(target: "phosphor::routing", log::Level::Debug) {
        let head: String = buffer_text
            .split_whitespace()
            .next()
            .unwrap_or("")
            .chars()
            .take(32)
            .collect();
        log::debug!(
            target: "phosphor::routing",
            "{stage}: {:?} by {} ({surface:?}, head {head:?}, {} bytes)",
            decision.route,
            decision.rule,
            buffer_text.len()
        );
    }
}

impl BlocklistAIInputModel {
    /// Builds a mock input model for the `warp_tui` test suite, wired against a
    /// mock terminal, a test `Sessions`, and an agent-view-less context model
    /// (mirroring the real TUI surface). The `input_config` is sourced from the
    /// supplied [`InputModePolicy`], and `Self` is constructed directly so the
    /// mock does not depend on the `CLIAgentSessionsModel` singleton that
    /// [`Self::new_tui`] subscribes to. Ported from Warp OSS; gated on `test-util`.
    ///
    /// [`InputModePolicy`]: super::input_mode_policy::InputModePolicy
    #[cfg(any(test, feature = "test-util"))]
    pub fn mock(
        policy: super::input_mode_policy::InputModePolicyHandle,
        ctx: &mut AppContext,
    ) -> ModelHandle<Self> {
        let model = Arc::new(FairMutex::new(TerminalModel::mock(None, None)));
        let terminal_view_id = EntityId::new();
        // Use the singleton-free, agent-view-less context model so `mock` does not
        // depend on the `BlocklistAIHistoryModel`/`LLMPreferences`/`AISettings`
        // singleton web that `BlocklistAIContextModel::new` subscribes to.
        let conversation_selection = ctx
            .add_model(|_| Box::new(MockConversationSelection) as Box<dyn ConversationSelection>);
        let ai_context_model = ctx.add_model(|_| {
            BlocklistAIContextModel::mock_agent_view_less(
                model.clone(),
                terminal_view_id,
                conversation_selection,
            )
        });
        let input_config = policy.initial_config(ctx);
        ctx.add_model(|_| Self {
            input_config,
            last_ai_autodetection_ts: None,
            explicit_input_type_set: None,
            non_autodetected_input_type: input_config.input_type,
            pending_detection_buffer: None,
            settle_wake_wanted: WakeFlag::default(),
            config_when_requested: input_config,
            detection_generation: 0,
            typeahead_prefix: None,
            typeahead_young: false,
            was_lock_set_with_empty_buffer: false,
            misroute: MisrouteMemory::default(),
            agent_view_controller: None,
            ai_context_model,
            terminal_view_id,
            policy,
            autodetect_abort_handle: None,
            detection_clock: ComputeClock::default(),
            model,
        })
    }

    pub fn new(
        model: Arc<FairMutex<TerminalModel>>,
        agent_view_controller: ModelHandle<AgentViewController>,
        ai_context_model: ModelHandle<BlocklistAIContextModel>,
        policy: InputModePolicyHandle,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        Self::new_inner(
            model,
            Some(agent_view_controller),
            ai_context_model,
            policy,
            terminal_view_id,
            ctx,
        )
    }

    /// Constructs an input model for the TUI surface, which has no agent-view controller.
    pub fn new_tui(
        model: Arc<FairMutex<TerminalModel>>,
        ai_context_model: ModelHandle<BlocklistAIContextModel>,
        policy: InputModePolicyHandle,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        Self::new_inner(model, None, ai_context_model, policy, terminal_view_id, ctx)
    }

    fn new_inner(
        model: Arc<FairMutex<TerminalModel>>,
        agent_view_controller: Option<ModelHandle<AgentViewController>>,
        ai_context_model: ModelHandle<BlocklistAIContextModel>,
        policy: InputModePolicyHandle,
        terminal_view_id: EntityId,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        // Reactively restore input config when CLI agent rich input closes.
        ctx.subscribe_to_model(
            &CLIAgentSessionsModel::handle(ctx),
            move |me, event, ctx| {
                let CLIAgentSessionsModelEvent::InputSessionChanged {
                    terminal_view_id: event_view_id,
                    previous_input_state,
                    ..
                } = event
                else {
                    return;
                };
                if *event_view_id != terminal_view_id {
                    return;
                }
                if let CLIAgentInputState::Open {
                    previous_input_config,
                    previous_was_lock_set_with_empty_buffer,
                    ..
                } = previous_input_state
                {
                    me.restore_input_config(
                        *previous_input_config,
                        *previous_was_lock_set_with_empty_buffer,
                        ctx,
                    );
                }
            },
        );

        ctx.subscribe_to_model(&AISettings::handle(ctx), move |me, event, ctx| {
            // Computing the guarded autodetection state takes the terminal-model lock, so only
            // compute it for the one event whose handling can need it; the policy must not rely
            // on it for any other event.
            let is_autodetection_enabled_for_current_context =
                matches!(event, AISettingsChangedEvent::AIAutoDetectionEnabled { .. })
                    && me.is_autodetection_enabled_for_current_context(ctx);
            if let Some(update) = me.policy.config_on_ai_settings_changed(
                event,
                me.input_config(),
                is_autodetection_enabled_for_current_context,
                ctx,
            ) {
                me.apply_policy_update(update, ctx);
            }
        });

        // Reactive config transitions driven by the surface's conversation selection. The
        // selection handle is not a constructor parameter: both surfaces already hand the same
        // handle to the context model (`AgentViewConversationSelection` on the GUI,
        // `TuiConversationSelection` on the TUI), so it is read back from there instead of being
        // threaded through `new`/`new_tui` and every call site a second time.
        //
        // The `.clone()` releases the borrow of `ctx` taken by `as_ref` before
        // `subscribe_to_model` takes it mutably.
        let conversation_selection = ai_context_model.as_ref(ctx).conversation_selection().clone();
        ctx.subscribe_to_model(&conversation_selection, |me, event, ctx| {
            // A policy that returns `None` leaves the config untouched -- no unconditional
            // rewrite on every selection event.
            if let Some(update) =
                me.policy
                    .config_on_conversation_selection_changed(event, me.input_config(), ctx)
            {
                me.apply_policy_update(update, ctx);
            }
        });

        if FeatureFlag::AgentView.is_enabled() {
            if let Some(agent_view_controller) = &agent_view_controller {
            ctx.subscribe_to_model(agent_view_controller, |me, event, ctx| match event {
                AgentViewControllerEvent::EnteredAgentView {
                    display_mode,
                    origin,
                    ..
                } => {
                    if display_mode.is_inline() {
                        me.set_input_config_internal(
                            InputConfig {
                                input_type: InputType::AI,
                                is_locked: true,
                            },
                            ctx,
                        );
                    } else if matches!(origin, AgentViewEntryOrigin::ClearBuffer) {
                        let is_autodetection_enabled =
                            AISettings::as_ref(ctx).is_ai_autodetection_enabled(ctx);
                        me.set_input_config_internal(
                            InputConfig {
                                input_type: me.input_config().input_type,
                                is_locked: !is_autodetection_enabled,
                            },
                            ctx,
                        );
                    } else if me.has_locking_attachment(ctx) {
                        // Interaction patterns that should fully bypass NLD on
                        // entry: image / file attachment in progress / attached, or block
                        // already in pending context. Force-lock to AI regardless of the
                        // user's NLD setting so the classifier never gets a chance to drop
                        // the buffer back to shell.
                        me.set_input_config_internal(
                            InputConfig {
                                input_type: InputType::AI,
                                is_locked: true,
                            },
                            ctx,
                        );
                    } else {
                        let is_autodetection_enabled =
                            AISettings::as_ref(ctx).is_ai_autodetection_enabled(ctx);
                        if is_autodetection_enabled {
                            // Upon entering the agent view, temporarily disable autodetection as
                            // the existing buffer contents, if any are now most likely intended to
                            // be sent to the agent, and if the input would otherwise trigger a
                            // false-negative classification, we'd drop the user right into shell
                            // mode.
                            me.temporarily_disable_autodetection(ctx);
                        }
                        me.set_input_config_internal(
                            InputConfig {
                                input_type: InputType::AI,
                                is_locked: !is_autodetection_enabled,
                            },
                            ctx,
                        );
                    }
                }
                AgentViewControllerEvent::ExitedAgentView {
                    is_exit_before_new_entrance,
                    ..
                } => {
                    if !is_exit_before_new_entrance {
                        // When truly exiting agent view, use the terminal-specific NLD setting
                        // since the user is returning to terminal mode.
                        let is_nld_in_terminal_enabled =
                            AISettings::as_ref(ctx).is_nld_in_terminal_enabled(ctx);
                        me.set_input_config_internal(
                            InputConfig {
                                input_type: InputType::Shell,
                                is_locked: !is_nld_in_terminal_enabled,
                            },
                            ctx,
                        );
                    }
                }
                _ => (),
            });
            }
        }

        let input_config = policy.initial_config(ctx);
        Self {
            input_config,
            agent_view_controller,
            ai_context_model,
            terminal_view_id,
            policy,
            last_ai_autodetection_ts: None,
            explicit_input_type_set: None,
            non_autodetected_input_type: input_config.input_type,
            pending_detection_buffer: None,
            settle_wake_wanted: WakeFlag::default(),
            config_when_requested: input_config,
            detection_generation: 0,
            typeahead_prefix: None,
            typeahead_young: false,
            was_lock_set_with_empty_buffer: false,
            misroute: MisrouteMemory::default(),
            autodetect_abort_handle: None,
            detection_clock: ComputeClock::default(),
            model,
        }
    }

    /// Applies a policy-produced config update.
    fn apply_policy_update(&mut self, update: PolicyConfigUpdate, ctx: &mut ModelContext<Self>) {
        if update.temporarily_disable_autodetection {
            self.temporarily_disable_autodetection(ctx);
        }
        self.set_input_config_internal(update.config, ctx);
    }

    /// Convenience wrapper around `BlocklistAIContextModel::has_locking_attachment`.
    fn has_locking_attachment(&self, app: &AppContext) -> bool {
        self.ai_context_model.as_ref(app).has_locking_attachment()
    }

    /// Returns the InputType enum which specifies how we will handle the terminal input.
    pub fn input_type(&self) -> InputType {
        self.input_config.input_type
    }

    /// Whether the input type is locked. Does not take user autodetection setting or feature flags
    /// into account.
    pub fn is_input_type_locked(&self) -> bool {
        self.input_config.is_locked
    }

    pub fn is_ai_input_enabled(&self) -> bool {
        matches!(self.input_config.input_type, InputType::AI)
    }

    pub fn is_terminal_use_active_or_pending(&self) -> bool {
        let model = self.model.lock();
        let active_block = model.block_list().active_block();
        // Keep AI input locked while an agent-requested command is waiting for its
        // CLI subagent, while the user has tagged the agent in, or while the user
        // can hand control of an active monitored command back to the agent.
        active_block.is_agent_driving_command()
            || active_block.is_agent_tagged_in()
            || active_block.is_eligible_for_agent_handoff()
    }

    pub fn input_config(&self) -> InputConfig {
        self.input_config
    }

    pub fn last_ai_autodetection_ts(&self) -> Option<Instant> {
        self.last_ai_autodetection_ts
    }

    /// Sets the input config iff the input is in classic mode (i.e. not UDI).
    pub fn set_input_config_for_classic_mode(
        &mut self,
        new_config: InputConfig,
        ctx: &mut ModelContext<Self>,
    ) {
        // When agent view is active, the input should behave like Universal mode
        // even if Classic mode is selected (e.g. when PS1 is enabled).
        if FeatureFlag::AgentView.is_enabled() && self.agent_view_controller.as_ref().is_none_or(|c| c.as_ref(ctx).is_active())
        {
            return;
        }

        let input_type = InputSettings::as_ref(ctx).input_type(ctx);
        if !matches!(input_type, InputBoxType::Classic) {
            return;
        }
        self.set_input_config_internal(new_config, ctx);
    }

    /// #935: after a cancel or reject inside an active agent view the conversation is still the
    /// context, so the follow-up input gets the state an approve leaves behind: AI, unlocked when
    /// autodetection is enabled (so a clearly-shell line such as a known command still runs in
    /// the shell, via the #906 veto at Enter) and locked otherwise. Returns false, doing nothing,
    /// outside an active agent view; the caller then falls back to the classic-mode downgrade.
    pub fn restore_agent_view_input_after_cancellation(
        &mut self,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if !FeatureFlag::AgentView.is_enabled()
            || !self
                .agent_view_controller
                .as_ref()
                .is_some_and(|c| c.as_ref(ctx).is_active())
        {
            return false;
        }
        let is_autodetection_enabled = AISettings::as_ref(ctx).is_ai_autodetection_enabled(ctx);
        self.set_input_config_internal(
            InputConfig {
                input_type: InputType::AI,
                is_locked: !is_autodetection_enabled,
            },
            ctx,
        );
        true
    }

    /// Swaps between Agent/Shell input types while preserving lock state. Temporarily disables
    /// autodetection.
    pub fn set_input_type(&mut self, input_type: InputType, ctx: &mut ModelContext<Self>) {
        self.temporarily_disable_autodetection(ctx);
        let current_config = self.input_config();
        self.set_input_config_internal(current_config.with_input_type(input_type), ctx);
    }

    /// Does not disable autodetection.
    fn set_input_config_internal(
        &mut self,
        new_config: InputConfig,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        self.write_input_config(new_config, false, ctx)
    }

    /// Writes an input type that autodetection decided. Unlike
    /// [`Self::set_input_config_internal`], this does not move
    /// [`Self::non_autodetected_input_type`].
    fn apply_autodetected_input_type(
        &mut self,
        input_type: InputType,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        self.write_input_config(
            InputConfig {
                input_type,
                ..self.input_config()
            },
            true,
            ctx,
        )
    }

    fn write_input_config(
        &mut self,
        new_config: InputConfig,
        is_autodetected: bool,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        // Locking the input to AI is only allowed when the view's policy permits it (e.g. the
        // GUI only allows it inside an active agent view or an open CLI agent rich input
        // session; the TUI always permits it).
        if new_config.input_type.is_ai()
            && new_config.is_locked
            && !self.policy.allows_locked_ai_input(ctx)
        {
            return false;
        }

        if !is_autodetected {
            self.non_autodetected_input_type = new_config.input_type;
            // Choosing AI for a typeahead buffer overrides its shell provenance (#886).
            if new_config.input_type.is_ai() {
                self.typeahead_prefix = None;
            }
        }

        if self.input_config == new_config {
            return false;
        }

        let old_config = self.input_config;

        if !new_config.is_locked && new_config.input_type.is_ai() {
            self.last_ai_autodetection_ts = Some(Instant::now());
        } else {
            self.last_ai_autodetection_ts = None;
        }

        // Capped: past the cap no reader's outcome changes, and every write is a persisted
        // setting change that re-renders the settings pages (see `ENTERED_AGENT_MODE_COUNT_CAP`).
        if new_config.input_type.is_ai()
            && crate::settings::should_count_agent_mode_entry(
                *AISettings::as_ref(ctx).entered_agent_mode_num_times,
            )
        {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let new_num_times = *settings.entered_agent_mode_num_times + 1;
                report_if_error!(settings
                    .entered_agent_mode_num_times
                    .set_value(new_num_times, ctx));
            });
        }

        self.input_config = new_config;

        // Emit specific events for what actually changed
        if old_config.input_type != new_config.input_type {
            ctx.emit(BlocklistAIInputEvent::InputTypeChanged { config: new_config });
        }

        if old_config.is_locked != new_config.is_locked {
            ctx.emit(BlocklistAIInputEvent::LockChanged { config: new_config });
        }

        true
    }

    /// Allows you to set the input config and mutate the lock state. Temporarily disables autodetection.
    pub fn set_input_config(
        &mut self,
        new_config: InputConfig,
        is_input_buffer_empty: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.temporarily_disable_autodetection(ctx);
        self.set_input_config_internal(new_config, ctx);
        if new_config.is_locked {
            self.abort_in_progress_detection();
        }
        self.was_lock_set_with_empty_buffer = self.is_input_type_locked() && is_input_buffer_empty;
    }

    /// Restores a previous input config without recomputing whether the lock was set while the
    /// buffer was empty.
    pub fn restore_input_config(
        &mut self,
        new_config: InputConfig,
        was_lock_set_with_empty_buffer: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.temporarily_disable_autodetection(ctx);
        self.set_input_config_internal(new_config, ctx);
        self.abort_in_progress_detection();
        self.was_lock_set_with_empty_buffer = was_lock_set_with_empty_buffer;
    }

    /// Returns `false` if the input type is locked and we will not attempt to automatically detect
    /// and change the input type.
    pub fn should_run_input_autodetection(&self, app: &AppContext) -> bool {
        FeatureFlag::AgentMode.is_enabled()
            && self.is_autodetection_enabled_for_current_context(app)
            && !self.input_config.is_locked
    }

    /// Returns whether autodetection is enabled for the current context.
    /// When AgentView is enabled, this checks whether we're in agent view or terminal mode
    /// and returns the appropriate setting.
    pub fn is_autodetection_enabled_for_current_context(&self, app: &AppContext) -> bool {
        // If the agent is driving a command (in control, or its requested command is waiting for
        // the CLI subagent), is tagged in, or can be handed control back, don't run
        // autodetection. This must be the broader `is_terminal_use_active_or_pending` (as at the
        // pin), not `is_agent_in_control_or_tagged_in`: otherwise NLD can classify a line meant
        // for the agent as Shell and it gets queued as a shell command.
        if self.is_terminal_use_active_or_pending() {
            return false;
        }

        // Defense in depth: while there is a pending attachment (image / file) or block,
        // the classifier must never have a chance to flip the input back to shell mode, even
        // per-keystroke. The `EnteredAgentView` subscriber and `set_input_mode_agent` already
        // lock at entry; this guard protects the window if any future caller forgets.
        if self.has_locking_attachment(app) {
            return false;
        }

        self.policy.is_autodetection_enabled(app)
    }

    /// Temporarily disable autodetection for a fixed duration.
    /// Useful for operations like history selection where we don't want
    /// autodetection to interfere with the manual input type setting.
    ///
    /// The suppression only covers the buffer the explicit write was made for (see
    /// [`ExplicitInputTypeSet`]). An explicit write also settles any pending detection: the user
    /// (or a flow acting for them) has decided, so a classifier verdict must not be waited for or
    /// override it at submission.
    fn temporarily_disable_autodetection(&mut self, ctx: &mut ModelContext<Self>) {
        self.explicit_input_type_set = Some(ExplicitInputTypeSet {
            at: Instant::now(),
            buffer_text: None,
        });
        if self.pending_detection_buffer.take().is_some() {
            // The verdict is no longer wanted: an Enter waiting on it is released by this event
            // (#1351), not found by a timer.
            self.wake_verdict_waiter(ctx);
        }
    }

    /// Notifies observers if an Enter is waiting on the verdict (see [`Self::settle_detection`]).
    fn wake_verdict_waiter(&mut self, ctx: &mut ModelContext<Self>) {
        if self.settle_wake_wanted.swap(false, AtomicOrdering::Relaxed) {
            ctx.notify();
        }
    }

    /// Returns `true` if a detection request for `buffer_text` falls under the explicit-write
    /// suppression window. The first request inside the window claims the explicit write for its
    /// buffer; later requests are suppressed only while they are for that same buffer (#833).
    fn is_detection_suppressed_by_explicit_set(&mut self, buffer_text: &str) -> bool {
        let Some(explicit) = self.explicit_input_type_set.as_mut() else {
            return false;
        };
        if !explicit.is_within_window() {
            return false;
        }
        if explicit.buffer_text.is_none() {
            explicit.buffer_text = Some(buffer_text.to_owned());
            return true;
        }
        if explicit.buffer_text.as_deref() == Some(buffer_text) {
            return true;
        }
        // Different text: the explicit write was for another buffer and is history.
        self.explicit_input_type_set = None;
        false
    }

    /// Whether the input is the agent's own steer/follow-up box: AI is the live type and the
    /// selected conversation is still streaming (#851). Autodetection must not flip this input
    /// to Shell: a shell-looking line typed to steer the agent would run in the shell and
    /// silently cancel the running answer. Choosing Shell explicitly still works.
    fn is_steering_running_agent(&self, app: &AppContext) -> bool {
        self.input_config.is_ai()
            && self
                .ai_context_model
                .as_ref(app)
                .selected_conversation_status_for_hint(app)
                .is_some_and(|status| status.is_in_progress())
    }

    /// Ends an explicit input-type write that no detection request has claimed yet, for a typed
    /// edit: the write (the agent view's entry, a toggle) was about the buffer before this
    /// keystroke, and the user typing is asking autodetection a new question. A claimed write
    /// keeps its own rules (#833).
    pub fn release_unclaimed_explicit_write(&mut self) {
        if self
            .explicit_input_type_set
            .as_ref()
            .is_some_and(|explicit| explicit.buffer_text.is_none())
        {
            self.explicit_input_type_set = None;
        }
    }

    /// Records that the view has requested input-type detection for `buffer_text` (an edit the
    /// user made, or a re-detection the view triggered). The view classifies a parse only while
    /// its text is still requested ([`Self::is_detection_requested_for`]) (#833).
    pub fn note_detection_requested(&mut self, buffer_text: String) {
        // #1604: a request for the text whose job is already in flight keeps that job and its
        // clock (#1335: a paste's `Edited` event re-requests what the paste path requested, and
        // an Enter already waiting on that job must not see it aborted and the clock it measures
        // reset -- for a buffer over `MAX_EAGER_ROUTING_BYTES` nothing but the next Enter would
        // start another). Any other request supersedes the job in flight: its result must not
        // land as a verdict, and a large buffer's Enter starts a job only when none is in flight.
        let same_text_in_flight = self.detection_job_in_flight()
            && self.pending_detection_buffer.as_deref() == Some(buffer_text.as_str());
        if !same_text_in_flight {
            self.abort_in_progress_detection();
        }
        let explicit_state = self
            .explicit_input_type_set
            .as_ref()
            .map(|explicit| (explicit.is_within_window(), explicit.buffer_text.is_none()));
        match explicit_state {
            // The explicit write is history: this edit is a new question for autodetection.
            Some((false, _)) => self.explicit_input_type_set = None,
            // The first edit inside the window is the one that accompanied the explicit write
            // (e.g. the history entry it inserted); the choice applies to its text.
            Some((true, true)) => {
                self.is_detection_suppressed_by_explicit_set(&buffer_text);
            }
            // Claimed: different text ends the explicit write, the same text keeps it.
            Some((true, false)) => {
                self.is_detection_suppressed_by_explicit_set(&buffer_text);
            }
            None => {}
        }
        self.drop_typeahead_prefix_unless_leading(&buffer_text);
        if same_text_in_flight {
            return;
        }
        self.pending_detection_buffer = Some(buffer_text);
        self.detection_clock = ComputeClock::default();
        self.config_when_requested = self.input_config;
    }

    /// Whether submitting `buffer_text` right now would race an unfinished input-type detection
    /// for it: autodetection is live (not locked, not disabled), the user's last edit requested a
    /// detection for exactly this text, and no verdict -- autodetected or explicit -- has settled
    /// it yet.
    ///
    /// An explicit write whose buffer is still unclaimed (no detection request has arrived since
    /// it) is treated as covering the current buffer: nothing has asked to reclassify it, and an
    /// explicit choice is never overridden. Enter consults it through
    /// [`Self::settle_pending_detection_for_submit`] (#886, the shell-ward half of #833's F6); it
    /// is the model's answer to "is this buffer's mode a real verdict?".
    pub fn is_detection_pending_for(&self, buffer_text: &str, app: &AppContext) -> bool {
        let pending = self.detection_pending_for_buffer_text(buffer_text, app);
        if pending {
            // The asker is about to wait for the verdict; wake it when it lands (#1181).
            self.settle_wake_wanted.store(true, AtomicOrdering::Relaxed);
        }
        pending
    }

    /// [`Self::is_detection_pending_for`] without registering a wait for the verdict.
    fn detection_pending_for_buffer_text(&self, buffer_text: &str, app: &AppContext) -> bool {
        if buffer_text.trim().is_empty()
            || !self.should_run_input_autodetection(app)
            || self.is_steering_running_agent(app)
        {
            return false;
        }
        let is_covered_by_explicit_set =
            self.explicit_input_type_set
                .as_ref()
                .is_some_and(|explicit| {
                    explicit
                        .buffer_text
                        .as_deref()
                        .is_none_or(|explicit_buffer| explicit_buffer == buffer_text)
                });
        if is_covered_by_explicit_set {
            return false;
        }
        self.pending_detection_buffer.as_deref() == Some(buffer_text)
    }

    /// Whether the user's latest detection-requesting edit was for exactly `buffer_text` and its
    /// verdict has not landed yet. The view's background path only classifies a parse when this
    /// holds, so a parse that lands after its text was submitted (or superseded) is dropped.
    pub fn is_detection_requested_for(&self, buffer_text: &str) -> bool {
        self.pending_detection_buffer.as_deref() == Some(buffer_text)
    }

    /// Records that the view inserted `typeahead` at the front of the buffer (#886); see
    /// [`Self::typeahead_prefix`].
    pub fn note_typeahead_inserted(&mut self, typeahead: String) {
        self.typeahead_prefix = (!typeahead.trim().is_empty()).then_some(typeahead);
        self.typeahead_young = false;
    }

    /// Forgets an explicit input-type write and the detection suppression it opened (#933). For
    /// text that arrives by the typeahead hand-off: the explicit write (a forced-shell submit of
    /// the previous command) was a choice for a different buffer, and within its window the
    /// first detection request is swallowed, so the new text would never be classified.
    pub fn clear_explicit_input_type_set(&mut self) {
        self.explicit_input_type_set = None;
    }

    /// Marks the typeahead provenance just noted as young (#933).
    pub fn mark_typeahead_young(&mut self) {
        self.typeahead_young = self.typeahead_prefix.is_some();
    }

    /// Whether the buffer carries typeahead provenance that is young (#933).
    pub fn typeahead_is_young(&self) -> bool {
        self.typeahead_prefix.is_some() && self.typeahead_young
    }

    /// Drops the typeahead provenance (#933): the text is to be routed like anything the user
    /// typed in the editor, so the agent-ward rules apply to it.
    pub fn clear_typeahead_provenance(&mut self) {
        self.typeahead_prefix = None;
    }

    /// The typeahead provenance currently held (#886), for view tests.
    #[cfg(test)]
    pub(crate) fn typeahead_prefix(&self) -> Option<&str> {
        self.typeahead_prefix.as_deref()
    }

    /// Delivers the verdict `input_type` for `buffer_text` the way the classifier's result does:
    /// applies it and settles the pending detection, notifying observers (#1351), for view tests.
    #[cfg(test)]
    pub(crate) fn deliver_verdict_for_test(
        &mut self,
        input_type: InputType,
        buffer_text: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        self.settle_with_route(input_type, buffer_text, ctx);
    }

    /// Whether the buffer carries typeahead provenance (#886) for an edit to re-check.
    pub fn has_typeahead_provenance(&self) -> bool {
        self.typeahead_prefix.is_some()
    }

    /// Records an edit of the buffer, now `buffer_text`: provenance survives only while the
    /// typeahead is still at its front (#886).
    pub fn note_buffer_edited(&mut self, buffer_text: &str) {
        self.drop_typeahead_prefix_unless_leading(buffer_text);
    }

    /// Drops the typeahead provenance once `buffer_text` no longer starts with the typeahead: the
    /// user replaced it (select-all and paste, a history pick) or emptied the buffer, so the text
    /// is theirs (#886).
    fn drop_typeahead_prefix_unless_leading(&mut self, buffer_text: &str) {
        if self
            .typeahead_prefix
            .as_deref()
            .is_some_and(|prefix| !buffer_text.starts_with(prefix))
        {
            self.typeahead_prefix = None;
        }
    }

    /// Whether the input is typed inside an active agent view (#935, #1183, #1184): the
    /// conversation is the context, so a shell verdict needs positive evidence.
    pub fn in_active_agent_view(&self, app: &AppContext) -> bool {
        FeatureFlag::AgentView.is_enabled()
            && self
                .agent_view_controller
                .as_ref()
                .is_some_and(|c| c.as_ref(app).is_active())
    }

    /// #1184: in an active agent view, a line the user submits unlocked whose first word is a
    /// slash command spelling rather than a path (`/mod` after Esc closed the slash menu) is
    /// text for the agent, never a command: bash would run it as a path, and a path that exists
    /// would execute. Moves an unlocked Shell input to AI and returns true. A real path
    /// (`/usr/bin/ls`), a locked Shell input (the user chose it) and any other view are left alone.
    pub fn route_unmatched_slash_line_to_agent(
        &mut self,
        buffer_text: &str,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if self.input_config.is_locked
            || !self.input_config.is_shell()
            || !self.in_active_agent_view(ctx)
            || !input_classifier::util::starts_with_slash_command_not_path(buffer_text)
        {
            return false;
        }
        self.abort_in_progress_detection();
        self.apply_autodetected_input_type(InputType::AI, ctx);
        self.settle_detection(buffer_text, ctx);
        true
    }

    /// Whether `buffer_text` is shell typeahead that may be decided Shell without a command head
    /// (#886): it still starts with the typeahead the shell handed back, and the *whole* buffer
    /// carries no prose evidence at all ([`input_classifier::util::has_no_prose_evidence`] --
    /// stricter than the pre-filter, the head word included). A question typed while a command
    /// ran (`exit vim`, `rm the temp files`, `mv files to backup`) is not decided here and goes
    /// through the normal pipeline; provenance never routes anything to the agent.
    fn is_shell_typeahead(&mut self, buffer_text: &str) -> bool {
        self.drop_typeahead_prefix_unless_leading(buffer_text);
        self.typeahead_prefix.is_some()
            && input_classifier::util::has_no_prose_evidence(buffer_text)
    }

    /// Settles a detection still pending for the buffer being submitted, from what can be
    /// decided synchronously about its *whole* text (#886, the shell-ward half of #833's F6).
    ///
    /// Enter routes on the live input type. When the latest edit's verdict has not landed, that
    /// type is the verdict for some earlier buffer -- after an agent submit, `AI`. A burst typed
    /// faster than the parse and the classifier never lets the verdict for the full line land
    /// before its Enter, so every command in the burst went to the agent (#886: 39 complete
    /// `echo bN >> ~/blk.txt` lines in one run). Before #833 the parse of a *prefix* of the line
    /// (`echo b31 >> ~/b`) could still land mid-burst and the pre-filter flipped the mode to
    /// Shell; #833 rightly dropped prefix verdicts (a shell-looking prefix of prose is not a
    /// verdict for the prose), which left nothing to correct the inherited type.
    ///
    /// Shell is decided only by rules that are already precision-first for the whole text: the
    /// user's denylist, [`input_classifier::util::is_unambiguous_shell_command`] (whose prose
    /// vetoes are the #831 holes), or prose-free shell typeahead ([`Self::is_shell_typeahead`]).
    /// The one AI decision is agent-ward: an *inherited Shell* moves to AI when the text reads as
    /// a request ([`Self::reads_as_request`], #834), never for typeahead, a one-off keyword head
    /// or shell structure; or, when `head_unknown_to_shell`, when the text is clearly prose
    /// headed by a word the session's shell does not know (#833). Anything else keeps the live
    /// type, as before. A locked or explicitly
    /// chosen type is never pending, so it is never overridden. Returns `true` if the input type was settled to Shell.
    pub fn settle_pending_detection_for_submit(
        &mut self,
        input: &ParsedTokensSnapshot,
        head_unknown_to_shell: bool,
        head_known_to_shell: bool,
        ran_before_exactly: bool,
        shell: ShellFamily,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if !self.detection_pending_for_buffer_text(&input.buffer_text, ctx) {
            return false;
        }
        // #1604: text over the sync cap is routed by the background job the Enter starts (or is
        // already waiting for): the rules are linear in the text, and a 10 MB paste would hold the
        // UI thread for hundreds of milliseconds here. Undecided, so the Enter waits for that
        // verdict; the clock never routes it (#1577).
        if routes_off_thread(&input.buffer_text) {
            return false;
        }
        if routing_v2_enabled() {
            // #1234: the one procedure, with the session's own answer about the head in place of
            // the parse that has not landed. Undecided keeps the live type, as before: the Enter
            // then waits its budget for the verdict or submits on the inherited type.
            let surface = self.routing_surface(ctx);
            let typeahead = self.typeahead_for_routing(&input.buffer_text);
            let denylisted = self.is_first_word_denylisted(&input.buffer_text, ctx);
            let decision = routing::route(
                input,
                &RoutingContext {
                    surface,
                    shell,
                    head_known_to_shell: if head_known_to_shell {
                        Some(true)
                    } else if head_unknown_to_shell {
                        Some(false)
                    } else {
                        None
                    },
                    commands_fully_loaded: false,
                    command_data_current: false,
                    is_agent_follow_up: self.is_agent_follow_up(),
                    ran_before_exactly,
                    denylisted,
                    typeahead,
                },
            );
            log_routing_decision("enter", surface, &input.buffer_text, &decision);
            return match decision.route {
                Route::Shell => {
                    self.settle_with_route(InputType::Shell, &input.buffer_text, ctx);
                    true
                }
                Route::Agent => {
                    self.settle_with_route(InputType::AI, &input.buffer_text, ctx);
                    false
                }
                Route::Undecided => false,
            };
        }
        // #1185: classified without a trailing comment. #1183: in an agent conversation a line
        // with no positive evidence of being a command is not decided Shell here, and an
        // inherited Shell moves to the agent (never for typeahead, as with requests).
        let classify_snapshot =
            input_classifier::util::strip_trailing_comment(input).into_owned();
        let no_command_evidence = self.in_active_agent_view(ctx)
            && input_classifier::util::lacks_command_evidence_in_agent_conversation(
                &classify_snapshot,
            );
        // #1203: `ran_before_exactly` is the caller's statement that this exact line ran in the
        // session's shell before and succeeded (the matcher's exact, exit-0 case, decided here
        // synchronously instead of by the Enter waiting for it); #1183's evidence rule applies to
        // it as to any Shell decision made here.
        let is_shell = self.is_shell_typeahead(&input.buffer_text)
            || self.is_first_word_denylisted(&input.buffer_text, ctx)
            || (!no_command_evidence
                && (ran_before_exactly
                    || input_classifier::util::is_unambiguous_shell_command(&classify_snapshot)));
        if !is_shell {
            // The agent-ward half of F6: prose pasted or typed fast right after a shell command
            // inherits `Shell` and Enter would run it. Only text that reads as a request in
            // English moves the inherited type (`help me`, `watch the logs for errors`), and
            // never typeahead, a one-off shell keyword head (`echo a and b`) or an inherited AI.
            if self.input_config.is_shell()
                && (self.reads_as_request(&input.buffer_text)
                    || (no_command_evidence && self.typeahead_prefix.is_none())
                    || (head_unknown_to_shell
                        && self.reads_as_prose_with_unknown_head(&input.buffer_text)))
            {
                self.abort_in_progress_detection();
                self.apply_autodetected_input_type(InputType::AI, ctx);
                self.settle_detection(&input.buffer_text, ctx);
            } else if self.input_config.input_type == InputType::AI
                && head_known_to_shell
                && !no_command_evidence
                && !input.buffer_text.trim().contains('\n')
                && !self.reads_as_request(&input.buffer_text)
            {
                // #906: an inherited (or landed) `AI` for a single line whose command the
                // session knows and which does not read as a request (`grep this that`,
                // `make the build`) is a misroute; the gate applies the same veto to a
                // classifier verdict.
                self.abort_in_progress_detection();
                self.apply_autodetected_input_type(InputType::Shell, ctx);
                self.settle_detection(&input.buffer_text, ctx);
                return true;
            }
            return false;
        }
        // The in-flight verdict is for this text and is superseded; a parse of it that lands
        // later is no longer requested and is dropped by the view.
        self.abort_in_progress_detection();
        self.apply_autodetected_input_type(InputType::Shell, ctx);
        self.settle_detection(&input.buffer_text, ctx);
        true
    }

    /// Whether `buffer_text` is safe to move from an inherited `Shell` to the agent at Enter
    /// without waiting for a verdict: it has no typeahead provenance, does not start with a
    /// one-off shell keyword, and reads as a request
    /// ([`input_classifier::util::reads_as_request_to_agent`], #833 F6 / #834).
    fn reads_as_request(&mut self, buffer_text: &str) -> bool {
        self.drop_typeahead_prefix_unless_leading(buffer_text);
        self.typeahead_prefix.is_none()
            && input_classifier::util::reads_as_request_to_agent(buffer_text)
    }

    /// #833: `head_unknown_to_shell` is the caller's statement, from the active session's own
    /// bootstrap data, that the line's first word is no alias, function, builtin, keyword or
    /// `$PATH` executable there (`false` whenever the session cannot say). Same typeahead rule as
    /// [`Self::reads_as_request`]; the prose test is
    /// [`input_classifier::util::reads_as_prose_with_unknown_head`].
    fn reads_as_prose_with_unknown_head(&mut self, buffer_text: &str) -> bool {
        self.drop_typeahead_prefix_unless_leading(buffer_text);
        self.typeahead_prefix.is_none()
            && input_classifier::util::reads_as_prose_with_unknown_head(buffer_text)
    }

    /// The surface routing reasons about (#1234).
    fn routing_surface(&self, app: &AppContext) -> RoutingSurface {
        if self.in_active_agent_view(app) {
            RoutingSurface::AgentConversation
        } else {
            RoutingSurface::PlainTerminal
        }
    }

    /// The buffer's typeahead provenance for routing (#886, #933), reconciled with the buffer
    /// first.
    fn typeahead_for_routing(&mut self, buffer_text: &str) -> RoutingTypeahead {
        self.drop_typeahead_prefix_unless_leading(buffer_text);
        match (self.typeahead_prefix.is_some(), self.typeahead_young) {
            (false, true | false) => RoutingTypeahead::None,
            (true, true) => RoutingTypeahead::Young,
            (true, false) => RoutingTypeahead::Running,
        }
    }

    /// Whether the last visible block is an agent block, so the buffer is a follow-up to it.
    fn is_agent_follow_up(&self) -> bool {
        let model = self.model.lock();
        let block_list = model.block_list();
        let block_index = block_list.last_non_hidden_block_by_index();
        match block_list.last_non_hidden_rich_content_block_after_block(block_index) {
            Some((_, content)) => content.content_type == Some(RichContentType::AIBlock),
            None => false,
        }
    }

    /// Applies a route the procedure decided for `buffer_text` and settles its detection: the
    /// verdict in flight, if any, is superseded.
    fn settle_with_route(
        &mut self,
        input_type: InputType,
        buffer_text: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        self.abort_in_progress_detection();
        self.apply_autodetected_input_type(input_type, ctx);
        self.settle_detection(buffer_text, ctx);
    }

    /// Whether the first word of `buffer_text` is in the user's autodetection denylist.
    fn is_first_word_denylisted(&self, buffer_text: &str, app: &AppContext) -> bool {
        let Some(first_word) = buffer_text.split_whitespace().next() else {
            return false;
        };
        AISettings::as_ref(app)
            .autodetection_command_denylist
            .value()
            .split(',')
            .any(|entry| entry == first_word)
    }

    /// Abandons the pending detection outright: the classifier is late (#1158) or the Enter gave
    /// up on it, so nothing must wait for its verdict any more. Whatever the input type is now
    /// stands. (Size is no longer a reason, #1604: every buffer gets a verdict.)
    pub fn abandon_pending_detection(&mut self) {
        // A verdict still in flight (#1158: a late one) must not land after the submit and set
        // the input type the next command is typed into.
        self.abort_in_progress_detection();
        self.pending_detection_buffer = None;
    }

    /// [`Self::abandon_pending_detection`] for an Enter the clock gave up on and handed back
    /// (#1577) -- except for a buffer over the sync routing cap (#1604), whose request and job are
    /// kept: its classification is linear and will land, and the next Enter must wait for it (or
    /// submit on it once it has) rather than run an unclassified paste on the inherited type. A
    /// typed line's late verdict is dropped as before, so the next Enter is the user's say-so.
    pub fn abandon_pending_detection_after_give_up(&mut self) {
        if self
            .pending_detection_buffer
            .as_deref()
            .is_some_and(routes_off_thread)
        {
            return;
        }
        self.abandon_pending_detection();
    }

    /// Marks detection for `buffer_text` as settled: whatever the input type is now is the
    /// verdict for that text.
    ///
    /// #1181: notifies observers when a pending detection settles, so an Enter deferred behind
    /// the verdict is released by the verdict landing instead of waiting for its next timer tick.
    fn settle_detection(&mut self, buffer_text: &str, ctx: &mut ModelContext<Self>) {
        if self.pending_detection_buffer.as_deref() == Some(buffer_text) {
            self.pending_detection_buffer = None;
            // Only wake observers when someone is waiting on the verdict (an Enter deferred
            // behind it) or the verdict changed the input config; otherwise nothing they
            // render has changed.
            let waiter = self.settle_wake_wanted.swap(false, AtomicOrdering::Relaxed);
            let verdict_changed_config = self.input_config != self.config_when_requested;
            self.config_when_requested = self.input_config;
            if waiter || verdict_changed_config {
                ctx.notify();
            }
        }
    }

    pub fn enable_autodetection(&mut self, input_type: InputType, ctx: &mut ModelContext<Self>) {
        self.set_input_config_internal(
            InputConfig {
                input_type,
                is_locked: false,
            },
            ctx,
        );
        // The goal of this function is to allow autodetection to run, but if we
        // don't clear this, it may be suppressed for a short duration.
        self.explicit_input_type_set = None;
    }

    /// Handles the input buffer being submitted.
    pub fn handle_input_buffer_submitted(&mut self, ctx: &mut ModelContext<Self>) {
        // If the agent is still driving (or can be handed back) a long-running command, keep the
        // input locked to AI mode. See `is_autodetection_enabled_for_current_context` for why this
        // is the broader check.
        let is_terminal_use_active_or_pending = self.is_terminal_use_active_or_pending();

        let new_config = if is_terminal_use_active_or_pending {
            InputConfig {
                input_type: InputType::AI,
                is_locked: true,
            }
        } else {
            // If NLD is enabled and input is currently locked, unlock it, as we want to
            // resume autodetection for the next input.
            self.input_config.unlocked_if_autodetection_enabled(
                self.agent_view_controller.as_ref().is_some_and(|c| c.as_ref(ctx).is_fullscreen()),
                ctx,
            )
        };

        self.set_input_config(
            new_config,
            // We know the buffer is currently empty, as it was just submitted.
            true, ctx,
        );
        // The reset above is made for the now-empty buffer, so the suppression window must not
        // swallow whatever the user types or pastes next (#833, F7: prose pasted within 250 ms of
        // submitting a command stayed in shell mode and Enter ran it). Claim the window for the
        // empty buffer up front instead of letting the next detection request claim it.
        if let Some(explicit) = self.explicit_input_type_set.as_mut() {
            explicit.buffer_text = Some(String::new());
        }
        // The submitted text has been routed; nothing may reclassify it now. The shell submit
        // path leaves the command in the editor until its block completes, so a debounced parse
        // of it can still land after this point, and the narrowed window above no longer
        // swallows it: without this, its verdict (say AI, for prose that just ran in the shell)
        // would become the mode the next command is typed into. The view only classifies a
        // parse whose text is still `is_detection_requested_for`.
        self.pending_detection_buffer = None;
        // The typeahead, if any, went with the submitted text (#886).
        self.typeahead_prefix = None;
    }

    pub fn was_lock_set_with_empty_buffer(&self) -> bool {
        self.was_lock_set_with_empty_buffer
    }

    /// Why a submit made right now would go where it goes (#835): `Locked` when the input type is
    /// locked (which includes explicit toggles that lock), `Explicit` inside the window of an
    /// unlocked explicit write, otherwise `Autodetected`. Read this *before* the submit resets
    /// the input config.
    pub fn submit_route_reason(&self) -> RouteReason {
        if self.input_config.is_locked {
            RouteReason::Locked
        } else if self
            .explicit_input_type_set
            .as_ref()
            .is_some_and(|explicit| {
                // The empty claim a submit leaves behind is not a choice about any buffer.
                explicit.is_within_window() && explicit.buffer_text.as_deref() != Some("")
            })
        {
            RouteReason::Explicit
        } else {
            RouteReason::Autodetected
        }
    }

    /// Remembers a submit for the misroute recovery (#835).
    pub fn record_submit_route(
        &mut self,
        text: &str,
        route: SubmitRoute,
        reason: RouteReason,
        ctx: &mut ModelContext<Self>,
    ) {
        self.misroute
            .record_submit(text, route, reason, Instant::now());
        self.schedule_misroute_expiry(ctx);
        ctx.notify();
    }

    /// Re-renders (observers are notified) once the current offer expires, so a stale label
    /// disappears without waiting for some other event.
    fn schedule_misroute_expiry(&self, ctx: &mut ModelContext<Self>) {
        let now = Instant::now();
        let Some(deadline) = self.misroute.offer_expires_at(now) else {
            return;
        };
        let wait = deadline.saturating_duration_since(now) + std::time::Duration::from_millis(50);
        ctx.spawn(
            async move {
                warpui::r#async::Timer::after(wait).await;
            },
            |_, _, ctx| ctx.notify(),
        );
    }

    /// The input config and its `was_lock_set_with_empty_buffer` flag, to hand back to
    /// [`Self::restore_input_config`] after a temporary override.
    pub fn input_config_snapshot(&self) -> (InputConfig, bool) {
        (self.input_config, self.was_lock_set_with_empty_buffer)
    }

    /// Forgets the remembered submit, e.g. for agent-view follow-ups, which are never offered
    /// a recovery (#835).
    pub fn clear_misroute_memory(&mut self, ctx: &mut ModelContext<Self>) {
        if self.misroute.clear() {
            ctx.notify();
        }
    }

    /// Reports a finished user command to the misroute memory (#835).
    pub fn note_command_finished(
        &mut self,
        command: &str,
        exit_code: warp_core::command::ExitCode,
        ctx: &mut ModelContext<Self>,
    ) {
        if self
            .misroute
            .note_command_finished(command, exit_code, Instant::now())
        {
            self.schedule_misroute_expiry(ctx);
            ctx.notify();
        }
    }

    /// Reports an edit of the input buffer; typing anything new dismisses the offer (#835).
    pub fn dismiss_misroute_offer_on_edit(
        &mut self,
        buffer_is_empty: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.misroute.note_buffer_edited(buffer_is_empty) {
            ctx.notify();
        }
    }

    /// The misroute recovery currently on offer, if any (#835).
    pub fn misroute_offer(&self) -> Option<MisrouteOffer> {
        self.misroute.offer(Instant::now())
    }

    /// Consumes the misroute offer so it fires once (#835).
    pub fn take_misroute_offer(&mut self, ctx: &mut ModelContext<Self>) -> Option<MisrouteOffer> {
        let offer = self.misroute.take_offer(Instant::now());
        ctx.notify();
        offer
    }

    /// The compute clock of the current detection request (#1351): what an Enter waiting for the
    /// verdict measures its give-up cap on, and gets stage-start events from.
    pub fn detection_clock(&self) -> ComputeClock {
        self.detection_clock.clone()
    }

    /// Whether a detection job is in flight for the pending request (#1604). Every new request
    /// aborts the previous job ([`Self::note_detection_requested`]), so an in-flight job is for
    /// the text awaiting its verdict; the view starts a large buffer's job at Enter only when
    /// none is.
    pub fn detection_job_in_flight(&self) -> bool {
        self.autodetect_abort_handle.is_some()
    }

    /// [`Self::abort_in_progress_detection`], unless the job in flight is for `buffer_text`, the
    /// text about to be requested again (#1604; see [`Self::note_detection_requested`]).
    pub fn abort_in_progress_detection_unless_for(&mut self, buffer_text: Option<&str>) {
        if buffer_text.is_some_and(|text| self.pending_detection_buffer.as_deref() == Some(text)) {
            return;
        }
        self.abort_in_progress_detection();
    }

    /// Aborts any in progress work for autodetection.
    pub fn abort_in_progress_detection(&mut self) {
        if let Some(handle) = self.autodetect_abort_handle.take() {
            handle.abort();
        }
    }

    /// If the input type is unlocked, analyze the input and set the input type to the type we
    /// detected. Emits an event if the input mode changed. If the input mode is locked, do
    /// nothing.
    ///
    /// When `session_id` is `Some`, history matching is performed. The `completion_context`
    /// is always used for alias expansion (callers without a live session should pass an
    /// `EmptyCompletionContext`).
    pub fn detect_and_set_input_type<C: CompletionContext + Clone + Send + 'static>(
        &mut self,
        input: ParsedTokensSnapshot,
        completion_context: C,
        session_id: Option<SessionId>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.detect_and_set_input_type_with_facts(
            input,
            completion_context,
            session_id,
            RoutingSessionFacts::default(),
            ctx,
        );
    }

    /// [`Self::detect_and_set_input_type`] with what the view knows about the session (#1234):
    /// the shell's syntax family. A caller without a session passes the default.
    pub fn detect_and_set_input_type_with_facts<C: CompletionContext + Clone + Send + 'static>(
        &mut self,
        input: ParsedTokensSnapshot,
        completion_context: C,
        session_id: Option<SessionId>,
        facts: RoutingSessionFacts,
        ctx: &mut ModelContext<Self>,
    ) {
        // Abort the last autodetect handle if exists.
        self.abort_in_progress_detection();

        // If the input mode is locked, there's no point in running autodetection.
        if !self.should_run_input_autodetection(ctx) {
            self.settle_detection(&input.buffer_text, ctx);
            return;
        }

        // #851: the agent is streaming and the input is its steer box; keep it AI.
        if self.is_steering_running_agent(ctx) {
            self.settle_detection(&input.buffer_text, ctx);
            return;
        }

        // An explicit write keeps its input type -- for the buffer it was made
        // for only; new text is classified (#833).
        if self.is_detection_suppressed_by_explicit_set(&input.buffer_text) {
            self.settle_detection(&input.buffer_text, ctx);
            return;
        }
        if self
            .explicit_input_type_set
            .as_ref()
            .is_some_and(|explicit| !explicit.is_within_window())
        {
            self.explicit_input_type_set = None;
        }

        // #1604: the rules see every byte of the buffer, but a buffer over the sync cap is routed
        // in the spawned job below rather than here on the UI thread: `route` is linear in the
        // text, microseconds for a typed line and hundreds of milliseconds for a 10 MB paste. The
        // view starts this job at Enter for such text (never per keystroke) and the Enter waits
        // for the verdict with a budget that grows with the text (`enter_verdict_budget_for`).
        // Such a buffer has no completer parse (that parse is quadratic and stays capped, #1174),
        // so its head is read from the text. The former #968 "never classify a huge paste" cap is
        // gone: the rules decide a 10 MB script exactly as they decide its first line, and the
        // model classifier sees a bounded opening of it (`truncate_for_model`).
        let off_thread = routes_off_thread(&input.buffer_text);

        // Without a parse (a buffer over the parse cap, or an Enter that beat the debounced parse)
        // the head is read from the text, within a bounded window: a 10 MB buffer with no
        // whitespace must not be scanned to its end for its first word.
        let first_token_str: Option<String> = input
            .parsed_tokens
            .first()
            .map(|t| t.token.as_str().to_owned())
            .or_else(|| head_word_from_text(&input.buffer_text));
        let Some(first_token_str) = first_token_str else {
            // If the buffer is empty, short-circuit and leave input type unchanged.
            //
            // We don't know enough (anything) to be able to change the input type one way or
            // another.
            self.settle_detection(&input.buffer_text, ctx);
            return;
        };

        let denylist: Vec<&str> = AISettings::as_ref(ctx)
            .autodetection_command_denylist
            .value()
            .split(',')
            .collect();

        // Early return if the first token is included in the denylist. No need to parse history.
        if denylist.contains(&first_token_str.as_str()) {
            self.apply_autodetected_input_type(InputType::Shell, ctx);
            self.settle_detection(&input.buffer_text, ctx);
            return;
        }

        // #808: if the buffer is unambiguously a shell command line (`yes | head -c 50000000 >
        // /dev/null`, `trap 'echo t' DEBUG`), force Shell right here, synchronously, before the
        // one-off natural-language shortcut, the history matcher and the classifier get a say.
        // The ONNX model routed both of those to the agent (and with no reachable model the user
        // got a stream error instead of their command); the rules are precision-first, so
        // anything merely *probably* a command still goes through the pipeline below. See
        // `input_classifier::util::is_unambiguous_shell_command`.
        //
        // #1185: classified without a trailing ` # comment`; #1183: in an agent conversation not
        // without positive evidence that the line is a plausible command.
        let in_active_agent_view = self.in_active_agent_view(ctx);
        // Determine if the input is a follow-up to an AI block.
        let is_agent_follow_up = self.is_agent_follow_up();
        // The comment-stripped snapshot (a copy) and the evidence scan are made here only for text
        // routed synchronously; for text over the sync cap the job makes them (#1604).
        let classify_snapshot = (!off_thread)
            .then(|| input_classifier::util::strip_trailing_comment(&input).into_owned());
        let mut off_thread_routing: Option<(RoutingContext, RoutingSurface)> = None;
        let mut history_may_decide = false;

        if classify_snapshot.is_some() && routing_v2_enabled() {
            // #1234: the one procedure decides every clear case here, synchronously; only an
            // undecided line goes on to the history matcher and the classifier below. The exact
            // history lookup is O(1) through the command index (#1203); the session is not asked
            // about the head on the keystroke path, where the completer's description and a
            // complete command index say what it would.
            let surface = if in_active_agent_view {
                RoutingSurface::AgentConversation
            } else {
                RoutingSurface::PlainTerminal
            };
            let typeahead = self.typeahead_for_routing(&input.buffer_text);
            let ran_before_exactly = session_id.is_some_and(|session_id| {
                History::as_ref(ctx).ran_before(session_id, input.buffer_text.trim())
                    == RanBefore::Succeeded
            });
            let decision = routing::route(
                &input,
                &RoutingContext {
                    surface,
                    shell: facts.shell,
                    head_known_to_shell: None,
                    commands_fully_loaded: completion_context.top_level_commands_fully_loaded(),
                    command_data_current: completion_context.top_level_command_data_is_current(),
                    is_agent_follow_up,
                    ran_before_exactly,
                    denylisted: false,
                    typeahead,
                },
            );
            log_routing_decision("keystroke", surface, &input.buffer_text, &decision);
            match decision.route {
                Route::Shell => {
                    self.settle_with_route(InputType::Shell, &input.buffer_text, ctx);
                    return;
                }
                Route::Agent => {
                    self.settle_with_route(InputType::AI, &input.buffer_text, ctx);
                    return;
                }
                Route::Undecided => {}
            }
        } else if let Some(classify_snapshot) = classify_snapshot.as_ref() {
            if input_classifier::util::is_unambiguous_shell_command(classify_snapshot)
                && !(in_active_agent_view
                    && input_classifier::util::lacks_command_evidence_in_agent_conversation(
                        classify_snapshot,
                    ))
            {
                self.apply_autodetected_input_type(InputType::Shell, ctx);
                self.settle_detection(&input.buffer_text, ctx);
                return;
            }

            // #886: text the shell handed back as typeahead was typed at the shell. A burst that
            // the tty split or merged leaves pieces with no recognisable head (`ho b199 >>
            // ~/blk.txt`, `b22 b23`, `b11 blk.txtecho b12`) that the classifier reads as prose,
            // and an AI verdict here is the mode the rest of the burst is then typed into. Only a
            // buffer with no prose evidence anywhere is decided here; anything that could be a
            // question falls through.
            if self.is_shell_typeahead(&input.buffer_text) {
                self.apply_autodetected_input_type(InputType::Shell, ctx);
                self.settle_detection(&input.buffer_text, ctx);
                return;
            }
        }
        if let Some(classify_snapshot) = classify_snapshot.as_ref() {
            // #1183: in an agent conversation, a line with no positive command evidence is never
            // decided Shell by the history close-match below (`make commit port` is a close match
            // of an earlier `make ...` line, and that match returned before the classifier's gate
            // ran).
            history_may_decide = history_close_match_allowed(in_active_agent_view, classify_snapshot);
        } else if routing_v2_enabled() {
            // #1604: the same procedure, run by the job. The exact-history lookup hashes the whole
            // text on the UI thread, so it is not made for this size (the close match in the job
            // covers history); the session is not asked about the head on this path either.
            let surface = if in_active_agent_view {
                RoutingSurface::AgentConversation
            } else {
                RoutingSurface::PlainTerminal
            };
            let typeahead = self.typeahead_for_routing(&input.buffer_text);
            off_thread_routing = Some((
                RoutingContext {
                    surface,
                    shell: facts.shell,
                    head_known_to_shell: None,
                    commands_fully_loaded: completion_context.top_level_commands_fully_loaded(),
                    command_data_current: completion_context.top_level_command_data_is_current(),
                    is_agent_follow_up,
                    ran_before_exactly: false,
                    denylisted: false,
                    typeahead,
                },
                surface,
            ));
        }

        // If we have a session, snapshot its command history (shared `Arc`s, oldest-first: no
        // entry is deep-copied). The #832/#965 filtering in `shell_history_candidates` scans
        // every entry (shell-word splitting per entry), so it runs in the spawned future below,
        // off the main thread, not once per keystroke on it (#1137, like #811 did for
        // autosuggestion). Matching iterates the result newest-first.
        let history_entries = session_id.and_then(|sid| History::as_ref(ctx).commands_shared(sid));

        // Gather agent prompt history (ascending / oldest-first). Ported from the pin
        // (`02b53fcd8`) for #312; off by default, matching the pin's own default-off
        // state (see `FeatureFlag::NldPromptHistoryMatch`'s doc comment).
        let prompt_entries = FeatureFlag::NldPromptHistoryMatch
            .is_enabled()
            .then(|| BlocklistAIHistoryModel::as_ref(ctx).prompt_history_candidates());

        let buffer_cloned = input.buffer_text.clone();
        let other_buffer_cloned = buffer_cloned.clone();
        self.detection_generation = self.detection_generation.wrapping_add(1);
        let detection_generation = self.detection_generation;
        let current_input_type = self.input_type();
        // The one-off natural-language shortcut asks "is the user already talking to the agent?".
        // Answer it from the last input type the user (or a flow acting for them) chose, not the
        // live one: the live type is whatever autodetection guessed for the previous keystroke, so
        // it made the verdict for the final text depend on typing speed (#833, F8).
        let shortcut_input_type = self.non_autodetected_input_type;

        let is_udi_enabled = InputSettings::as_ref(ctx).is_universal_developer_input_enabled(ctx);

        let classifier = InputClassifierModel::as_ref(ctx).classifier();
        let compute_clock = self.detection_clock.clone();
        let handle = ctx
            .spawn(
                async move {
                    // The clock starts when this job runs, not when it was queued (#1351).
                    let _compute = compute_clock.begin();
                    // #1604: a buffer over the sync cap is routed here, off the UI thread, by the
                    // same rules the keystroke path applies synchronously to shorter text.
                    if let Some((routing_context, surface)) = off_thread_routing {
                        let decision = routing::route(&input, &routing_context);
                        log_routing_decision("background", surface, &input.buffer_text, &decision);
                        match decision.route {
                            Route::Shell => return InputType::Shell,
                            Route::Agent => return InputType::AI,
                            Route::Undecided => {}
                        }
                    }
                    let (classify_snapshot, history_may_decide) = match classify_snapshot {
                        Some(snapshot) => (snapshot, history_may_decide),
                        None => {
                            let snapshot =
                                input_classifier::util::strip_trailing_comment(&input).into_owned();
                            let allowed =
                                history_close_match_allowed(in_active_agent_view, &snapshot);
                            (snapshot, allowed)
                        }
                    };
                    drop(input);
                    // First check if the token is a natural language word, if the user was already
                    // in AI mode before this buffer's autodetection began.
                    if matches!(shortcut_input_type, InputType::AI)
                        && is_one_off_natural_language_word(first_token_str.to_lowercase().as_str())
                    {
                        return InputType::AI;
                    }

                    // If this is clearly intended to be a follow-up to an AI block, classify it as AI.
                    if is_agent_follow_up
                        && is_agent_follow_up_input(&buffer_cloned.trim().to_lowercase())
                    {
                        return InputType::AI;
                    }

                    // If we have history entries (i.e., a live session), check for
                    // close matches against command history and agent prompt history.
                    // TODO(vorporeal): decide if we still want to do this with NldImprovements.
                    if let Some(history_entries) = history_entries.filter(|_| history_may_decide) {
                        let history_entries = shell_history_candidates(
                            history_entries.iter().map(|entry| entry.as_ref()),
                            &buffer_cloned,
                        );
                        // Iterate commands newest-first so the first match is the
                        // most-recent matching command.
                        let command_match = most_recent_close_match(
                            &buffer_cloned,
                            history_entries
                                .iter()
                                .rev()
                                .map(|(command, start_ts)| (command.as_str(), *start_ts)),
                            HISTORY_ENTRY_MATCH_CUTOFF,
                        )
                        .await;

                        if let Some(prompt_entries) = &prompt_entries {
                            // `prompt_entries` is ascending (oldest-first); reverse it so the
                            // matcher iterates newest-first and the first match is most-recent.
                            let prompt_match = most_recent_close_match(
                                &buffer_cloned,
                                prompt_entries
                                    .iter()
                                    .rev()
                                    .map(|entry| (&*entry.text, Some(entry.start_ts))),
                                HISTORY_ENTRY_MATCH_CUTOFF,
                            )
                            .await;

                            // The `InputTypeAutoDetectionSource` half of the decision is
                            // discarded here: this model does not thread a decision source
                            // through its own input-type state (that's the separate, larger
                            // port tracked in #399/#254, same as the classifier path above).
                            if let Some((input_type, _source)) =
                                resolve_history_match(command_match, prompt_match)
                            {
                                return input_type;
                            }
                        } else if let Some((input_type, _source)) =
                            resolve_history_match(command_match, HistoryMatch::NoMatch)
                        {
                            // Feature disabled: preserve the command-only behavior.
                            return input_type;
                        }
                    }

                    // Yield so that an attempt to abort the classification is handled.  We do this periodically
                    // so that we can skip doing additional expensive work if the classification is aborted.
                    futures_lite::future::yield_now().await;

                    let input =
                        warp_completer::util::expand_aliases(classify_snapshot, &completion_context)
                            .await;

                    futures_lite::future::yield_now().await;

                    let context = input_classifier::Context {
                        current_input_type,
                        is_agent_follow_up,
                        commands_fully_loaded: completion_context.top_level_commands_fully_loaded(),
                        in_active_agent_view,
                        command_data_current: completion_context
                            .top_level_command_data_is_current(),
                    };
                    // The classifier now also reports *why* it decided (an
                    // `InputClassifierDecisionSource`, #428); this model does not
                    // yet thread a decision source through its own input-type
                    // state (that's the separate, larger `InputTypeAutoDetectionSource`
                    // port tracked in #399/#254), so only the `InputType` is kept here.
                    let new_input_type = classifier
                        .detect_input_type(input.clone(), &context)
                        .await
                        .input_type;

                    futures_lite::future::yield_now().await;

                    new_input_type
                },
                move |me, new_input_type, ctx| {
                    // In theory, we shouldn't need to check this, as we only run autodetection if the input
                    // is not locked, and we should abort the autodetect future if the input is locked, but
                    // we do it anyway out of an abundance of caution.
                    if !me.should_run_input_autodetection(ctx) {
                        me.settle_detection(&other_buffer_cloned, ctx);
                        return;
                    }
                    // If the autodetect abort handle is none, it means we aborted autodetection.
                    // It's possible that the future already completed before we aborted, and then we reach this callback after abort.
                    // In this case, don't set the input type.
                    if me.autodetect_abort_handle.is_none() {
                        return;
                    }
                    // Likewise if a newer detection replaced this one after it had already
                    // completed: the newer one owns the handle and will report its own verdict.
                    if me.detection_generation != detection_generation {
                        return;
                    }
                    // #1604: a background routing job answers for the text it was started with;
                    // a buffer edited since has asked for its own verdict.
                    if off_thread && !me.is_detection_requested_for(&other_buffer_cloned) {
                        me.autodetect_abort_handle = None;
                        return;
                    }
                    me.autodetect_abort_handle = None;
                    me.apply_autodetected_input_type(new_input_type, ctx);
                    me.settle_detection(&other_buffer_cloned, ctx);
                    if current_input_type != new_input_type {
                        let buffer_length = other_buffer_cloned.len();
                        send_telemetry_from_ctx!(
                            TelemetryEvent::AgentModeChangedInputType {
                                input: None,
                                buffer_length,
                                is_manually_changed: false,
                                new_input_type,
                                active_block_id: me
                                    .model
                                    .lock()
                                    .block_list()
                                    .active_block_id()
                                    .clone(),
                                is_udi_enabled,
                            },
                            ctx
                        );
                    }
                },
            )
            .abort_handle();
        self.autodetect_abort_handle = Some(handle);
    }
}

#[derive(Debug, Clone)]
pub enum BlocklistAIInputEvent {
    /// Emitted when the terminal input type is updated.
    InputTypeChanged {
        /// The new input config.
        config: InputConfig,
    },
    /// Emitted when the input lock state is updated.
    LockChanged {
        /// The new input config.
        config: InputConfig,
    },
}

impl BlocklistAIInputEvent {
    pub fn did_update_input_config(&self) -> bool {
        match self {
            BlocklistAIInputEvent::InputTypeChanged { .. }
            | BlocklistAIInputEvent::LockChanged { .. } => true,
        }
    }

    pub fn updated_config(&self) -> &InputConfig {
        match self {
            BlocklistAIInputEvent::InputTypeChanged { config }
            | BlocklistAIInputEvent::LockChanged { config } => config,
        }
    }
}

impl Entity for BlocklistAIInputModel {
    type Event = BlocklistAIInputEvent;
}

/// GUI implementation of [`InputModePolicy`], extracted verbatim from the logic
/// [`BlocklistAIInputModel`] hardcoded before the policy seam existed: lock
/// permission and the autodetection setting both depend on the pane's
/// `AgentViewController` (fullscreen / active state) and, for lock permission,
/// on whether a CLI agent rich input session is open. This is a pure refactor —
/// it does not change GUI behavior.
///
/// `config_on_conversation_selection_changed` always returns `None` here. `new_inner` does now
/// subscribe to the surface's `ConversationSelection` and route its events at the policy, so the
/// hook is live -- but this policy deliberately opts out of it: agent-view enter/exit still
/// drives GUI config transitions via the `AgentViewController` subscription in `new_inner`,
/// unchanged from before this policy existed, and consuming both would apply each transition
/// twice. Migrating the GUI off the `AgentViewController` subscription and onto
/// `ConversationSelection` is a separate, larger change than #313 (which is scoped to lock
/// gating / initial config / AI-settings transitions).
pub(crate) struct GuiInputModePolicy {
    agent_view_controller: Option<ModelHandle<AgentViewController>>,
    terminal_view_id: EntityId,
}

impl GuiInputModePolicy {
    pub(crate) fn new(
        agent_view_controller: ModelHandle<AgentViewController>,
        terminal_view_id: EntityId,
    ) -> Self {
        Self {
            agent_view_controller: Some(agent_view_controller),
            terminal_view_id,
        }
    }
}

impl InputModePolicy for GuiInputModePolicy {
    fn initial_config(&self, app: &AppContext) -> InputConfig {
        let is_autodetection_enabled = if FeatureFlag::AgentView.is_enabled() {
            AISettings::as_ref(app).is_nld_in_terminal_enabled(app)
        } else {
            AISettings::as_ref(app).is_ai_autodetection_enabled(app)
        };
        InputConfig {
            input_type: InputType::Shell,
            is_locked: !is_autodetection_enabled,
        }
    }

    fn allows_locked_ai_input(&self, app: &AppContext) -> bool {
        // When `AgentView` is enabled, AI input mode can only be set in the top-level terminal
        // mode via autodetection; it cannot be locked to AI input mode unless there is an active
        // agent view or a CLI agent rich input session is open. In the agent view case, executing
        // autodetected AI input will trigger entering the agent view with that query. In the CLI
        // agent rich input case, the input must be in AI mode to suppress shell decorations
        // (syntax highlighting, error underlining).
        !FeatureFlag::AgentView.is_enabled()
            || self
                .agent_view_controller
                .as_ref()
                .is_none_or(|c| c.as_ref(app).is_active())
            || CLIAgentSessionsModel::as_ref(app).is_input_open(self.terminal_view_id)
    }

    fn is_autodetection_enabled(&self, app: &AppContext) -> bool {
        let ai_settings = AISettings::as_ref(app);
        if FeatureFlag::AgentView.is_enabled() {
            if self
                .agent_view_controller
                .as_ref()
                .is_some_and(|c| c.as_ref(app).is_fullscreen())
            {
                ai_settings.is_ai_autodetection_enabled(app)
            } else {
                ai_settings.is_nld_in_terminal_enabled(app)
            }
        } else {
            // AgentView not enabled: use the main autodetection setting
            ai_settings.is_ai_autodetection_enabled(app)
        }
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
        event: &AISettingsChangedEvent,
        current: InputConfig,
        is_autodetection_enabled_for_current_context: bool,
        app: &AppContext,
    ) -> Option<PolicyConfigUpdate> {
        match event {
            AISettingsChangedEvent::AIAutoDetectionEnabled { .. }
                if FeatureFlag::AgentView.is_enabled() =>
            {
                if self
                    .agent_view_controller
                    .as_ref()
                    .is_some_and(|c| c.as_ref(app).is_fullscreen())
                {
                    // Use context-specific check to determine if autodetection should be enabled
                    let is_nld_enabled = AISettings::as_ref(app).is_ai_autodetection_enabled(app);
                    // If autodetection is enabled, unlock the input.
                    Some(PolicyConfigUpdate::new(InputConfig {
                        is_locked: !is_nld_enabled,
                        input_type: InputType::AI,
                    }))
                } else {
                    None
                }
            }
            AISettingsChangedEvent::AIAutoDetectionEnabled { .. } => {
                // If autodetection is enabled, unlock the input.
                Some(PolicyConfigUpdate::new(InputConfig {
                    is_locked: !is_autodetection_enabled_for_current_context,
                    ..current
                }))
            }
            AISettingsChangedEvent::NLDInTerminalEnabled { .. }
                if FeatureFlag::AgentView.is_enabled()
                    && !self
                        .agent_view_controller
                        .as_ref()
                        .is_none_or(|c| c.as_ref(app).is_active()) =>
            {
                let is_nld_enabled = AISettings::as_ref(app).is_nld_in_terminal_enabled(app);
                Some(PolicyConfigUpdate::new(InputConfig {
                    is_locked: !is_nld_enabled,
                    input_type: InputType::Shell,
                }))
            }
            _ => None,
        }
    }
}

/// The source of the final input-type decision applied to the user input.
///
/// Ported from the pin (`app/src/ai/blocklist/input_model.rs`, `02b53fcd8`) for #312, but
/// narrowed to the single variant this fork can actually produce. The pin's version has ~30
/// variants tagging every autodetection decision path (manual toggle, shell prefix, denylist,
/// cloud handoff, etc.); threading a source through all of those is a separate, much larger
/// port (12 files, ~76 references) already tracked by #399/#254 item d and out of scope here.
/// `resolve_history_match` needs *a* value of this type to keep its pin-shaped return signature
/// (and the ported test assertions in `input_model_tests.rs` compare against it directly), so
/// this is the minimal faithful slice: the one variant `resolve_history_match` can return.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
enum InputTypeAutoDetectionSource {
    /// Buffer text closely matched a recent shell command history or agent
    /// prompt history entry.
    HistoryMatch,
}

/// One history source's match result for [`resolve_history_match`].
///
/// Ported verbatim from the pin (`02b53fcd8`) for #312.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HistoryMatch {
    /// No entry in this source closely matched the input.
    NoMatch,
    /// An entry matched, and its recency is known via `start_ts`.
    MatchedAt(DateTime<Local>),
    /// An entry matched, but the source has no timestamp for it (e.g. a command
    /// read from a shell history file such as `.zsh_history`).
    MatchedWithoutTimestamp,
}

/// Returns the [`HistoryMatch`] for the most-recent entry that closely matches
/// the provided word, using the given similarity threshold, or
/// [`HistoryMatch::NoMatch`] if no entry matches. The possibilities must be
/// ordered newest-first.
///
/// Adapted from [`difflib::get_close_matches`], but an async function with periodic
/// yields such that the operation can be aborted if necessary. Also, unlike the
/// original function, this returns as soon as it finds any match above the
/// threshold, instead of finding _all_ matches above the threshold and returning
/// the top N matches. Ported from the pin (`02b53fcd8`) for #312, replacing this
/// fork's earlier `has_any_close_matches` (which returned `bool` and carried no
/// timestamp, since it only ever considered command history).
async fn most_recent_close_match<'a>(
    word: &str,
    possibilities: impl Iterator<Item = (&'a str, Option<DateTime<Local>>)>,
    cutoff: f32,
) -> HistoryMatch {
    const BATCH_SIZE: usize = 50;

    if !(0.0..=1.0).contains(&cutoff) {
        panic!("Cutoff must be greater than 0.0 and lower than 1.0");
    }
    // #1604: the matcher indexes every byte of `word` when it is built (a 10 MB paste is a
    // 10 M-entry map), so it is built only once a candidate's length allows a match at all. The
    // length bound is the matcher's own `real_quick_ratio`: a candidate shorter than
    // `cutoff / (2 - cutoff)` of the buffer (or longer than its inverse) can never reach `cutoff`.
    let mut matcher = None;
    for (idx, (candidate, start_ts)) in possibilities.enumerate() {
        // Periodically, yield to the executor so this task can be aborted if
        // requested.
        if idx % BATCH_SIZE == 0 {
            futures_lite::future::yield_now().await;
        }

        if !close_match_possible_by_length(candidate.len(), word.len(), cutoff) {
            continue;
        }
        let matcher = matcher
            .get_or_insert_with(|| difflib::sequencematcher::SequenceMatcher::new("", word));
        matcher.set_first_seq(candidate);
        // The fast ratio computations produce an upper bound on the value of
        // ratio, so if a faster check fails, the slower checks are guaranteed
        // to also fail.
        if matcher.real_quick_ratio() >= cutoff && matcher.ratio() >= cutoff {
            return match start_ts {
                Some(ts) => HistoryMatch::MatchedAt(ts),
                None => HistoryMatch::MatchedWithoutTimestamp,
            };
        }
    }

    HistoryMatch::NoMatch
}

/// Whether two sequences of these lengths can be `cutoff`-similar at all (#1604): the matcher's
/// ratio is `2 * matched / (a + b)` with `matched <= min(a, b)`, so this is its `real_quick_ratio`
/// upper bound, computed before the matcher (and its index of the whole buffer) exists.
pub(crate) fn close_match_possible_by_length(a: usize, b: usize, cutoff: f32) -> bool {
    let total = a + b;
    if total == 0 {
        return true;
    }
    2.0 * a.min(b) as f32 / total as f32 >= cutoff
}

/// Fork-original (#832): the command-history entries that may route `buffer_text` to Shell by a
/// close match, as `(command, start_ts)` pairs in the input order.
///
/// A command-history match short-circuits the classifier, so a sentence that once ran in the
/// shell by mistake (`find the bug in main.rs`: `find` exits 1, not 127) used to route itself and
/// every near-variant to the shell from then on — and through the shell's history file, across
/// restarts. The rules:
///
/// - An entry that failed with "command not found" never matches (the pin's original filter).
/// - When the buffer [`reads_as_prose`](input_classifier::util::reads_as_prose) (the #808/#831
///   pre-filter's own vetoes: a function word or the article `a` after the head, sentence
///   punctuation, a trailing `?`, an apostrophe in a word...), only an entry that is known to
///   have **exited 0** and whose text is the buffer's **exactly** (ignoring runs of whitespace)
///   may match:
///   - A failed run is no evidence the line was a command, and neither is an entry with no exit
///     code. History-file lines this app ran get the exit code of their latest run back from the
///     persisted command table (`History`'s `persisted_commands_summary` join), so they are judged
///     like session commands; only a line the app never ran (typed in another terminal, or from
///     before the table existed) or one still running has none — and a mistake typed elsewhere
///     persists exactly that way.
///   - Exit 0 is strict on purpose: `was_successful` also accepts 130 (ctrl-c), and interrupting
///     a sentence that hung (`yes please`) proves nothing.
///   - Exact, not the 0.9 cutoff: a prose-shaped line that genuinely ran (`echo is it the cache`)
///     still re-runs from history, but its near-variants (`echo is it the caches`, or another
///     sentence one word away) go to the classifier instead of riding on it.
/// - A buffer with none of those prose signals keeps the original behavior, so re-running a real
///   command (`cargo test` that failed, `ssh host` from `.bash_history`) or a near-variant of one
///   still routes to Shell.
///
/// Known trade-off: shell syntax that shares those signals (`for i in ...; do ...; done`, `if
/// true; then ...; fi`, `grep -r for src`, `scp f.txt host:`, the quoted colon in `git commit -m
/// "feat: x"`) also reads as prose, so it needs an exact exit-0 entry for the history shortcut;
/// otherwise it goes to the classifier. Residual: command-headed prose with no veto word (`find
/// large files`, `install docker`, `free up disk space`) does not read as prose, so a failed
/// entry for it can still route it and its near-variants to Shell.
/// What the buffer's typeahead provenance says about Enter (#933).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnterProvenance {
    /// Typed in the editor, or typeahead that began right after a command.
    Classifiable,
    /// Typed ahead for a running command (#886): shell typeahead, decided by the shell rules.
    ShellTypeahead,
}

/// Whether an Enter must wait for the verdict of the detection requested for the exact buffer text
/// before it submits (#933). Text that reads as English must never be run by the shell on an
/// inherited input type because the verdict had not landed yet. Real commands wait only for the
/// classification itself, which the safety gate resolves to Shell.
pub fn should_wait_for_verdict_before_submit(
    has_prose_evidence: bool,
    is_slash_command: bool,
    verdict_pending: bool,
    provenance: EnterProvenance,
    gave_up_on_this_text: bool,
) -> bool {
    has_prose_evidence
        && !is_slash_command
        && verdict_pending
        && provenance == EnterProvenance::Classifiable
        && !gave_up_on_this_text
}

/// The most compute the jobs of a detection request may spend before an Enter that waits for the
/// verdict gives up on it (#1158, #1351).
///
/// This caps the classifier's OWN work (parse without generators, alias expansion, history match,
/// model), measured on the request's [`ComputeClock`] from when a job actually started running to
/// now. It is never measured from the Enter: a main-thread stall (frame, reflow, big paste) that
/// delays the Enter's handling of a verdict already computed and queued must not count against
/// the classifier. A verdict that lands, or is already queued, always decides the route. Past the
/// cap the classifier is slow and the Enter decides on the Enter-time rules that already ran
/// (known head, history, syntax, request shape) instead of holding: it is never kept in the editor
/// and never waits on I/O. It used to be 5 s, on a verdict that waited for remote completion
/// generators (0.8-3.8 s).
pub const ENTER_VERDICT_BUDGET: Duration = Duration::from_millis(20);

/// What a waiting Enter does when it is woken (#1158).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnterVerdictWait {
    /// The verdict for the exact buffer text has landed: submit on it.
    Submit,
    /// Within the compute cap: keep waiting for the verdict event.
    Wait,
    /// The classifier spent its compute cap: abandon it and submit on the Enter-time rules.
    SubmitWithoutVerdict,
}

/// Decides [`EnterVerdictWait`] from whether the verdict is still pending and how much compute
/// the classifier has spent on it. Never "keep in editor": a slow verdict is no reason to swallow
/// the Enter.
pub fn enter_verdict_wait_decision(
    verdict_pending: bool,
    classifier_compute: Duration,
) -> EnterVerdictWait {
    enter_verdict_wait_decision_for(verdict_pending, classifier_compute, 0)
}

/// [`enter_verdict_wait_decision`] for a buffer of `text_len` bytes: the compute cap is
/// [`enter_verdict_budget_for`] (#1604), so a 10 MB paste's linear classification is waited for
/// instead of handed back after the 20 ms a typed line gets.
pub fn enter_verdict_wait_decision_for(
    verdict_pending: bool,
    classifier_compute: Duration,
    text_len: usize,
) -> EnterVerdictWait {
    if !verdict_pending {
        EnterVerdictWait::Submit
    } else if classifier_compute >= enter_verdict_budget_for(text_len) {
        EnterVerdictWait::SubmitWithoutVerdict
    } else {
        EnterVerdictWait::Wait
    }
}

fn shell_history_candidates<'a>(
    entries: impl Iterator<Item = &'a HistoryEntry>,
    buffer_text: &str,
) -> Vec<(String, Option<DateTime<Local>>)> {
    // #965: a line the safety gate reads as a request to the agent (`say hello`, `where is the
    // config`, `watch the logs for errors`) is the agent's whatever the history holds. An entry
    // that ran it in the shell once (macOS speaks `say hello` and exits 0) must not pin the line
    // and its near-variants to the shell from then on: the gate would send the classifier verdict
    // to the agent, but a history match returns before the classifier is ever consulted.
    if input_classifier::util::reads_as_request_to_agent(buffer_text) {
        return Vec::new();
    }
    let buffer_reads_as_prose = input_classifier::util::reads_as_prose(buffer_text);
    entries
        // The same rule for the entry: a request-shaped line in history (`say hello`) must not pin
        // its own *prefixes* (`say hell` is 0.94 similar) to the shell while the user types it.
        // Those pins land before the full line's verdict does and are what an Enter that arrives
        // first routes on.
        .filter(|entry| !input_classifier::util::reads_as_request_to_agent(&entry.command))
        .filter(|entry| match entry.exit_code {
            Some(code) if code.was_command_not_found() => false,
            _ if !buffer_reads_as_prose => true,
            Some(code) if code.value() == 0 => entry
                .command
                .split_whitespace()
                .eq(buffer_text.split_whitespace()),
            _ => false,
        })
        .map(|entry| (entry.command.to_string(), entry.start_ts))
        .collect()
}

/// Resolves the history-match decision from the command-history and agent
/// prompt-history match results produced by [`most_recent_close_match`].
///
/// Returns `None` when neither source matched
/// - command-only match -> Shell
/// - prompt-only match -> AI
/// - both matched: the entry with the later timestamp wins. When the command
///   has no timestamp (e.g. a history-file entry) but the prompt does, the
///   prompt is treated as more recent (AI).
///
/// Ported verbatim from the pin (`02b53fcd8`) for #312.
fn resolve_history_match(
    command_match: HistoryMatch,
    prompt_match: HistoryMatch,
) -> Option<(InputType, InputTypeAutoDetectionSource)> {
    use HistoryMatch::{MatchedAt, MatchedWithoutTimestamp, NoMatch};

    match (command_match, prompt_match) {
        (NoMatch, NoMatch) => None,
        // Both sources matched: the entry with the later timestamp wins. When
        // the command has no timestamp (e.g. a shell history-file entry) but the
        // prompt does, the prompt is treated as more recent (AI). Without a
        // prompt timestamp we cannot prove the prompt is newer, so we preserve
        // the Shell short-circuit.
        (MatchedAt(command_ts), MatchedAt(prompt_ts)) => {
            if prompt_ts > command_ts {
                log::debug!("found match from prompt history at {prompt_ts:?}");
                Some((InputType::AI, InputTypeAutoDetectionSource::HistoryMatch))
            } else {
                log::debug!("found match from command history at {command_ts:?}");
                Some((InputType::Shell, InputTypeAutoDetectionSource::HistoryMatch))
            }
        }
        (MatchedWithoutTimestamp, MatchedAt(prompt_ts)) => {
            log::debug!("found match from prompt history at {prompt_ts:?}");
            Some((InputType::AI, InputTypeAutoDetectionSource::HistoryMatch))
        }
        (MatchedAt(_) | MatchedWithoutTimestamp, MatchedWithoutTimestamp) => {
            log::debug!("found match from command history");
            Some((InputType::Shell, InputTypeAutoDetectionSource::HistoryMatch))
        }
        // Command-only match -> Shell.
        (MatchedAt(_) | MatchedWithoutTimestamp, NoMatch) => {
            log::debug!("found match from command history");
            Some((InputType::Shell, InputTypeAutoDetectionSource::HistoryMatch))
        }
        // Prompt-only match -> AI.
        (NoMatch, MatchedAt(_) | MatchedWithoutTimestamp) => {
            log::debug!("found match from prompt history");
            Some((InputType::AI, InputTypeAutoDetectionSource::HistoryMatch))
        }
    }
}

/// The longest buffer the routing rules run on synchronously, on the UI thread (#1604): per
/// keystroke in [`BlocklistAIInputModel::detect_and_set_input_type`] and at Enter in
/// [`BlocklistAIInputModel::settle_pending_detection_for_submit`]. The rules are linear in the
/// text with a bounded amount of work per word, so 4 KiB is well under a millisecond; anything
/// longer is a paste, routed by the same rules in the detection job the Enter starts and waits
/// for. There is no size above which text is NOT classified: the former 4096-byte prefilter cap,
/// the 16 KiB #968 autodetect cap and the parse-cap abandon all sent an over-size shell command
/// to the agent in agent mode.
pub(crate) const MAX_SYNC_ROUTING_BYTES: usize = 4096;

/// Whether `buffer_text` is routed in the background job rather than on the UI thread (#1604).
pub(crate) fn routes_off_thread(buffer_text: &str) -> bool {
    buffer_text.len() > MAX_SYNC_ROUTING_BYTES
}

/// The first whitespace-separated word of `buffer_text`, read from its first
/// [`MAX_SYNC_ROUTING_BYTES`] bytes and cut to 256 chars (#1604): what the denylist and the
/// one-off-word shortcut need when no parse describes the buffer, at a cost bounded by the window,
/// not the text. `None` for a buffer whose window holds only whitespace.
pub(crate) fn head_word_from_text(buffer_text: &str) -> Option<String> {
    let mut end = buffer_text.len().min(MAX_SYNC_ROUTING_BYTES);
    while !buffer_text.is_char_boundary(end) {
        end -= 1;
    }
    buffer_text[..end]
        .split_whitespace()
        .next()
        .map(|word| word.chars().take(256).collect())
}

/// The longest buffer whose background routing job is started from the edit path, as the text is
/// edited (#1604). Every edit aborts the job in flight, but a job inside its linear `route` cannot
/// stop before it finishes, so a typing burst into a 10 MB buffer would stack up seconds of
/// background CPU (#1191's blocking-pool starvation); at 256 KiB a job is ~25 ms of compute. A
/// longer buffer is classified once, by the job its Enter starts, which the Enter waits for.
pub(crate) const MAX_EAGER_ROUTING_BYTES: usize = 256 * 1024;

/// Bytes of text per millisecond of extra classifier compute an Enter allows for a buffer over
/// [`MAX_SYNC_ROUTING_BYTES`] (#1604): 4 KiB/ms is a floor of 4 MB/s for the linear rules plus
/// the bounded model input, a few times slower than a release build runs them.
const LARGE_TEXT_BYTES_PER_MS: usize = 4096;

/// Extra compute the classifier is allowed on a buffer of `text_len` bytes before an Enter waiting
/// for its verdict gives up (#1604): nothing up to [`MAX_SYNC_ROUTING_BYTES`], then one
/// millisecond per [`LARGE_TEXT_BYTES_PER_MS`] bytes (a 1 MB paste gets ~250 ms more, 10 MB
/// ~2.5 s). The give-up never routes the line: with autodetection live it hands the text back
/// to the editor (#1577).
pub fn large_text_compute_allowance(text_len: usize) -> Duration {
    Duration::from_millis(
        (text_len.saturating_sub(MAX_SYNC_ROUTING_BYTES) / LARGE_TEXT_BYTES_PER_MS) as u64,
    )
}

/// [`ENTER_VERDICT_BUDGET`] plus the large-text allowance for a buffer of `text_len` bytes.
pub fn enter_verdict_budget_for(text_len: usize) -> Duration {
    ENTER_VERDICT_BUDGET + large_text_compute_allowance(text_len)
}

#[cfg(test)]
#[path = "input_model_tests.rs"]
mod tests;
