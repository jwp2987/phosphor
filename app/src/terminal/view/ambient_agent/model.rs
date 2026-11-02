use instant::Instant;
use warp_cli::agent::Harness;
use warpui::{Entity, EntityId, ModelContext};

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::ambient_agents::{AmbientAgentTaskId, SpawnAgentRequest};

use super::AmbientAgentProgressUIState;

/// Tracks progress timestamps for each step during ambient agent spawning.
#[derive(Debug, Clone)]
pub struct AgentProgress {
    /// When the agent run was requested.
    pub spawned_at: Instant,
    /// When the run was claimed by a worker.
    pub claimed_at: Option<Instant>,
    /// When the agent harness began executing.
    pub harness_started_at: Option<Instant>,
    /// When the agent stopped.
    pub stopped_at: Option<Instant>,
}

/// Status of the ambient agent run.
#[derive(Debug, Clone)]
pub enum Status {
    /// Not in an ambient agent session.
    NotAmbientAgent,
    /// First-time ambient-agent setup.
    Setup,
    /// The user is composing their ambient agent prompt.
    Composing,
    /// Waiting for the ambient agent run to be ready.
    WaitingForSession { progress: AgentProgress },
    /// The agent is running and the session is ready.
    AgentRunning,
    /// The agent failed.
    Failed {
        progress: AgentProgress,
        error_message: String,
    },
    /// The user needs to authenticate with GitHub.
    NeedsGithubAuth {
        progress: AgentProgress,
        error_message: String,
        auth_url: String,
    },
    /// The agent was cancelled.
    Cancelled { progress: AgentProgress },
}

/// Model to track the state of an ambient agent run.
pub struct AmbientAgentViewModel {
    status: Status,

    /// The request with which the ambient agent was spawned, if it was spawned.
    request: Option<SpawnAgentRequest>,

    /// The terminal view this model is part of.
    terminal_view_id: EntityId,

    /// Whether this ambient agent view has a parent terminal view to return to.
    /// `false` for standalone views.
    /// `true` for nested views (pushed onto an existing terminal's pane stack).
    has_parent_terminal: bool,

    /// UI state for rendering the ambient agent progress screen.
    pub ui_state: AmbientAgentProgressUIState,

    /// The task ID for the current ambient-agent task, if one has been spawned.
    task_id: Option<AmbientAgentTaskId>,

    /// The local conversation associated with this ambient-agent run, if any.
    /// Set for remote child agents spawned via `start_agent` so the `run_id`
    /// from the server response can be wired back to the conversation.
    conversation_id: Option<AIConversationId>,

    /// Selected execution harness for the ambient-agent run.
    /// Defaults to `Harness::Oz`. Used to populate `AgentConfigSnapshot.harness` on spawn.
    harness: Harness,
    /// Whether the optimistic InitialUserQuery block has been inserted for the current run.
    has_inserted_ambient_agent_user_query_block: bool,
    /// Whether the harness CLI (e.g. `claude`, `gemini`) has started running for a non-oz run.
    /// Used to transition the ambient-agent setup UI out of the pre-first-exchange phase when
    /// there is no oz `AppendedExchange` to key off of.
    harness_command_started: bool,
}

impl AmbientAgentViewModel {
    pub fn new(
        terminal_view_id: EntityId,
        has_parent_terminal: bool,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        let ui_state = AmbientAgentProgressUIState::new(ctx);

        Self {
            status: Status::NotAmbientAgent,
            request: None,
            terminal_view_id,
            has_parent_terminal,
            ui_state,
            task_id: None,
            conversation_id: None,
            harness: Harness::default(),
            has_inserted_ambient_agent_user_query_block: false,
            harness_command_started: false,
        }
    }

    pub fn request(&self) -> Option<&SpawnAgentRequest> {
        self.request.as_ref()
    }

    pub fn selected_harness(&self) -> Harness {
        self.harness
    }

    pub fn set_harness(&mut self, harness: Harness, ctx: &mut ModelContext<Self>) {
        if self.harness == harness {
            return;
        }
        self.harness = harness;
        ctx.emit(AmbientAgentViewModelEvent::HarnessSelected);
    }

    /// Whether the harness CLI has started running. Only meaningful for non-oz runs.
    pub(super) fn harness_command_started(&self) -> bool {
        self.harness_command_started
    }

    /// Whether or not this terminal session is for an ambient agent.
    pub fn is_ambient_agent(&self) -> bool {
        !matches!(self.status, Status::NotAmbientAgent)
    }

    /// Returns the task ID for the current ambient-agent task, if one has been spawned.
    pub fn task_id(&self) -> Option<AmbientAgentTaskId> {
        self.task_id
    }

