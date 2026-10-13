//! Model-layer AI input state management logic.
//!
//! The primary export of this module is `BlocklistAIInputModel`, which is a terminal pane-scoped
//! model managing input "type" state (whether the input is in AI or shell mode). This model also
//! exposes methods for running query autodetection, where an algorithm determines if the current
//! input contents are an AI query or shell command, which is then used to update the input mode.

use std::future::Future;
use std::sync::Arc;

use chrono::{DateTime, Local};
use futures::channel::oneshot;
use futures::stream::AbortHandle;
use input_classifier::util::{is_agent_follow_up_input, is_one_off_natural_language_word};
use instant::Instant;
use parking_lot::FairMutex;
use serde::{Deserialize, Serialize};
use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warpui::{AppContext, Entity, EntityId, ModelContext, ModelHandle, SingletonEntity};

pub use input_classifier::InputType;

use super::agent_view::{AgentViewController, AgentViewControllerEvent, AgentViewEntryOrigin};
use super::context_model::BlocklistAIContextModel;
use super::conversation_selection::{ConversationSelection, ConversationSelectionEvent};
use super::history_model::BlocklistAIHistoryModel;
#[cfg(any(test, feature = "test-util"))]
use super::conversation_selection::MockConversationSelection;
use super::input_mode_policy::{InputModePolicy, InputModePolicyHandle, PolicyConfigUpdate};
use crate::terminal::cli_agent_sessions::{
    CLIAgentInputState, CLIAgentSessionsModel, CLIAgentSessionsModelEvent,
};
use crate::PrivacySettings;
use warp_completer::completer::CompletionContext;

use crate::{
    input_classifier::InputClassifierModel,
    report_if_error, send_telemetry_from_ctx,
    settings::{AISettings, AISettingsChangedEvent, InputBoxType, InputSettings},
    terminal::{
        input::decorations::ParsedTokensSnapshot,
        model::{rich_content::RichContentType, session::SessionId},
        History, TerminalModel,
    },
    TelemetryEvent,
};

use super::telemetry_banner::should_collect_ai_ugc_telemetry;

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
    fn is_within_window(&self) -> bool {
        Instant::now()
            < self.at + std::time::Duration::from_millis(AUTODETECTION_DISABLE_DURATION_MS)
    }
}

/// Which steps of the detection pipeline run after the synchronous stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PipelineSteps {
    /// Shortcut, follow-up, history, alias expansion and the classifier (the background path).
    Full,
    /// As `Full`, but classify the unexpanded buffer (the Enter-time path; alias expansion can
    /// await the completer).
    ClassifierWithoutAliasExpansion,
    /// Only the steps that need nothing but the buffer text: shortcut, follow-up, history.
    TextOnly,
}

