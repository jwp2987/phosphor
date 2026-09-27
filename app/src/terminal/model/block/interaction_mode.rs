use anyhow::anyhow;
use warp_terminal::model::{grid::Dimensions, Point};

use crate::{
    ai::{
        agent::{conversation::AIConversationId, task::TaskId, AIAgentActionId},
        blocklist::block::cli_controller::{LongRunningCommandControlState, UserTakeOverReason},
    },
    terminal::{
        event::Event,
        model::{
            grid::{grid_handler::GridHandler, RespectDisplayedOutput},
            RespectObfuscatedSecrets,
        },
    },
};

use super::{Block, SerializedAIMetadata};

impl Block {
    /// `true` if the command is executing and the user has opened the agent mode input.
    ///
    /// "tagged in" means that the agent mode input should be shown, but control has yet
    /// to be passed to the agent.
    pub fn is_agent_tagged_in(&self) -> bool {
        if !self.is_active_and_long_running() {
            return false;
        }

        match &self.interaction_mode {
            InteractionMode::User(user_mode) => user_mode.did_user_tag_in_agent,
            _ => false,
        }
    }

    /// `true` if the command is eligible for tagging in the agent (e.g. showing the agent mode
    /// input and sending a query to trigger the CLI subagent).
    ///
    /// Notably, this is NOT true if the subagent had already taken control of the command, and
    /// the user took control back from the subagent.
    ///
    /// See doc comment on `InteractionMode` for explanation on the semantics of 'tagged in',
    /// 'agent in control', 'user take control, and 'agent handoff'.
    pub fn is_eligible_to_tag_in_agent(&self) -> bool {
        if !self.is_active_and_long_running()
            || self.is_in_band_command_block()
            || !self.bootstrap_stage.is_bootstrapped()
            || self.env_var_metadata().is_some()
        {
            return false;
        }

        match &self.interaction_mode {
            InteractionMode::User(user_mode) => !user_mode.did_user_tag_in_agent,
            _ => false,
        }
    }

    pub fn set_is_agent_tagged_in(&mut self, value: bool) {
        if let InteractionMode::User(UserMode {
            did_user_tag_in_agent,
        }) = &mut self.interaction_mode
        {
            if *did_user_tag_in_agent != value {
                *did_user_tag_in_agent = value;
                self.event_proxy
                    .send_terminal_event(Event::AgentTaggedInChanged {
                        is_tagged_in: value,
                    });
            }
        }
    }

    /// Returns `true` if an agent is monitoring/interacting with this command.
    pub fn is_agent_monitoring(&self) -> bool {
        self.is_active_and_long_running() && self.long_running_control_state().is_some()
    }

    /// Returns `true` if the agent is either in control or has been tagged in by the user.
    pub fn is_agent_in_control_or_tagged_in(&self) -> bool {
        self.is_agent_in_control() || self.is_agent_tagged_in()
    }

    pub fn cli_subagent_task_id(&self) -> Option<&TaskId> {
        self.agent_interaction_metadata()
            .and_then(|metadata| metadata.subagent_task_id())
    }

    pub fn upgrade_cli_subagent_task_id(&mut self, new_task_id: TaskId) -> anyhow::Result<()> {
        if let InteractionMode::Agent(AgentInteractionMetadata {
            subagent_task_id: Some(task_id),
            ..
        }) = &mut self.interaction_mode
        {
            *task_id = new_task_id;
            Ok(())
        } else {
            Err(anyhow!("Tried to upgrade CLI subagent task ID for block with no prior CLI subagent task ID."))
        }
    }

    /// Returns `true` if this command is active and the agent is in control.
    pub fn is_agent_in_control(&self) -> bool {
        self.is_active_and_long_running()
            && self
                .long_running_control_state()
                .is_some_and(LongRunningCommandControlState::is_agent_in_control)
    }