    pub fn has_inserted_ambient_agent_user_query_block(&self) -> bool {
        self.has_inserted_ambient_agent_user_query_block
    }

    /// Returns whether this ambient agent view has a parent terminal to return to.
    pub fn has_parent_terminal(&self) -> bool {
        self.has_parent_terminal
    }

    /// Sets whether this ambient agent view has a parent terminal to return to.
    /// This should be called when pushing the view onto an existing pane stack.
    pub fn set_has_parent_terminal(&mut self, has_parent: bool) {
        self.has_parent_terminal = has_parent;
    }

    /// Whether or not this terminal session is in the setup state (first-time environment creation).
    pub fn is_in_setup(&self) -> bool {
        matches!(self.status, Status::Setup)
    }

    /// Whether or not this terminal session is currently setting up an ambient agent run.
    pub fn is_configuring_ambient_agent(&self) -> bool {
        matches!(self.status, Status::Composing)
    }

    /// Whether or not this terminal session is waiting for an ambient agent session to be ready.
    pub fn is_waiting_for_session(&self) -> bool {
        matches!(self.status, Status::WaitingForSession { .. })
    }

    /// Whether or not the ambient agent failed to spawn.
    pub fn is_failed(&self) -> bool {
        matches!(self.status, Status::Failed { .. })
    }

    /// Whether or not the ambient agent was cancelled.
    pub fn is_cancelled(&self) -> bool {
        matches!(self.status, Status::Cancelled { .. })
    }

    /// Whether or not the ambient agent needs GitHub authentication.
    pub fn is_needs_github_auth(&self) -> bool {
        matches!(self.status, Status::NeedsGithubAuth { .. })
    }

    /// Whether or not we should show a status footer (loading, error, auth, or cancelled).
    pub fn should_show_status_footer(&self) -> bool {
        if false {
            return false;
        }

        self.is_waiting_for_session()
            || self.is_failed()
            || self.is_needs_github_auth()
            || self.is_cancelled()
    }

    /// Returns the error message if the agent is in a failed state.
    pub fn error_message(&self) -> Option<&str> {
        match &self.status {
            Status::Failed { error_message, .. } => Some(error_message),
            _ => None,
        }
    }

    /// Enter the setup state for first-time environment creation.
    pub fn enter_setup(&mut self, ctx: &mut ModelContext<Self>) {
        self.status = Status::Setup;
        ctx.emit(AmbientAgentViewModelEvent::EnteredSetupState);
    }

    /// Transition from Setup to Composing state.
    pub fn enter_composing_from_setup(&mut self, ctx: &mut ModelContext<Self>) {
        self.status = Status::Composing;
        ctx.emit(AmbientAgentViewModelEvent::EnteredComposingState);
    }

    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Reset the status back to NotAmbientAgent.
    pub fn reset_status(&mut self, ctx: &mut ModelContext<Self>) {
        self.status = Status::NotAmbientAgent;
        self.task_id = None;
        self.conversation_id = None;
        self.has_inserted_ambient_agent_user_query_block = false;
        self.harness_command_started = false;
        ctx.notify();
    }

    /// Sets the local conversation ID associated with this ambient-agent run.
    pub fn set_conversation_id(&mut self, id: Option<AIConversationId>) {
        self.conversation_id = id;
    }
}

/// Events emitted by the ambient agent view model.
#[derive(Debug, Clone)]
pub enum AmbientAgentViewModelEvent {
    /// The user has entered the setup state (first-time environment creation).
    EnteredSetupState,
    /// The user has entered the composing state (typing their prompt).
    EnteredComposingState,
    /// The ambient agent run has been dispatched.
    DispatchedAgent,
    /// The spawn progress has been updated (e.g., task claimed or in-progress).
    ProgressUpdated,
    /// The ambient agent has started sharing its session.
    SessionReady,
    /// The ambient agent failed.
    Failed { error_message: String },
    /// Request to show the agent credits modal.
    ShowAICreditModal,
    /// The ambient agent needs GitHub authentication.
    NeedsGithubAuth,
    /// The ambient agent was cancelled.
    Cancelled,
    /// The selected execution harness (Oz / Claude Code) changed.
    HarnessSelected,
    /// The harness CLI (for non-oz runs) has started executing in the shared session.
    /// Fires once per run and signals the transition out of the pre-first-exchange phase
    /// for claude / gemini / other third-party harnesses.
    HarnessCommandStarted,
}

impl Entity for AmbientAgentViewModel {
    type Event = AmbientAgentViewModelEvent;
}