/// Result of [`BlocklistAIInputModel::settle_detection_synchronously`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncDetectionOutcome {
    /// Nothing to settle: no detection is pending for this buffer (autodetection locked or
    /// disabled, an explicit choice covers it, or its verdict already landed).
    NotPending,
    /// A verdict for this buffer was reached (and applied).
    Settled(InputType),
    /// The steps available synchronously did not decide; the input type was left alone.
    Undecided,
    /// The pipeline did not finish within the budget; the input type was left alone.
    BudgetExhausted { elapsed: std::time::Duration },
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

    /// Buffer text the in-flight async classification task was spawned for.
    in_flight_detection_buffer: Option<String>,

    /// Incremented for every spawned classification task, so a task that completed just before
    /// being replaced cannot apply its verdict (or clear the handle) over its replacement.
    detection_generation: u64,

    /// Notified whenever a detection settles (or is cancelled by an explicit write); see
    /// [`Self::detection_settled_receiver`]. Shared behind a mutex only so the model stays
    /// `Clone`; it is only ever touched on the main thread.
    detection_settled_waiters: Arc<parking_lot::Mutex<Vec<oneshot::Sender<()>>>>,

    /// Whether the input buffer was empty at the time the lock was set.  This will be true
    /// if a persistent lock is in place and a buffer is submitted.
    was_lock_set_with_empty_buffer: bool,

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
    model: Arc<FairMutex<TerminalModel>>,
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
            in_flight_detection_buffer: None,
            detection_generation: 0,
            detection_settled_waiters: Default::default(),
            was_lock_set_with_empty_buffer: false,
            agent_view_controller: None,
            ai_context_model,
            terminal_view_id,
            policy,
            autodetect_abort_handle: None,
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
                            me.temporarily_disable_autodetection();
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
            in_flight_detection_buffer: None,
            detection_generation: 0,
            detection_settled_waiters: Default::default(),
            was_lock_set_with_empty_buffer: false,
            autodetect_abort_handle: None,
            model,
        }
    }

    /// Applies a policy-produced config update.
    fn apply_policy_update(&mut self, update: PolicyConfigUpdate, ctx: &mut ModelContext<Self>) {
        if update.temporarily_disable_autodetection {
            self.temporarily_disable_autodetection();
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

    /// Swaps between Agent/Shell input types while preserving lock state. Temporarily disables
    /// autodetection.
    pub fn set_input_type(&mut self, input_type: InputType, ctx: &mut ModelContext<Self>) {
        self.temporarily_disable_autodetection();
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

        if new_config.input_type.is_ai() {
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
        self.temporarily_disable_autodetection();
        self.set_input_config_internal(new_config, ctx);
        if new_config.is_locked {
            self.abort_in_progress_detection();
        }
        self.was_lock_set_with_empty_buffer = self.is_input_type_locked() && is_input_buffer_empty;
    }

    /// Restores a previous input config without recomputing whether the lock was set while the
    /// buffer was empty.
    fn restore_input_config(
        &mut self,
        new_config: InputConfig,
        was_lock_set_with_empty_buffer: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        self.temporarily_disable_autodetection();
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
    fn temporarily_disable_autodetection(&mut self) {
        self.explicit_input_type_set = Some(ExplicitInputTypeSet {
            at: Instant::now(),
            buffer_text: None,
        });
        self.pending_detection_buffer = None;
        self.notify_detection_settled();
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
        explicit.buffer_text.as_deref() == Some(buffer_text)
    }

    /// Records that the view has requested input-type detection for `buffer_text` (an edit the
    /// user made), so a submission of that exact text can wait for the verdict (#833).
    pub fn note_detection_requested(&mut self, buffer_text: String) {
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
            Some((true, false)) | None => {}
        }
        self.pending_detection_buffer = Some(buffer_text);
    }

    /// Whether submitting `buffer_text` right now would race an unfinished input-type detection
    /// for it: autodetection is live (not locked, not disabled), the user's last edit requested a
    /// detection for exactly this text, and no verdict -- autodetected or explicit -- has settled
    /// it yet.
    ///
    /// An explicit write whose buffer is still unclaimed (no detection request has arrived since
    /// it) is treated as covering the current buffer: nothing has asked to reclassify it, and an
    /// explicit choice is never overridden at Enter.
    pub fn is_detection_pending_for(&self, buffer_text: &str, app: &AppContext) -> bool {
        if buffer_text.trim().is_empty() || !self.should_run_input_autodetection(app) {
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

    /// Whether the async classification task is currently running for `buffer_text`.
    pub fn is_detection_in_flight_for(&self, buffer_text: &str) -> bool {
        self.autodetect_abort_handle.is_some()
            && self.in_flight_detection_buffer.as_deref() == Some(buffer_text)
    }

    /// Returns a receiver that resolves the next time any detection settles (a verdict is applied
    /// or found unchanged, or an explicit write overrides it). Receivers are one-shot; callers
    /// re-check [`Self::is_detection_pending_for`] when it fires, because the detection that
    /// settled may have been for other text.
    pub fn detection_settled_receiver(&mut self) -> oneshot::Receiver<()> {
        let (tx, rx) = oneshot::channel();
        self.detection_settled_waiters.lock().push(tx);
        rx
    }

    fn notify_detection_settled(&mut self) {
        for waiter in self.detection_settled_waiters.lock().drain(..) {
            let _ = waiter.send(());
        }
    }

    /// Marks detection for `buffer_text` as settled: whatever the input type is now is the
    /// verdict for that text.
    fn settle_detection(&mut self, buffer_text: &str) {
        if self.pending_detection_buffer.as_deref() == Some(buffer_text) {
            self.pending_detection_buffer = None;
        }
        self.notify_detection_settled();
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
    }

    pub fn was_lock_set_with_empty_buffer(&self) -> bool {
        self.was_lock_set_with_empty_buffer
    }

    /// Aborts any in progress work for autodetection.
    pub fn abort_in_progress_detection(&mut self) {
        if let Some(handle) = self.autodetect_abort_handle.take() {
            handle.abort();
        }
        self.in_flight_detection_buffer = None;
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
        // Abort the last autodetect handle if exists.
        self.abort_in_progress_detection();

        let Some(first_token_str) = self.run_synchronous_detection_stage(&input, ctx) else {
            return;
        };

        let buffer_text = input.buffer_text.clone();
        let other_buffer_cloned = buffer_text.clone();
        self.detection_generation = self.detection_generation.wrapping_add(1);
        let detection_generation = self.detection_generation;
        let current_input_type = self.input_type();
        let is_udi_enabled = InputSettings::as_ref(ctx).is_universal_developer_input_enabled(ctx);
        let pipeline = self.detection_pipeline(
            input,
            first_token_str,
            completion_context,
            session_id,
            PipelineSteps::Full,
            ctx,
        );
        let handle = ctx
            .spawn(
                // With the classifier enabled the pipeline always reaches a verdict.
                async move { pipeline.await.unwrap_or(current_input_type) },
                move |me, new_input_type, ctx| {
                    // In theory, we shouldn't need to check this, as we only run autodetection if the input
                    // is not locked, and we should abort the autodetect future if the input is locked, but
                    // we do it anyway out of an abundance of caution.
                    if !me.should_run_input_autodetection(ctx) {
                        me.settle_detection(&other_buffer_cloned);
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
                    me.autodetect_abort_handle = None;
                    me.in_flight_detection_buffer = None;
                    me.apply_autodetected_input_type(new_input_type, ctx);
                    me.settle_detection(&other_buffer_cloned);
                    if current_input_type != new_input_type {
                        let buffer_length = other_buffer_cloned.len();
                        let input_buffer_text_for_telemetry = should_collect_ai_ugc_telemetry(
                            ctx,
                            PrivacySettings::as_ref(ctx).is_telemetry_enabled,
                        )
                        .then_some(other_buffer_cloned);
                        send_telemetry_from_ctx!(
                            TelemetryEvent::AgentModeChangedInputType {
                                input: input_buffer_text_for_telemetry,
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
        self.in_flight_detection_buffer = Some(buffer_text);
    }

    /// Settles a pending detection for `buffer_text` right now, on the calling (UI) thread, for
    /// an Enter keypress that would otherwise route on the previous buffer's verdict (#833, F6).
    ///
    /// Inputs available synchronously, and therefore the only ones used:
    /// - `described`: the view's completed parse, if it is for exactly `buffer_text`. It carries
    ///   the completer's token descriptions, so the classifier can run on it.
    /// - `undescribed`: a description-less tokenization of `buffer_text`
    ///   ([`warp_completer::util::parse_current_commands_and_tokens_without_descriptions`]), used
    ///   when there is no matching parse. Producing descriptions runs the completer, which can
    ///   await filesystem listings and generator commands, so it is never done here.
    ///
    /// Steps, in the background path's order:
    /// 1. lock / explicit-window / empty checks, the denylist and the #808 pre-filter (token text
    ///    only);
    /// 2. the one-off natural-language shortcut, the agent follow-up check and the command /
    ///    prompt history match (buffer text only);
    /// 3. the classifier -- only with `described`, and not when the first token is an alias:
    ///    alias expansion re-parses through the completer, and classifying the unexpanded alias
    ///    would be guessing. Without descriptions the classifier is skipped entirely: it reads
    ///    the token descriptions as command evidence, so an undescribed `git status` would look
    ///    like prose.
    ///
    /// Steps 2-3 are the background pipeline's own future, polled here with a no-op waker until
    /// it resolves or `budget` elapses (the pipeline only yields between steps; a single
    /// classifier poll is not interruptible, so `elapsed` can exceed `budget`).
    ///
    /// On a verdict the background task for this buffer is aborted and the verdict applied and
    /// settled, exactly as the task would have. Otherwise the input type is left alone -- the
    /// caller routes in the current mode, as before #833 -- and the background task keeps
    /// running.
    pub fn settle_detection_synchronously<C: CompletionContext + Clone + Send + 'static>(
        &mut self,
        buffer_text: &str,
        described: Option<ParsedTokensSnapshot>,
        undescribed: ParsedTokensSnapshot,
        completion_context: C,
        session_id: Option<SessionId>,
        budget: std::time::Duration,
        ctx: &mut ModelContext<Self>,
    ) -> SyncDetectionOutcome {
        let started = Instant::now();
        if !self.is_detection_pending_for(buffer_text, ctx) {
            return SyncDetectionOutcome::NotPending;
        }
        let has_descriptions = described
            .as_ref()
            .is_some_and(|described| described.buffer_text == buffer_text);
        let input = match described {
            Some(described) if has_descriptions => described,
            _ => undescribed,
        };
        debug_assert_eq!(input.buffer_text, buffer_text);

        let Some(first_token_str) = self.run_synchronous_detection_stage(&input, ctx) else {
            // Settled (or nothing to decide). The background task for this text is moot.
            self.abort_in_progress_detection();
            return SyncDetectionOutcome::Settled(self.input_type());
        };

        let first_token_is_alias = input
            .parsed_tokens
            .iter()
            .filter(|token| token.token_index == 0)
            .any(|token| {
                completion_context
                    .alias_command(token.token.as_str())
                    .is_some()
            });
        let steps = if has_descriptions && !first_token_is_alias {
            PipelineSteps::ClassifierWithoutAliasExpansion
        } else {
            PipelineSteps::TextOnly
        };
        let pipeline = self.detection_pipeline(
            input,
            first_token_str,
            completion_context,
            session_id,
            steps,
            ctx,
        );
        let mut pipeline = std::pin::pin!(pipeline);
        let waker = futures::task::noop_waker();
        let mut poll_context = std::task::Context::from_waker(&waker);
        let verdict = loop {
            match pipeline.as_mut().poll(&mut poll_context) {
                std::task::Poll::Ready(verdict) => break Some(verdict),
                std::task::Poll::Pending if started.elapsed() >= budget => break None,
                std::task::Poll::Pending => {}
            }
        };
        match verdict {
            Some(Some(input_type)) => {
                self.abort_in_progress_detection();
                self.apply_autodetected_input_type(input_type, ctx);
                self.settle_detection(buffer_text);
                SyncDetectionOutcome::Settled(input_type)
            }
            Some(None) => SyncDetectionOutcome::Undecided,
            None => SyncDetectionOutcome::BudgetExhausted {
                elapsed: started.elapsed(),
            },
        }
    }

    /// The synchronous stage of detection, shared by the background path
    /// ([`Self::detect_and_set_input_type`]) and the Enter-time path
    /// ([`Self::settle_detection_synchronously`]): lock / explicit-window / empty-buffer checks,
    /// the denylist and the #808 pre-filter. Every one of these needs only token *text*.
    ///
    /// Returns `None` when this stage settled the buffer (applying a verdict, or leaving the type
    /// alone for a reason that settles it); otherwise the first token, for the pipeline.
    fn run_synchronous_detection_stage(
        &mut self,
        input: &ParsedTokensSnapshot,
        ctx: &mut ModelContext<Self>,
    ) -> Option<String> {
        // If the input mode is locked, there's no point in running autodetection.
        if !self.should_run_input_autodetection(ctx) {
            self.settle_detection(&input.buffer_text);
            return None;
        }

        // An explicit write in the last 250 ms keeps its input type -- for the buffer it was made
        // for only; new text is classified (#833).
        if self.is_detection_suppressed_by_explicit_set(&input.buffer_text) {
            self.settle_detection(&input.buffer_text);
            return None;
        }
        if self
            .explicit_input_type_set
            .as_ref()
            .is_some_and(|explicit| !explicit.is_within_window())
        {
            self.explicit_input_type_set = None;
        }

        let first_token_str = input.parsed_tokens.first().map(|t| t.token.item.clone());
        let Some(first_token_str) = first_token_str else {
            // If the buffer is empty, short-circuit and leave input type unchanged.
            //
            // We don't know enough (anything) to be able to change the input type one way or
            // another.
            self.settle_detection(&input.buffer_text);
            return None;
        };

        let denylist: Vec<&str> = AISettings::as_ref(ctx)
            .autodetection_command_denylist
            .value()
            .split(',')
            .collect();

        // Early return if the first token is included in the denylist. No need to parse history.
        if denylist.contains(&first_token_str.as_str()) {
            self.apply_autodetected_input_type(InputType::Shell, ctx);
            self.settle_detection(&input.buffer_text);
            return None;
        }

        // #808: if the buffer is unambiguously a shell command line (`yes | head -c 50000000 >
        // /dev/null`, `trap 'echo t' DEBUG`), force Shell right here, synchronously, before the
        // one-off natural-language shortcut, the history matcher and the classifier get a say.
        // The ONNX model routed both of those to the agent (and with no reachable model the user
        // got a stream error instead of their command); the rules are precision-first, so
        // anything merely *probably* a command still goes through the pipeline below. See
        // `input_classifier::util::is_unambiguous_shell_command`.
        if input_classifier::util::is_unambiguous_shell_command(input) {
            self.apply_autodetected_input_type(InputType::Shell, ctx);
            self.settle_detection(&input.buffer_text);
            return None;
        }

        Some(first_token_str)
    }

    /// Builds the asynchronous part of detection: the one-off natural-language shortcut, the
    /// agent follow-up check, the command/prompt history match and then, depending on `steps`,
    /// alias expansion and the classifier. Resolves to `None` only when `steps` excludes the
    /// classifier and no earlier step decided.
    ///
    /// Everything before the classifier needs only the buffer text; the classifier relies on the
    /// completer's token descriptions in `input`. The future does no I/O itself (it yields
    /// between steps so it can be aborted), with one exception: alias expansion re-parses an
    /// expanded buffer through the completer, which can await filesystem and generator work.
    fn detection_pipeline<C: CompletionContext + Clone + Send + 'static>(
        &mut self,
        input: ParsedTokensSnapshot,
        first_token_str: String,
        completion_context: C,
        session_id: Option<SessionId>,
        steps: PipelineSteps,
        ctx: &mut ModelContext<Self>,
    ) -> impl Future<Output = Option<InputType>> + 'static {
        // If we have a session, gather command history entries. `History::commands`
        // returns entries in ascending (oldest-first) order, so downstream matching
        // reverses them to iterate newest-first.
        let history_entries = session_id.map(|sid| {
            History::as_ref(ctx)
                .commands(sid)
                .into_iter()
                .flatten()
                .filter_map(|entry| {
                    if entry
                        .exit_code
                        .is_some_and(|code| code.was_command_not_found())
                    {
                        return None;
                    }
                    Some((entry.command.to_string(), entry.start_ts))
                })
                .collect::<Vec<(String, Option<DateTime<Local>>)>>()
        });

        // Gather agent prompt history (ascending / oldest-first). Ported from the pin
        // (`02b53fcd8`) for #312; off by default, matching the pin's own default-off
        // state (see `FeatureFlag::NldPromptHistoryMatch`'s doc comment).
        let prompt_entries = FeatureFlag::NldPromptHistoryMatch
            .is_enabled()
            .then(|| BlocklistAIHistoryModel::as_ref(ctx).prompt_history_candidates());

        let buffer_cloned = input.buffer_text.clone();
        let current_input_type = self.input_type();
        // The one-off natural-language shortcut asks "is the user already talking to the agent?".
        // Answer it from the last input type the user (or a flow acting for them) chose, not the
        // live one: the live type is whatever autodetection guessed for the previous keystroke, so
        // it made the verdict for the final text depend on typing speed (#833, F8).
        let shortcut_input_type = self.non_autodetected_input_type;

        // Determine if the input is a follow-up to an AI block.
        let is_agent_follow_up = {
            let model = self.model.lock();
            let block_list = model.block_list();
            let block_index = block_list.last_non_hidden_block_by_index();
            match block_list.last_non_hidden_rich_content_block_after_block(block_index) {
                Some((_, content)) => content.content_type == Some(RichContentType::AIBlock),
                _ => false,
            }
        };

        let classifier = InputClassifierModel::as_ref(ctx).classifier();
        async move {
            // First check if the token is a natural language word, if the user was already
            // in AI mode before this buffer's autodetection began.
            if matches!(shortcut_input_type, InputType::AI)
                && is_one_off_natural_language_word(first_token_str.to_lowercase().as_str())
            {
                return Some(InputType::AI);
            }

            // If this is clearly intended to be a follow-up to an AI block, classify it as AI.
            if is_agent_follow_up && is_agent_follow_up_input(&buffer_cloned.trim().to_lowercase())
            {
                return Some(InputType::AI);
            }

            // If we have history entries (i.e., a live session), check for
            // close matches against command history and agent prompt history.
            // TODO(vorporeal): decide if we still want to do this with NldImprovements.
            if let Some(history_entries) = history_entries {
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
                        return Some(input_type);
                    }
                } else if let Some((input_type, _source)) =
                    resolve_history_match(command_match, HistoryMatch::NoMatch)
                {
                    // Feature disabled: preserve the command-only behavior.
                    return Some(input_type);
                }
            }

            if steps == PipelineSteps::TextOnly {
                return None;
            }

            // Yield so that an attempt to abort the classification is handled.  We do this periodically
            // so that we can skip doing additional expensive work if the classification is aborted.
            futures_lite::future::yield_now().await;

            let input = if steps == PipelineSteps::Full {
                warp_completer::util::expand_aliases(input, &completion_context).await
            } else {
                input
            };

            futures_lite::future::yield_now().await;

            let context = input_classifier::Context {
                current_input_type,
                is_agent_follow_up,
                commands_fully_loaded: completion_context.top_level_commands_fully_loaded(),
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

            Some(new_input_type)
        }
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
    let mut matcher = difflib::sequencematcher::SequenceMatcher::new("", word);
    for (idx, (candidate, start_ts)) in possibilities.enumerate() {
        // Periodically, yield to the executor so this task can be aborted if
        // requested.
        if idx % BATCH_SIZE == 0 {
            futures_lite::future::yield_now().await;
        }

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

#[cfg(test)]
#[path = "input_model_tests.rs"]
mod tests;