    /// Returns `true` if the agent is actively driving this command.
    ///
    /// This is broader than `is_agent_in_control`: it also covers the window between
    /// when the agent writes an agent-requested command to the PTY (synchronous) and
    /// when the CLI subagent is later spawned and `long_running_control_state` is set
    /// (asynchronous, via `BlocklistAIHistoryEvent::CreatedSubtask`). Returns `false`
    /// once the user takes over, even for agent-initiated commands.
    pub fn is_agent_driving_command(&self) -> bool {
        if self.is_agent_in_control() {
            return true;
        }
        // Agent-initiated command where the CLI subagent hasn't formally taken control yet.
        self.interaction_mode
            .agent_interaction_metadata()
            .is_some_and(|metadata| {
                metadata.requested_command_action_id().is_some()
                    && metadata.long_running_control_state().is_none()
            })
    }

    /// Returns `true` if the agent's interaction with this command is currently blocked by user
    /// approval.
    pub fn is_agent_blocked(&self) -> bool {
        self.is_active_and_long_running()
            && self
                .long_running_control_state()
                .is_some_and(LongRunningCommandControlState::is_agent_blocked)
    }

    /// Returns `true` if the command is eligible to be handed off to an agent.
    pub fn is_eligible_for_agent_handoff(&self) -> bool {
        self.is_active_and_long_running()
            && self
                .long_running_control_state()
                .is_some_and(LongRunningCommandControlState::is_user_in_control)
    }

    pub fn update_is_agent_blocked(&mut self, new_value: bool) {
        if let InteractionMode::Agent(AgentInteractionMetadata {
            long_running_control_state:
                Some(LongRunningCommandControlState::Agent {
                    ref mut is_blocked, ..
                }),
            ..
        }) = self.interaction_mode
        {
            *is_blocked = new_value;
        }
    }

    /// Hands control to the user with a non-resuming `Stop`. Used by teardown paths (rewind,
    /// stop) where the conversation has been cancelled and must not resume when the command
    /// completes.
    ///
    /// An agent-requested command with no control state yet (before its first snapshot --
    /// the whole life of a `wait_until_completion` command) gets the `Stop` installed too,
    /// not just rewritten. Leaving it `None` left the teardown with no durable record:
    /// `is_agent_driving_command` stayed true, so if the process survived the Ctrl-C (a REPL)
    /// a later password prompt could still take it over with `BlockedOnInput` -- which the
    /// `None` arm of `take_over_for_user` admits -- and that reason's `should_auto_resume()`
    /// then resumed the conversation the user had just cancelled once the command completed.
    /// With `Stop` in place the take-over is refused, as it is for any user-held state.
    pub fn set_user_control_with_stop_reason(&mut self) {
        let InteractionMode::Agent(AgentInteractionMetadata {
            requested_command_action_id,
            long_running_control_state,
            ..
        }) = &mut self.interaction_mode
        else {
            return;
        };
        if long_running_control_state.is_none() && requested_command_action_id.is_none() {
            return;
        }
        *long_running_control_state = Some(LongRunningCommandControlState::User {
            reason: UserTakeOverReason::Stop {
                should_auto_resume: false,
            },
        });
    }

    /// Returns `true` if agent responses should be hidden in the UI.
    pub fn should_hide_responses(&self) -> bool {
        self.is_active_and_long_running()
            && self
                .long_running_control_state()
                .is_some_and(LongRunningCommandControlState::should_hide_responses)
    }

    /// Returns the `agent_interaction_metadata` associated with this block, if any.
    pub fn agent_interaction_metadata(&self) -> Option<&AgentInteractionMetadata> {
        self.interaction_mode.agent_interaction_metadata()
    }

    pub fn ai_conversation_id(&self) -> Option<AIConversationId> {
        match &self.interaction_mode {
            InteractionMode::Agent(metadata) => Some(metadata.conversation_id),
            _ => None,
        }
    }

    pub fn requested_command_action_id(&self) -> Option<&AIAgentActionId> {
        match &self.interaction_mode {
            InteractionMode::Agent(metadata) => metadata.requested_command_action_id(),
            _ => None,
        }
    }

    pub fn is_agent_requested_command(&self) -> bool {
        self.requested_command_action_id().is_some()
    }

    /// Returns the `long_running_control_state` associated with this block, if any.
    pub fn long_running_control_state(&self) -> Option<&LongRunningCommandControlState> {
        self.interaction_mode.long_running_control_state()
    }

    pub fn has_agent_written_to_block(&self) -> bool {
        self.interaction_mode
            .agent_interaction_metadata()
            .is_some_and(|metadata| metadata.has_agent_written_to_block())
    }

    pub fn mark_agent_written_to_block(&mut self) {
        if let InteractionMode::Agent(metadata) = &mut self.interaction_mode {
            metadata.has_agent_written_to_block = true;
        }
    }

    pub fn set_should_hide(&mut self, value: bool) {
        self.interaction_mode.set_should_hide_block(value);
    }

    pub fn set_agent_interaction_mode_for_requested_command(
        &mut self,
        requested_command_action_id: AIAgentActionId,
        subagent_task_id: Option<TaskId>,
        conversation_id: AIConversationId,
    ) {
        self.interaction_mode = InteractionMode::Agent(AgentInteractionMetadata {
            requested_command_action_id: Some(requested_command_action_id),
            conversation_id,
            subagent_task_id,
            long_running_control_state: None,
            has_agent_written_to_block: false,
            should_hide_block: true,
        })
    }

    pub fn set_agent_interaction_mode_for_agent_monitored_command(
        &mut self,
        task_id: &TaskId,
        conversation_id: AIConversationId,
    ) -> Result<(), UpdateInteractionModeError> {
        let new_mode = self
            .interaction_mode
            .to_agent_monitored(task_id, conversation_id)?;
        self.interaction_mode = new_mode;
        Ok(())
    }

    pub fn set_agent_interaction_mode(
        &mut self,
        agent_interaction_metadata: AgentInteractionMetadata,
    ) {
        self.interaction_mode = InteractionMode::new_agent(agent_interaction_metadata);
    }

    pub fn set_interaction_mode_from_serialized_ai_metadata(
        &mut self,
        serialized_metadata: SerializedAIMetadata,
    ) {
        self.interaction_mode = InteractionMode::from_serialized_ai_metadata(serialized_metadata);
    }

    pub fn take_over_control_for_user(
        &mut self,
        reason: UserTakeOverReason,
    ) -> Result<(), UpdateInteractionModeError> {
        self.interaction_mode.take_over_for_user(reason)
    }

    pub fn handoff_control_to_agent(&mut self) -> Result<(), UpdateInteractionModeError> {
        self.interaction_mode.handoff_to_agent()
    }

    /// Returns true if the interaction mode is agent-monitored and subagent response visibility was actually toggled.
    pub fn toggle_subagent_response_visibility(&mut self) -> bool {
        match &mut self.interaction_mode {
            InteractionMode::Agent(AgentInteractionMetadata {
                long_running_control_state:
                    Some(LongRunningCommandControlState::Agent {
                        should_hide_responses,
                        ..
                    }),
                ..
            }) => {
                *should_hide_responses = !*should_hide_responses;
                true
            }
            _ => false,
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum UpdateInteractionModeError {
    #[error("Attempted to update interaction mode from agent with requested command to agent-monitored for mismatched conversation IDs.")]
    UnexpectedConversationId,
    #[error("Attempted to take over control for user when block was not already agent controlled")]
    InvalidTakeOver,
    #[error("Attempted to handoff control to agent when block was not already user controlled")]
    InvalidHandOff,
}

#[derive(Debug, Clone)]
pub struct UserMode {
    // `true` if the user executed the command themself and the agent mode input should be shown.
    //
    // This does _not_ mean an agent is in control of the command. This merely means the user has
    // opted to show the agent input, indicated intent to send a query to give control.
    //
    // If the user executes a command, shows the input, then hides the input, this reverts to
    // `false`.
    did_user_tag_in_agent: bool,
}

/// Represents the 'interaction mode' for a command block with respect to the agent.
///
/// There are 4 user-perceived states:
///
/// 1) The command was executed by the user; if long-running, they are in control and the input is hidden.
/// 2) The command was executed by the user, but is long-running and the user has toggled on the
///    agent input and may send a query to trigger the Agent (the CLI subagent). We refer to this
///    state, where the user executed the command and deliberately opened the agent input, as the
///    agent being 'tagged in'. Note that this is distinct from the agent actually having control.
///    "Tagged in" merely means the command is running and the agent mode input is visible
/// 3) The command was executed by the agent (is a requested command) and is not long running. No CLI subagent was triggered.
/// 4) The command was executed by the agent and is long running, and thus the CLI subagent was triggered.
///   a) The agent is in control of the command (actively reading the command's output or writing input to the command)
///   b) The agent was in the control of the command, but the user took over.
///
/// The `User` variant represents modes where the user executed the original command and the agent has yet to take control.
/// The `Agent` variant represents modes where the agent either ran the command itself, or the user tagged in the agent and
/// passed control to the CLI subagent by sending a query during its execution
#[derive(Debug, Clone)]
pub enum InteractionMode {
    User(UserMode),
    Agent(AgentInteractionMetadata),
}

impl InteractionMode {
    fn to_agent_monitored(
        &self,
        task_id: &TaskId,
        conversation_id: AIConversationId,
    ) -> Result<Self, UpdateInteractionModeError> {
        let (requested_command_action_id, existing_control_state) = match self {
            InteractionMode::User(_) => (None, None),
            InteractionMode::Agent(metadata) => {
                if metadata.conversation_id != conversation_id {
                    return Err(UpdateInteractionModeError::UnexpectedConversationId);
                }
                (
                    metadata.requested_command_action_id.clone(),
                    metadata.long_running_control_state.clone(),
                )
            }
        };

        // Spawning the subagent does not imply seizing control, so a hand-over the user
        // already holds survives it. `CreatedSubtask` is asynchronous and can land *after*
        // the PTY was handed over: `BlockedOnInput` fires about a second into the block,
        // which is exactly the window before any control state exists -- that is why
        // `take_over_for_user` admits it with `long_running_control_state: None`. Stamping
        // `Agent` over it here erased the hand-over silently, and since `should_auto_resume()`
        // is `false` for `Agent`, `on_user_block_completed` then never resumed the
        // conversation: the user answered the password prompt, the command succeeded, and the
        // agent sat waiting on a result that never arrived until the tab was killed. Because
        // the take-over is only legal in the pre-`CreatedSubtask` window, that clobber was the
        // expected continuation of the path rather than a rare interleaving.
        let long_running_control_state = match existing_control_state {
            Some(state @ LongRunningCommandControlState::User { .. }) => state,
            _ => LongRunningCommandControlState::Agent {
                is_blocked: false,
                should_hide_responses: false,
            },
        };

        Ok(Self::Agent(AgentInteractionMetadata {
            requested_command_action_id,
            conversation_id,
            subagent_task_id: Some(task_id.clone()),
            long_running_control_state: Some(long_running_control_state),
            has_agent_written_to_block: false,
            should_hide_block: false,
        }))
    }

    fn new_agent(metadata: AgentInteractionMetadata) -> Self {
        Self::Agent(metadata)
    }

    fn from_serialized_ai_metadata(serialized_metadata: SerializedAIMetadata) -> Self {
        Self::Agent(serialized_metadata.into())
    }

    fn agent_interaction_metadata(&self) -> Option<&AgentInteractionMetadata> {
        match self {
            Self::Agent(agent_interaction_metadata) => Some(agent_interaction_metadata),
            Self::User(_) => None,
        }
    }

    pub fn should_hide_block(&self) -> bool {
        match self {
            Self::Agent(metadata) => metadata.should_hide_block,
            _ => false,
        }
    }

    pub fn long_running_control_state(&self) -> Option<&LongRunningCommandControlState> {
        match self {
            Self::Agent(metadata) => metadata.long_running_control_state.as_ref(),
            _ => None,
        }
    }

    pub fn is_agent_tagged_in(&self) -> bool {
        matches!(
            self,
            Self::User(UserMode {
                did_user_tag_in_agent: true
            })
        )
    }

    fn set_should_hide_block(&mut self, value: bool) {
        if let Self::Agent(metadata) = self {
            metadata.should_hide_block = value;
        }
    }

    fn take_over_for_user(
        &mut self,
        reason: UserTakeOverReason,
    ) -> Result<(), UpdateInteractionModeError> {
        let Self::Agent(AgentInteractionMetadata {
            requested_command_action_id,
            long_running_control_state,
            ..
        }) = self
        else {
            return Err(UpdateInteractionModeError::InvalidTakeOver);
        };

        // A take-over normally requires the CLI subagent to already hold control, which keeps
        // stray take-overs from seizing a command the agent never had. The password-prompt
        // hand-over is the one exception: termios is first sampled a second after the block
        // starts, so a hand-over can be attempted before a control state is installed, and
        // requiring one would reject exactly what `BlockedOnInput` exists for.
        //
        // Widening this to *any* reason was tried and reverted. It was meant as the escape hatch
        // for a warpified agent command that never receives a control state at all -- and
        // `AgentInteractionMetadata::new_hidden` still leaves that state `None` (installing one
        // there was also tried and reverted; see its doc comment) -- but the hatch gave up a
        // property `blocked_on_input_takes_over_before_the_subagent_has_control` in
        // `terminal_use_tests.rs` correctly guards: a manual take-over should still require
        // the agent to actually hold control.
        let is_valid_take_over = match long_running_control_state.as_ref() {
            Some(state) => state.is_agent_in_control(),
            None => reason.is_blocked_on_input() && requested_command_action_id.is_some(),
        };
        if !is_valid_take_over {
            return Err(UpdateInteractionModeError::InvalidTakeOver);
        }

        *long_running_control_state = Some(LongRunningCommandControlState::User { reason });
        Ok(())
    }

    fn handoff_to_agent(&mut self) -> Result<(), UpdateInteractionModeError> {
        let Self::Agent(AgentInteractionMetadata {
            long_running_control_state,
            ..
        }) = self
        else {
            return Err(UpdateInteractionModeError::InvalidHandOff);
        };

        if !long_running_control_state
            .as_ref()
            .is_some_and(|state| state.is_user_in_control())
        {
            return Err(UpdateInteractionModeError::InvalidHandOff);
        }

        *long_running_control_state = Some(LongRunningCommandControlState::Agent {
            is_blocked: false,
            should_hide_responses: false,
        });
        Ok(())
    }
}

impl Default for InteractionMode {
    fn default() -> Self {
        Self::User(UserMode {
            did_user_tag_in_agent: false,
        })
    }
}

/// Blocklist AI metadata associated with this block.
#[derive(Debug, Clone)]
pub struct AgentInteractionMetadata {
    /// The ID of the `AIAgentAction` associated with this block's requested command execution.
    /// This is optional because not all AI-related blocks are associated with a requested command.
    requested_command_action_id: Option<AIAgentActionId>,

    /// The ID of the conversation to which this action belongs.
    conversation_id: AIConversationId,

    /// The task ID for the CLI subagent interaction with this block if any.
    subagent_task_id: Option<TaskId>,

    /// State governing user/agent interaction with the command in this block.
    long_running_control_state: Option<LongRunningCommandControlState>,

    /// `true` if the agent has previously written to this block.
    has_agent_written_to_block: bool,

    /// `true` if this block should be hidden from the user (as is the case with AI-requested
    /// commands, for example).
    should_hide_block: bool,
}

impl AgentInteractionMetadata {
    /// Creates a new metadata instance with fully specified fields.
    pub fn new(
        requested_command_action_id: Option<AIAgentActionId>,
        conversation_id: AIConversationId,
        subagent_task_id: Option<TaskId>,
        long_running_control_state: Option<LongRunningCommandControlState>,
        has_agent_written_to_block: bool,
        should_hide_block: bool,
    ) -> Self {
        AgentInteractionMetadata {
            requested_command_action_id,
            conversation_id,
            subagent_task_id,
            long_running_control_state,
            has_agent_written_to_block,
            should_hide_block,
        }
    }

    /// Convenience constructor for the common "hidden by default" case used for requested commands.
    ///
    /// `long_running_control_state` deliberately stays `None` until the BYOP LRC monitor
    /// fallback upgrades it. Installing `Agent { .. }` here was tried and reverted: two widely
    /// consulted predicates are defined in terms of its *absence* --
    /// `is_agent_driving_command`'s fallback arm requires `long_running_control_state().is_none()`,
    /// and `is_agent_monitoring()` is `is_active_and_long_running() && state.is_some()` -- so
    /// filling it in collapsed the first to `is_agent_in_control()` (false for the block's first
    /// 50ms, which is exactly when the password-prompt poller is armed) and made the
    /// `PendingLrcAutoQueue` origin unproducible. It disabled both fixes it was meant to
    /// complete, and flipped a dozen unrelated call sites at the 50ms boundary besides.
    ///
    /// Giving a warpified agent command a control state is still the right end state; it needs
    /// those predicates redefined first, which is a design change rather than a constructor
    /// tweak. See TODO.md.
    pub fn new_hidden(
        requested_command_action_id: AIAgentActionId,
        conversation_id: AIConversationId,
    ) -> Self {
        Self::new(
            Some(requested_command_action_id),
            conversation_id,
            None,
            None,
            false,
            true,
        )
    }

    pub fn requested_command_action_id(&self) -> Option<&AIAgentActionId> {
        self.requested_command_action_id.as_ref()
    }

    pub fn conversation_id(&self) -> &AIConversationId {
        &self.conversation_id
    }

    pub fn subagent_task_id(&self) -> Option<&TaskId> {
        self.subagent_task_id.as_ref()
    }

    pub fn is_agent_in_control(&self) -> bool {
        self.long_running_control_state
            .as_ref()
            .is_some_and(|state| state.is_agent_in_control())
    }

    pub fn long_running_control_state(&self) -> Option<&LongRunningCommandControlState> {
        self.long_running_control_state.as_ref()
    }

    pub fn has_agent_written_to_block(&self) -> bool {
        self.has_agent_written_to_block
    }

    pub fn should_hide_block(&self) -> bool {
        self.should_hide_block
    }
}

/// String representation of the cursor to interpolate in the terminal contents string.
pub const CURSOR_MARKER: &str = "<|cursor|>";

/// Returns a string representation of the terminal contents (represented by the `grid_handler`),
/// limited to `max_row_count` rows in the grid.
///
/// This function returns a string representation of the terminal contents, with a cursor "marker" substring
/// interpolated at the same position in the string as it appears in the grid.
pub fn formatted_terminal_contents_for_input(
    grid_handler: &GridHandler,
    max_row_count: Option<usize>,
    cursor_pattern: &'static str,
) -> String {
    let cursor_point = grid_handler.cursor_point();

    let max_column_index = grid_handler.columns().saturating_sub(1);
    let (context_start_point, context_end_point) = match max_row_count {
        Some(max_count) => {
            // Return start and end points such that the range is of size max_count, bounded to the
            // max row value of the grid.
            let end_point = Point::new(grid_handler.max_content_row(), max_column_index).min(
                Point::new(cursor_point.row + max_count / 2, max_column_index),
            );
            let start_point = Point::new(end_point.row.saturating_sub(max_count), 0);
            (start_point, end_point)
        }
        None => (
            Point::new(0, 0),
            Point::new(
                grid_handler.total_rows().saturating_sub(1),
                grid_handler.columns().saturating_sub(1),
            ),
        ),
    };

    format!(
        "{}{}{cursor_pattern}{}",
        grid_handler.bounds_to_string(
            context_start_point,
            if cursor_point.col == 0 {
                Point::new(
                    cursor_point.row.saturating_sub(1),
                    grid_handler.columns().saturating_sub(1),
                )
            } else {
                Point::new(cursor_point.row, cursor_point.col.saturating_sub(1))
            },
            false,
            RespectObfuscatedSecrets::Yes,
            true,
            RespectDisplayedOutput::No,
        ),
        if cursor_point.col == 0 { "\n" } else { "" },
        grid_handler.bounds_to_string(
            cursor_point,
            context_end_point,
            false,
            RespectObfuscatedSecrets::Yes,
            true,
            RespectDisplayedOutput::No,
        )
    )
}

/// The text on the cursor's logical line (i.e. following soft wraps backward) up to (not
/// including) the cursor, or `None` when the cursor sits at column 0 -- i.e. the last thing the
/// command printed ended in a newline, which is not the shape of a prompt waiting for an answer
/// on the same line.
///
/// Used to spot a command stalled on an interactive prompt (`[y/N]`, `read -p`, "Press any
/// key"). Those prompts leave termios in cooked mode with echo on, so unlike a password prompt
/// the termios poller cannot see them; the only signal is what is printed before the cursor. A
/// narrow terminal wraps even a short prompt across several grid rows, so the start of the range
/// has to follow `WRAPLINE` back to the start of the logical line rather than assuming the
/// cursor's grid row holds the whole thing.
pub fn text_before_cursor_on_cursor_line(grid_handler: &GridHandler) -> Option<String> {
    let cursor_point = grid_handler.cursor_point();
    if cursor_point.col == 0 {
        return None;
    }
    Some(grid_handler.bounds_to_string(
        grid_handler.line_search_left(cursor_point),
        Point::new(cursor_point.row, cursor_point.col.saturating_sub(1)),
        false,
        RespectObfuscatedSecrets::Yes,
        true,
        RespectDisplayedOutput::No,
    ))
}

/// How strongly a line reads as a prompt waiting for an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InteractivePromptShape {
    /// An explicit confirmation marker: `[y/N]`, `(yes/no)`, "Press any key", ...
    Explicit,
    /// Only a trailing `?` or `:` -- what `read -p "Continue? "` and `read -p "Name: "` leave
    /// the cursor after, but also what `printf "Waiting for DB:"; sleep 30` leaves.
    TrailingPunctuation,
}

impl InteractivePromptShape {
    /// Consecutive once-a-second polls the prompt must sit unchanged before it counts as
    /// stalled. A bare `?` / `:` is far weaker evidence, so it waits long enough to outlast
    /// a progress label and a short `read -t` timeout; a real `read -p` then waits ~15s for
    /// the hand-over instead of the 30-minute backstop.
    pub fn required_consecutive_polls(self) -> u8 {
        match self {
            Self::Explicit => 3,
            Self::TrailingPunctuation => 15,
        }
    }
}

/// The shape of an interactive prompt in `line_before_cursor` -- the output on the cursor's
/// row, up to the cursor -- or `None` if it doesn't read as one.
///
/// Deliberately loose: it is only consulted for an agent command whose tool call is blocked
/// waiting for completion (so the agent could not answer anyway), and only after the same
/// line, at the same cursor position, has been seen across several termios polls. A false
/// positive hands the PTY to the user while the command keeps running; a false negative is
/// the 30-minute hang.
pub fn interactive_prompt_shape(line_before_cursor: &str) -> Option<InteractivePromptShape> {
    /// Prompts are one short line. Anything longer is output that happens to lack a newline.
    const MAX_PROMPT_CHARS: usize = 200;
    const MARKERS: &[&str] = &[
        "[y/n]",
        "(y/n)",
        "[yes/no]",
        "(yes/no",
        "[y/n/",
        "y/n?",
        "press any key",
        "press enter",
        "press return",
        "hit enter",
        "hit any key",
    ];

    let line = line_before_cursor.trim();
    if line.is_empty() || line.chars().count() > MAX_PROMPT_CHARS {
        return None;
    }
    let lower = line.to_lowercase();
    if MARKERS.iter().any(|marker| lower.contains(marker)) {
        Some(InteractivePromptShape::Explicit)
    } else if line.ends_with('?') || line.ends_with(':') {
        Some(InteractivePromptShape::TrailingPunctuation)
    } else {
        None
    }
}

/// One poll's view of a possible interactive prompt: the text before the cursor, where the
/// cursor is, and how prompt-like the text is. Two polls only count as "the same prompt" if
/// all of it matches, so any further output -- which moves the cursor or changes the line --
/// restarts the count.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InteractivePromptCandidate {
    pub text: String,
    pub cursor: Point,
    pub shape: InteractivePromptShape,
}

/// The prompt-shaped, newline-less line the command has left the cursor on in
/// `grid_handler`, if any.
pub fn interactive_prompt_candidate(
    grid_handler: &GridHandler,
) -> Option<InteractivePromptCandidate> {
    let text = text_before_cursor_on_cursor_line(grid_handler)?;
    let shape = interactive_prompt_shape(&text)?;
    Some(InteractivePromptCandidate {
        text,
        cursor: grid_handler.cursor_point(),
        shape,
    })
}

/// What a termios poll detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptDetection {
    /// Echo off with canonical input on: the password-prompt shape.
    Password,
    /// A prompt-shaped line that has sat unchanged for its shape's debounce.
    Interactive,
}

/// Turns the stream of termios polls for one block into prompt detections.
///
/// Each kind fires once per *episode* and re-arms when the episode ends, rather than once
/// per block: a password prompt re-arms once termios leaves the password shape, and an
/// interactive prompt re-arms once no candidate is seen (which includes whenever the user,
/// not the agent, holds the command -- the surface reports no candidate then). That is what
/// lets a second prompt in the same command be detected after the user hands control back,
/// e.g. an agent-run `ssh` stopping first on the host-key `(yes/no/[fingerprint])?` and then
/// on the password prompt.
#[derive(Debug, Default)]
pub struct PromptDetector {
    password_latched: bool,
    last_candidate: Option<InteractivePromptCandidate>,
    consecutive_polls: u8,
    interactive_latched: bool,
}

impl PromptDetector {
    /// Forgets everything observed so far. Called when a new block starts.
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    /// Feeds one poll. `interactive_candidate` is only called when termios is cooked with
    /// echo on, the one shape an interactive prompt can have; raw-mode programs (editors,
    /// REPLs, a logged-in ssh session) are never probed.
    pub fn observe(
        &mut self,
        is_echo_on: bool,
        is_canonical: bool,
        interactive_candidate: impl FnOnce() -> Option<InteractivePromptCandidate>,
    ) -> Option<PromptDetection> {
        if !is_echo_on && is_canonical {
            self.clear_interactive();
            if self.password_latched {
                return None;
            }
            self.password_latched = true;
            return Some(PromptDetection::Password);
        }
        self.password_latched = false;

        let candidate = if is_echo_on && is_canonical {
            interactive_candidate()
        } else {
            None
        };
        let Some(candidate) = candidate else {
            self.clear_interactive();
            return None;
        };
        if self.last_candidate.as_ref() == Some(&candidate) {
            self.consecutive_polls = self.consecutive_polls.saturating_add(1);
        } else {
            self.consecutive_polls = 1;
            self.interactive_latched = false;
            self.last_candidate = Some(candidate);
        }
        let required = self.last_candidate.as_ref().map_or(u8::MAX, |candidate| {
            candidate.shape.required_consecutive_polls()
        });
        if self.interactive_latched || self.consecutive_polls < required {
            return None;
        }
        self.interactive_latched = true;
        Some(PromptDetection::Interactive)
    }

    fn clear_interactive(&mut self) {
        self.last_candidate = None;
        self.consecutive_polls = 0;
        self.interactive_latched = false;
    }
}
