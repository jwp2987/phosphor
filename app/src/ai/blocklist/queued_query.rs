use std::collections::HashMap;

use chrono::{NaiveDateTime, Utc};
use uuid::Uuid;
use warp_core::execution_mode::AppExecutionMode;
use warpui::{AppContext, Entity, EntityId, ModelContext, SingletonEntity};

use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::{BlocklistAIHistoryEvent, BlocklistAIHistoryModel, PendingAttachment};
use crate::features::FeatureFlag;
use crate::global_resource_handles::GlobalResourceHandlesProvider;
use crate::persistence::ModelEvent;
use crate::persistence::model::QueuedPromptRow;
use crate::settings::{
    AISettings, AISettingsChangedEvent, LongRunningCommandSubmissionMode, PromptSubmissionMode,
};
use crate::terminal::general_settings::GeneralSettings;
use crate::terminal::model::block::Block;

/// A globally unique identifier for a single queued prompt row.
/// Used by the queue panel to address rows across reorder, edit, and delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct QueuedQueryId(Uuid);

impl QueuedQueryId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// The id as stored in the `queued_prompts` table.
    fn to_storage_string(self) -> String {
        self.0.to_string()
    }

    /// Parses a stored id. A malformed one is replaced rather than dropping the row: the id
    /// only addresses the row in the panel, so nothing depends on it surviving a restart.
    fn from_storage_string(raw: &str) -> Self {
        Uuid::try_parse(raw)
            .map(Self)
            .unwrap_or_else(|_| Self::new())
    }
}

/// Where a queued prompt came from.
/// The origin is informational for telemetry; FIFO ordering and firing semantics are uniform.
///
/// Zap note: upstream Warp also has an `InitialCloudMode` origin for a permanently-locked
/// row that holds the initial Cloud Mode prompt until hand-off. Cloud Mode is a Warp cloud
/// feature that Zap does not have, so that origin (and its dedicated `remove_*` path) is
/// dropped here. The locked origins are `PendingLrcAutoQueue` and (jwp2987/phosphor#690
/// follow-up) `PendingApprovalFollowUp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuedQueryOrigin {
    /// Filed via the `/queue <prompt>` slash command.
    QueueSlashCommand,
    /// Filed via the auto-queue toggle in the warping indicator.
    AutoQueueToggle,
    /// Filed because auto-queue was in effect during an agent-requested long-running command.
    LrcAutoQueue,
    /// Filed while an agent-requested run_shell_command action's snapshot has not yet fired.
    /// Locked for manual push and auto-fire until the snapshot fires.
    PendingLrcAutoQueue,
    /// Filed because the conversation was `Blocked` on a pending action confirmation when the
    /// user submitted a follow-up. Locked for manual push and auto-fire until the blocked
    /// action resolves (approved and finished, rejected, or the conversation is cancelled --
    /// see [`QueuedQueryModel::unlock_pending_approval_rows`]).
    PendingApprovalFollowUp,
    /// A former `PendingApprovalFollowUp` row whose blocked action has resolved. Ordinary
    /// unlocked queued-prompt semantics from here.
    ApprovalFollowUp,
    /// Filed as the follow-up prompt of a `/compact-and <prompt>` slash command, waiting for
    /// the summarize to finish.
    CompactAndSlashCommand,
    /// Filed as the follow-up prompt of a `/fork-and-compact <prompt>` slash command on the
    /// forked conversation, waiting for the fork's summarize to finish.
    ForkAndCompactSlashCommand,
}

impl QueuedQueryOrigin {
    /// The variant name stored in `queued_prompts.origin`. Diagnostic only: restored rows do
    /// not keep their origin (see [`QueuedQuery::from_persisted_row`]).
    fn storage_name(self) -> &'static str {
        match self {
            Self::QueueSlashCommand => "QueueSlashCommand",
            Self::AutoQueueToggle => "AutoQueueToggle",
            Self::LrcAutoQueue => "LrcAutoQueue",
            Self::PendingLrcAutoQueue => "PendingLrcAutoQueue",
            Self::PendingApprovalFollowUp => "PendingApprovalFollowUp",
            Self::ApprovalFollowUp => "ApprovalFollowUp",
            Self::CompactAndSlashCommand => "CompactAndSlashCommand",
            Self::ForkAndCompactSlashCommand => "ForkAndCompactSlashCommand",
        }
    }
}

/// Whether a queued row is an agent prompt or a shell command. Attachments live inside the
/// `Prompt` variant so a `Command` structurally cannot carry any.
#[derive(Debug, Clone)]
enum QueuedQueryKind {
    /// An agent prompt, with any image/file attachments captured from the input when it was
    /// queued. The attachments fire with the prompt and are dropped when the row is removed.
    Prompt { attachments: Vec<PendingAttachment> },
    /// A shell command run in the terminal (or via the shared session for cloud panes).
    Command,
}

/// A single queued row: an agent prompt or a shell command.
#[derive(Debug, Clone)]
pub struct QueuedQuery {
    id: QueuedQueryId,
    text: String,
    origin: QueuedQueryOrigin,
    kind: QueuedQueryKind,
    created_at: NaiveDateTime,
    /// True for a row restored from the previous session (#1027). A suspended row is never
    /// auto-fired: the user must see it first, and sends, edits or removes it themselves.
    suspended: bool,
}

impl QueuedQuery {
    pub fn new(text: String, origin: QueuedQueryOrigin) -> Self {
        Self::new_with_attachments(text, origin, Vec::new())
    }

    pub fn new_with_attachments(
        text: String,
        origin: QueuedQueryOrigin,
        attachments: Vec<PendingAttachment>,
    ) -> Self {
        Self {
            id: QueuedQueryId::new(),
            text,
            origin,
            kind: QueuedQueryKind::Prompt { attachments },
            created_at: Utc::now().naive_utc(),
            suspended: false,
        }
    }

    /// Builds a queued shell command. Commands never carry attachments.
    pub fn new_command(text: String, origin: QueuedQueryOrigin) -> Self {
        Self {
            id: QueuedQueryId::new(),
            text,
            origin,
            kind: QueuedQueryKind::Command,
            created_at: Utc::now().naive_utc(),
            suspended: false,
        }
    }

    /// True when this row was restored from the previous session and has not been acted on
    /// yet; such a row never auto-fires (#1027).
    pub fn is_suspended(&self) -> bool {
        self.suspended
    }

    /// The row as stored in `queued_prompts`. Attachments (images/files) are not stored; see
    /// [`QueuedQueryModel::persist_queue`].
    fn to_persisted_row(
        &self,
        conversation_id: AIConversationId,
        position: usize,
    ) -> QueuedPromptRow {
        QueuedPromptRow {
            id: self.id.to_storage_string(),
            conversation_id: conversation_id.to_string(),
            position: i32::try_from(position).unwrap_or(i32::MAX),
            kind: if self.is_command() {
                "command"
            } else {
                "prompt"
            }
            .to_owned(),
            text: self.text.clone(),
            origin: self.origin.storage_name().to_owned(),
            created_at: self.created_at,
        }
    }

    /// Rebuilds a row stored by a previous session, suspended. An unrecognised `kind` is read
    /// as a prompt rather than dropping the user's text. The origin is reset to
    /// [`QueuedQueryOrigin::QueueSlashCommand`]: the locked origins describe an action
    /// snapshot that no longer exists, and the origin-specific send paths (LRC delivery,
    /// approval follow-ups) must not pick up a row that was restored.
    fn from_persisted_row(row: &QueuedPromptRow) -> Self {
        let kind = if row.kind == "command" {
            QueuedQueryKind::Command
        } else {
            QueuedQueryKind::Prompt {
                attachments: Vec::new(),
            }
        };
        Self {
            id: QueuedQueryId::from_storage_string(&row.id),
            text: row.text.clone(),
            origin: QueuedQueryOrigin::QueueSlashCommand,
            kind,
            created_at: row.created_at,
            suspended: true,
        }
    }

    pub fn id(&self) -> QueuedQueryId {
        self.id
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn origin(&self) -> QueuedQueryOrigin {
        self.origin
    }

    /// Returns true if this row is a shell command rather than an agent prompt.
    pub fn is_command(&self) -> bool {
        matches!(self.kind, QueuedQueryKind::Command)
    }

    pub fn attachments(&self) -> &[PendingAttachment] {
        match &self.kind {
            QueuedQueryKind::Prompt { attachments } => attachments,
            QueuedQueryKind::Command => &[],
        }
    }

    /// Returns true if this row is locked from *submission*: it cannot be reordered, pushed
    /// manually, popped, or auto-fired by the drain, because doing so during a pending action
    /// snapshot is the CliAgentUserQuery/LRC race this origin exists to prevent.
    ///
    /// It can still be **deleted and edited**. Neither submits anything, so neither can race,
    /// and gating them meant a user who queued a prompt during a long-running command could
    /// not remove it for as long as that command ran -- with both handlers returning silently,
    /// so the row simply ignored every click. The panel already took this view: it disables
    /// only the Send Now button for these rows (`queued_prompts_panel.rs:400`).
    ///
    /// `PendingLrcAutoQueue` rows are locked only until the action snapshot fires;
    /// `PendingApprovalFollowUp` rows only until the blocked action resolves.
    pub fn is_locked(&self) -> bool {
        matches!(
            self.origin,
            QueuedQueryOrigin::PendingLrcAutoQueue | QueuedQueryOrigin::PendingApprovalFollowUp
        )
    }
}

/// What the auto-fire drain should do with the head row. Produced by
/// [`QueuedQueryModel::peek_autofire`] *without* removing the row; the caller removes it via
/// [`QueuedQueryModel::remove_fired_row`] once the prompt has been dispatched or restored.
#[derive(Debug)]
pub enum AutofireAction {
    /// Submit this prompt as a normal queued user query. The row stays in the queue so the send
    /// path can read its attachments by `query_id`; the caller removes it afterward.
    Submit {
        query_id: QueuedQueryId,
        text: String,
    },
    /// The head row was in edit mode. The caller restores `text` (the row's last committed text)
    /// and `attachments` to the input box, then removes the row. `is_command` distinguishes a
    /// shell command (no attachments; restored in shell mode) from an agent prompt, so the
    /// restored row keeps its kind instead of being re-submitted as the wrong type.
    PopFromEditMode {
        query_id: QueuedQueryId,
        text: String,
        attachments: Vec<PendingAttachment>,
        is_command: bool,
    },
    /// Execute this row as a shell command (its kind is `Command`). The caller runs the command,
    /// removes the row, and waits for the command to finish before draining the next row.
    ExecuteCommand {
        query_id: QueuedQueryId,
        command: String,
    },
}

/// Per-conversation queue / edit / toggle state.
/// Lives inside [`QueuedQueryModel::queues`]; a missing key means empty queue, no edit in
/// progress, and no explicit auto-queue override (so the cached default from
/// [`AISettings::default_prompt_submission_mode`] is used).
#[derive(Default)]
struct ConversationQueueState {
    queue: Vec<QueuedQuery>,
    editing: Option<QueuedQueryId>,
    /// Explicit per-conversation override. `None` defers to the model's cached
    /// `default_mode`; `Some` means the user has toggled this conversation
    /// at least once.
    queue_next_prompt_override: Option<bool>,
    /// True while a drained shell command from this queue is running. Set when the command is
    /// dispatched and cleared when it finishes; keeps the queue accepting new rows while the
    /// agent is idle and gates the next drain until the command completes.
    command_in_flight: bool,
    /// Manual queue toggle made during an agent-requested long-running command. Cleared when
    /// the command ends; never touches `queue_next_prompt_override`.
    queue_next_lrc_prompt_override: Option<bool>,
}

/// App-wide singleton owning the queued prompts and auto-queue toggle for every conversation,
/// indexed by [`AIConversationId`]. Queues outlive the agent-view session that originated them;
/// cleanup is driven by [`BlocklistAIHistoryModel`] lifecycle events that this model subscribes
/// to in [`QueuedQueryModel::new`].
pub struct QueuedQueryModel {
    queues: HashMap<AIConversationId, ConversationQueueState>,
    /// Cached value of the `AISettings::default_prompt_submission_mode` setting,
    /// refreshed by an `AISettingsChangedEvent::DefaultPromptSubmissionMode`
    /// subscription. Used as the fallback when a conversation has no explicit
    /// per-conversation override. Caching keeps the warping-indicator render
    /// path doing only a hashmap lookup plus a comparison.
    default_mode: PromptSubmissionMode,
    /// How many rows [`Self::restore_persisted`] put back into queues, not yet announced.
    restored_suspended: usize,
    /// Restored rows whose conversation no longer exists. A terminal view claims them with
    /// [`Self::take_restored_session`] and puts their text in its input.
    restored_orphans: Vec<QueuedQuery>,
    /// Stored conversation ids behind `restored_orphans`; their rows are deleted from the
    /// database once a view has claimed the text, never before.
    orphan_conversation_ids: Vec<String>,
}

/// What [`QueuedQueryModel::take_restored_session`] hands the first terminal view after a
/// restart (#1027).
#[derive(Debug, Default)]
pub struct RestoredQueueSession {
    /// Number of rows put back into conversation queues, suspended.
    pub suspended_count: usize,
    /// Rows whose conversation is gone; the caller must put their text in an input.
    pub orphans: Vec<QueuedQuery>,
}

/// Events emitted by [`QueuedQueryModel`]. Every variant carries the `conversation_id` it applies
/// to so subscribers can filter to the conversation they care about.
#[derive(Debug, Clone)]
pub enum QueuedQueryEvent {
    Appended {
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
    },
    /// Emitted when locked rows are transitioned to their unlocked counterpart:
    /// `PendingLrcAutoQueue` -> `LrcAutoQueue` after the action snapshot fires, or
    /// `PendingApprovalFollowUp` -> `ApprovalFollowUp` after a blocked action resolves
    /// (jwp2987/phosphor#690 follow-up).
    RowUnlocked {
        conversation_id: AIConversationId,
    },
    Removed {
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
    },
    Reordered {
        conversation_id: AIConversationId,
    },
    EditEntered {
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
    },
    EditCommitted {
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
    },
    EditCancelled {
        conversation_id: AIConversationId,
        #[allow(dead_code)]
        query_id: QueuedQueryId,
    },
    Cleared {
        conversation_id: AIConversationId,
    },
    /// A conversation was removed, deleted, or cleared while it still had queued rows (#1026).
    /// Emitted just before [`Self::Cleared`]; carries the rows so the owning terminal view can
    /// put their text back in its input instead of silently losing the user's prompts.
    /// `terminal_view_id` is the view the removal came from, when the history event names one.
    DroppedUnsent {
        conversation_id: AIConversationId,
        terminal_view_id: Option<EntityId>,
        rows: Vec<QueuedQuery>,
    },
    QueueNextPromptToggled {
        conversation_id: AIConversationId,
    },
    /// The `AISettings::default_prompt_submission_mode` setting changed, so the
    /// effective value of `is_queue_next_prompt_enabled` may have changed for
    /// every conversation without an explicit override. Subscribers that
    /// display the toggle state should re-render.
    DefaultModeChanged,
}

impl Entity for QueuedQueryModel {
    type Event = QueuedQueryEvent;
}

impl SingletonEntity for QueuedQueryModel {}

impl QueuedQueryModel {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Drop queue/toggle state for any conversation that is removed, deleted, or cleared
        // from its owning terminal view. Agent-view exit is intentionally NOT subscribed to:
        // conversations (cloud agents in particular) outlive their visible session.
        let history_handle = BlocklistAIHistoryModel::handle(ctx);
        ctx.subscribe_to_model(&history_handle, |this, event, ctx| {
            this.handle_history_event(event, ctx);
        });

        // Cache the default submission mode and refresh whenever the AI setting
        // changes. The render path consults the cache instead of dereferencing
        // the setting on every call. The LRC submission-mode setting is read by
        // callers directly, but its changes also re-emit `DefaultModeChanged` so
        // the chip and ghost text re-render with the new effective state.
        let default_mode = AISettings::as_ref(ctx).default_prompt_submission_mode;
        let ai_settings_handle = AISettings::handle(ctx);
        ctx.subscribe_to_model(&ai_settings_handle, |this, event, ctx| match event {
            AISettingsChangedEvent::PromptSubmissionMode { .. } => {
                this.default_mode = AISettings::as_ref(ctx).default_prompt_submission_mode;
                ctx.emit(QueuedQueryEvent::DefaultModeChanged);
            }
            AISettingsChangedEvent::LongRunningCommandSubmissionMode { .. } => {
                ctx.emit(QueuedQueryEvent::DefaultModeChanged);
            }
            _ => {}
        });

        Self {
            queues: HashMap::new(),
            default_mode,
            restored_suspended: 0,
            restored_orphans: Vec::new(),
            orphan_conversation_ids: Vec::new(),
        }
    }

    /// Writes `conversation_id`'s whole queue to the `queued_prompts` table through the SQLite
    /// writer thread, so a quit or crash does not lose it (#1027). An empty queue deletes the
    /// stored rows. Called after every change to a queue.
    ///
    /// Follows the conversation writes' channel discipline (#1000): hand the event over from
    /// this thread with `try_send` so successive writes reach the writer in order, and fall
    /// back to a blocking send off the UI thread only when the channel is full. Attachments
    /// (images and files) are not stored; only the text survives a restart.
    ///
    /// Skipped, like conversation persistence, when the user turned conversation saving off
    /// or the app cannot save sessions (tests, CLI).
    fn persist_queue(&self, conversation_id: AIConversationId, ctx: &mut ModelContext<Self>) {
        let rows: Vec<QueuedPromptRow> = self
            .queue(conversation_id)
            .iter()
            .enumerate()
            .map(|(position, row)| row.to_persisted_row(conversation_id, position))
            .collect();
        Self::send_queue_rows(conversation_id.to_string(), rows, ctx);
    }

    fn send_queue_rows(
        stored_conversation_id: String,
        rows: Vec<QueuedPromptRow>,
        ctx: &mut ModelContext<Self>,
    ) {
        if !ctx.has_singleton_model::<GeneralSettings>()
            || !*GeneralSettings::as_ref(ctx).persist_conversations
            || !ctx.has_singleton_model::<AppExecutionMode>()
            || !AppExecutionMode::as_ref(ctx).can_save_session()
            || !ctx.has_singleton_model::<GlobalResourceHandlesProvider>()
        {
            return;
        }
        let Some(sender) = GlobalResourceHandlesProvider::as_ref(ctx)
            .get()
            .model_event_sender
            .clone()
        else {
            return;
        };
        let event = ModelEvent::ReplaceQueuedPrompts {
            conversation_id: stored_conversation_id,
            rows,
        };
        match sender.try_send(event) {
            Ok(()) => {}
            Err(std::sync::mpsc::TrySendError::Full(event)) => {
                log::warn!("SQLite writer channel is full; sending the queue write async");
                ctx.spawn(
                    async move {
                        if let Err(e) = sender.send(event) {
                            log::warn!("Failed to send queued prompts to the sqlite writer: {e:?}");
                        }
                    },
                    |_, _, _| {},
                );
            }
            Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::warn!("Failed to send queued prompts to the sqlite writer: disconnected");
            }
        }
    }

    /// Puts the queues saved by the previous session back (#1027), once, at startup.
    ///
    /// Rows whose conversation still exists return to its queue **suspended**: they never
    /// auto-fire, and sit in the panel until the user sends, edits or removes them. Rows whose
    /// conversation is gone cannot be shown anywhere, so they are held as orphans for the
    /// first terminal view to put in its input ([`Self::take_restored_session`]). Nothing
    /// is sent, and nothing is deleted from the database here: stored rows stay until a later
    /// change to the same queue rewrites them, or a view claims an orphan.
    pub fn restore_persisted(&mut self, rows: Vec<QueuedPromptRow>, ctx: &mut ModelContext<Self>) {
        let history = BlocklistAIHistoryModel::handle(ctx);
        for row in rows {
            let conversation_id = AIConversationId::try_from(row.conversation_id.clone()).ok();
            let exists = conversation_id.is_some_and(|id| {
                history.read(ctx, |history, _| {
                    history.conversation(&id).is_some()
                        || history.get_conversation_metadata(&id).is_some()
                })
            });
            let query = QueuedQuery::from_persisted_row(&row);
            match conversation_id.filter(|_| exists) {
                Some(id) => {
                    self.queues.entry(id).or_default().queue.push(query);
                    self.restored_suspended += 1;
                }
                None => {
                    if !self.orphan_conversation_ids.contains(&row.conversation_id) {
                        self.orphan_conversation_ids
                            .push(row.conversation_id.clone());
                    }
                    self.restored_orphans.push(query);
                }
            }
        }
    }

    /// Hands over what [`Self::restore_persisted`] found, exactly once. The orphans' stored
    /// rows are deleted here, after the caller has them in hand, so a crash before any view
    /// existed leaves them for the next launch.
    pub fn take_restored_session(&mut self, ctx: &mut ModelContext<Self>) -> RestoredQueueSession {
        let session = RestoredQueueSession {
            suspended_count: std::mem::take(&mut self.restored_suspended),
            orphans: std::mem::take(&mut self.restored_orphans),
        };
        for stored_conversation_id in std::mem::take(&mut self.orphan_conversation_ids) {
            Self::send_queue_rows(stored_conversation_id, Vec::new(), ctx);
        }
        session
    }

    fn handle_history_event(
        &mut self,
        event: &BlocklistAIHistoryEvent,
        ctx: &mut ModelContext<Self>,
    ) {
        match event {
            BlocklistAIHistoryEvent::RemoveConversation {
                conversation_id,
                terminal_view_id,
            } => {
                self.drop_conversation(*conversation_id, Some(*terminal_view_id), ctx);
            }
            BlocklistAIHistoryEvent::DeletedConversation {
                conversation_id,
                terminal_view_id,
                ..
            } => {
                self.drop_conversation(*conversation_id, *terminal_view_id, ctx);
            }
            BlocklistAIHistoryEvent::ClearedConversationsInTerminalView {
                terminal_view_id,
                ..
            } => {
                // Upstream Warp's clear event carries the cleared ids directly; Zap's
                // `ClearedConversationsInTerminalView` does not, so read them back from the
                // history model. That map accumulates every cleared id for the view, so
                // dropping already-dropped conversations is a harmless no-op.
                let cleared = BlocklistAIHistoryModel::handle(ctx).read(ctx, |history, _| {
                    history
                        .cleared_conversation_ids_for_terminal_view(*terminal_view_id)
                        .to_vec()
                });
                for conversation_id in cleared {
                    self.drop_conversation(conversation_id, Some(*terminal_view_id), ctx);
                }
            }
            _ => {}
        }
    }

    fn drop_conversation(
        &mut self,
        conversation_id: AIConversationId,
        terminal_view_id: Option<EntityId>,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Some(state) = self.queues.remove(&conversation_id) {
            // Never drop queued prompts silently (#1026): hand them to the owning view first.
            if !state.queue.is_empty() {
                ctx.emit(QueuedQueryEvent::DroppedUnsent {
                    conversation_id,
                    terminal_view_id,
                    rows: state.queue,
                });
            }
            ctx.emit(QueuedQueryEvent::Cleared { conversation_id });
            self.persist_queue(conversation_id, ctx);
        }
    }

    /// Returns the queue for `conversation_id`. Returns an empty slice when no entry exists.
    pub fn queue(&self, conversation_id: AIConversationId) -> &[QueuedQuery] {
        self.queues
            .get(&conversation_id)
            .map(|state| state.queue.as_slice())
            .unwrap_or(&[])
    }

    /// Returns true when `conversation_id` has at least one queued prompt.
    pub fn has_queue(&self, conversation_id: AIConversationId) -> bool {
        self.queues
            .get(&conversation_id)
            .is_some_and(|state| !state.queue.is_empty())
    }

    /// Returns true when a queued row would auto-fire for `conversation_id` the next time the
    /// conversation finishes successfully. Mirrors [`Self::peek_autofire`]'s gating: false for an
    /// empty queue or a locked head row (which never auto-fires).
    pub fn has_autofireable_prompt(&self, conversation_id: AIConversationId) -> bool {
        self.queues
            .get(&conversation_id)
            .and_then(|state| state.queue.iter().find(|row| !row.suspended))
            .is_some_and(|first| !first.is_locked())
    }

    /// Marks that a dispatched queued command is running for `conversation_id`. While set, the
    /// queue keeps accepting new rows (the agent is idle) and the next drain waits for the
    /// command to finish.
    pub fn arm_command_in_flight(&mut self, conversation_id: AIConversationId) {
        self.queues
            .entry(conversation_id)
            .or_default()
            .command_in_flight = true;
    }

    /// Clears the in-flight-command marker for `conversation_id`.
    pub fn clear_command_in_flight(&mut self, conversation_id: AIConversationId) {
        if let Some(state) = self.queues.get_mut(&conversation_id) {
            state.command_in_flight = false;
        }
    }

    /// Returns true while a dispatched queued command is running for `conversation_id`.
    pub fn has_command_in_flight(&self, conversation_id: AIConversationId) -> bool {
        self.queues
            .get(&conversation_id)
            .is_some_and(|state| state.command_in_flight)
    }

    /// Returns the conversation owned by `terminal_view_id` that currently has a queued command in
    /// flight, if any.
    pub fn command_in_flight_for_terminal_view(
        &self,
        terminal_view_id: EntityId,
        history_model: &BlocklistAIHistoryModel,
    ) -> Option<AIConversationId> {
        history_model
            .all_live_conversations_for_terminal_view(terminal_view_id)
            .find_map(|conversation| {
                self.has_command_in_flight(conversation.id())
                    .then_some(conversation.id())
            })
    }

    /// Returns the row currently in edit mode for `conversation_id`, if any.
    pub fn editing_row(&self, conversation_id: AIConversationId) -> Option<QueuedQueryId> {
        self.queues
            .get(&conversation_id)
            .and_then(|state| state.editing)
    }

    /// Returns true when the head row of `conversation_id`'s queue is currently being edited.
    pub fn first_row_is_in_edit_mode(&self, conversation_id: AIConversationId) -> bool {
        let Some(state) = self.queues.get(&conversation_id) else {
            return false;
        };
        let Some(editing_id) = state.editing else {
            return false;
        };
        state.queue.first().is_some_and(|q| q.id == editing_id)
    }

    /// Returns the effective auto-queue state for `conversation_id`, given the terminal's
    /// `active_block`.
    pub fn is_queue_next_prompt_enabled(
        &self,
        conversation_id: AIConversationId,
        active_block: &Block,
        app: &AppContext,
    ) -> bool {
        if is_lrc_auto_queue_active(active_block, conversation_id, app) {
            // While an agent controls the active agent-requested command, the command-scoped
            // toggle governs queueing.
            self.is_queue_next_prompt_enabled_during_lrc(conversation_id)
        } else {
            // Otherwise the per-conversation toggle governs queueing.
            self.is_queue_next_prompt_toggle_enabled(conversation_id)
        }
    }

    /// Auto-queue state while an eligible agent-requested long-running command is active: on unless
    /// toggled off for the duration of the command.
    fn is_queue_next_prompt_enabled_during_lrc(&self, conversation_id: AIConversationId) -> bool {
        self.queues
            .get(&conversation_id)
            .and_then(|state| state.queue_next_lrc_prompt_override)
            .unwrap_or(true)
    }

    /// Per-conversation auto-queue toggle state, ignoring any long-running-command override:
    /// the explicit toggle when set, otherwise on when the default submission mode is `Queue`.
    pub(crate) fn is_queue_next_prompt_toggle_enabled(
        &self,
        conversation_id: AIConversationId,
    ) -> bool {
        self.queues
            .get(&conversation_id)
            .and_then(|state| state.queue_next_prompt_override)
            .unwrap_or(self.default_mode == PromptSubmissionMode::Queue)
    }

    /// Toggles the per-conversation auto-queue state. Computes the effective
    /// current value (which may come from the cached default) before writing
    /// its inverse as an explicit override, so toggling from the setting-driven
    /// default flips correctly.
    pub fn toggle_queue_next_prompt(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let current = self.is_queue_next_prompt_toggle_enabled(conversation_id);
        let state = self.queues.entry(conversation_id).or_default();
        state.queue_next_prompt_override = Some(!current);
        ctx.emit(QueuedQueryEvent::QueueNextPromptToggled { conversation_id });
    }

    /// Makes the "queue next prompt" toggle one-shot (#1030): once a prompt has been queued
    /// because the toggle was on, the toggle turns itself off again. Only an explicit "on" is
    /// consumed, and only when the default submission mode is not `Queue`: with the persistent
    /// setting on, queueing is the standing mode, not a one-time choice, and is left alone.
    /// Returns whether the toggle was cleared.
    pub fn consume_queue_next_prompt_toggle(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        if self.default_mode == PromptSubmissionMode::Queue {
            return false;
        }
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return false;
        };
        if state.queue_next_prompt_override != Some(true) {
            return false;
        }
        state.queue_next_prompt_override = None;
        ctx.emit(QueuedQueryEvent::QueueNextPromptToggled { conversation_id });
        true
    }

    /// Toggles the auto-queue state for the duration of the eligible long-running command.
    pub fn toggle_queue_next_prompt_during_lrc(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let current = self.is_queue_next_prompt_enabled_during_lrc(conversation_id);
        let state = self.queues.entry(conversation_id).or_default();
        state.queue_next_lrc_prompt_override = Some(!current);
        ctx.emit(QueuedQueryEvent::QueueNextPromptToggled { conversation_id });
    }

    /// Clears the LRC-scoped auto-queue override when the long-running command ends, so the
    /// conversation reverts to its pre-command queue state.
    pub fn clear_queue_next_lrc_prompt_override(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        if state.queue_next_lrc_prompt_override.take().is_some() {
            ctx.emit(QueuedQueryEvent::QueueNextPromptToggled { conversation_id });
        }
    }

    /// Transitions all `PendingLrcAutoQueue` rows for `conversation_id` to `LrcAutoQueue`,
    /// unlocking them for auto-fire when the command completes. Emits `RowUnlocked` if any
    /// rows were changed.
    pub fn unlock_pending_lrc_rows(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let mut unlocked = false;
        for row in state.queue.iter_mut() {
            if row.origin == QueuedQueryOrigin::PendingLrcAutoQueue {
                row.origin = QueuedQueryOrigin::LrcAutoQueue;
                unlocked = true;
            }
        }
        if unlocked {
            ctx.emit(QueuedQueryEvent::RowUnlocked { conversation_id });
            self.persist_queue(conversation_id, ctx);
        }
    }

    /// Transitions all `PendingApprovalFollowUp` rows for `conversation_id` to
    /// `ApprovalFollowUp`, unlocking them for manual push and auto-fire now that the blocked
    /// action they were waiting on has resolved. Emits `RowUnlocked` if any rows were changed.
    ///
    /// Called from the same `drain_queued_prompts` sites that already call
    /// [`Self::unlock_pending_lrc_rows`] (jwp2987/phosphor#690 follow-up): a turn `Complete`
    /// auto-fires the now-unlocked head through the ordinary path, matching "approved and
    /// finished" (the conversation continues to some other terminal status); a turn
    /// `Error`/`Cancelled` unlocks but only restores the head into the input when the user is
    /// looking at the conversation, otherwise leaving it queued -- never a silent send for a
    /// genuinely cancelled conversation.
    ///
    /// Also called directly from `BlocklistAIController`'s `FinishedAction` subscriber
    /// (jwp2987/phosphor#790, a #725 follow-up) for a rejection (or any other blocked-action
    /// resolution) that does not trigger a follow-up request: that path writes the
    /// conversation's terminal status without ever producing a `FinishedReceivingOutput` event,
    /// so none of the `drain_queued_prompts` call sites above would otherwise run for it and
    /// the row would stay locked forever. That call site unlocks only -- the row gets ordinary
    /// queued-prompt treatment afterward, never an automatic send.
    pub fn unlock_pending_approval_rows(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let mut unlocked = false;
        for row in state.queue.iter_mut() {
            if row.origin == QueuedQueryOrigin::PendingApprovalFollowUp {
                row.origin = QueuedQueryOrigin::ApprovalFollowUp;
                unlocked = true;
            }
        }
        if unlocked {
            ctx.emit(QueuedQueryEvent::RowUnlocked { conversation_id });
            self.persist_queue(conversation_id, ctx);
        }
    }

    /// Removes all `PendingLrcAutoQueue` rows for `conversation_id` so stale locked
    /// rows do not linger.
    pub fn remove_pending_lrc_rows(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let mut removed_ids = Vec::new();
        state.queue.retain(|row| {
            if row.origin == QueuedQueryOrigin::PendingLrcAutoQueue {
                removed_ids.push(row.id);
                false
            } else {
                true
            }
        });
        let changed = !removed_ids.is_empty();
        for query_id in removed_ids {
            ctx.emit(QueuedQueryEvent::Removed {
                conversation_id,
                query_id,
            });
        }
        if changed {
            self.persist_queue(conversation_id, ctx);
        }
    }

    /// Appends `query` to the tail of `conversation_id`'s queue.
    pub fn append(
        &mut self,
        conversation_id: AIConversationId,
        query: QueuedQuery,
        ctx: &mut ModelContext<Self>,
    ) -> QueuedQueryId {
        let query_id = query.id;
        let state = self.queues.entry(conversation_id).or_default();
        state.queue.push(query);
        ctx.emit(QueuedQueryEvent::Appended {
            conversation_id,
            query_id,
        });
        self.persist_queue(conversation_id, ctx);
        query_id
    }

    /// Pops the first row in `conversation_id`'s queue and returns it.
    /// Used by the non-clean drain path (Error / Cancelled) to restore a single popped
    /// prompt to the input editor. No-ops when the head is locked
    /// ([`QueuedQuery::is_locked`]) so a status-transition arriving before a locked
    /// `PendingLrcAutoQueue` or `PendingApprovalFollowUp` (jwp2987/phosphor#690 follow-up) row
    /// is unlocked cannot clobber it.
    pub fn pop_front(
        &mut self,
        conversation_id: AIConversationId,
        ctx: &mut ModelContext<Self>,
    ) -> Option<QueuedQuery> {
        let state = self.queues.get_mut(&conversation_id)?;
        // A restored (suspended) row stays in the panel for the user to act on; the first
        // row that is not restored is the one a cancel/error drain returns to the input.
        let index = state.queue.iter().position(|row| !row.suspended)?;
        if state.queue[index].is_locked() {
            return None;
        }
        let popped = state.queue.remove(index);
        if state.editing == Some(popped.id) {
            state.editing = None;
        }
        ctx.emit(QueuedQueryEvent::Removed {
            conversation_id,
            query_id: popped.id,
        });
        self.persist_queue(conversation_id, ctx);
        Some(popped)
    }

    /// Auto-fire drain entry point for `conversation_id`. Returns the action for the head row
    /// *without* removing it (so the send path can read its attachments by id), or `None` for an
    /// empty queue or a locked head ([`QueuedQuery::is_locked`]). The caller removes the row via
    /// [`Self::remove_fired_row`] once it has been dispatched or restored to the input.
    pub fn peek_autofire(&self, conversation_id: AIConversationId) -> Option<AutofireAction> {
        let state = self.queues.get(&conversation_id)?;
        // Restored (suspended) rows are skipped, not treated as a locked head: a prompt queued
        // behind them this session must still drain, or it would sit there forever.
        let first = state.queue.iter().find(|row| !row.suspended)?;
        if first.is_locked() {
            return None;
        }
        let first_in_edit_mode = state.editing == Some(first.id);
        Some(if first_in_edit_mode {
            AutofireAction::PopFromEditMode {
                query_id: first.id,
                text: first.text.clone(),
                attachments: first.attachments().to_vec(),
                is_command: first.is_command(),
            }
        } else if first.is_command() {
            AutofireAction::ExecuteCommand {
                query_id: first.id,
                command: first.text.clone(),
            }
        } else {
            AutofireAction::Submit {
                query_id: first.id,
                text: first.text.clone(),
            }
        })
    }

    /// Removes the row `query_id` from `conversation_id`'s queue after it has been fired. In the
    /// edit-mode auto-fire path, the caller first restores the row's committed text and
    /// attachments to the input, then calls this to drop the row and clear edit state.
    pub fn remove_fired_row(
        &mut self,
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let Some(idx) = state.queue.iter().position(|q| q.id == query_id) else {
            return;
        };
        state.queue.remove(idx);
        if state.editing == Some(query_id) {
            state.editing = None;
        }
        ctx.emit(QueuedQueryEvent::Removed {
            conversation_id,
            query_id,
        });
        self.persist_queue(conversation_id, ctx);
    }

    /// Restores a fired row when submission fails after the row was removed.
    pub(crate) fn restore_fired_row(
        &mut self,
        conversation_id: AIConversationId,
        insert_index: usize,
        query: QueuedQuery,
        ctx: &mut ModelContext<Self>,
    ) {
        let state = self.queues.entry(conversation_id).or_default();
        let query_id = query.id;
        if state.queue.iter().any(|queued| queued.id == query_id) {
            return;
        }
        let insert_index = insert_index.min(state.queue.len());
        state.queue.insert(insert_index, query);
        ctx.emit(QueuedQueryEvent::Appended {
            conversation_id,
            query_id,
        });
        self.persist_queue(conversation_id, ctx);
    }

    /// Returns the attachments captured on the queued row `query_id` within `conversation_id`'s
    /// queue, or an empty slice if no such row exists. Used by the send path to attach a fired
    /// queued prompt's images/files without removing the row first.
    pub fn attachments_for(
        &self,
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
    ) -> &[PendingAttachment] {
        self.queues
            .get(&conversation_id)
            .and_then(|state| state.queue.iter().find(|q| q.id == query_id))
            .map(QueuedQuery::attachments)
            .unwrap_or(&[])
    }

    /// Removes a specific row by id within `conversation_id`'s queue, if present. Returns the
    /// removed row. No-ops when the target row is locked ([`QueuedQuery::is_locked`]); a locked
    /// `PendingLrcAutoQueue` row becomes removable once its action snapshot fires and it is
    /// unlocked via [`Self::unlock_pending_lrc_rows`].
    pub fn remove_by_id(
        &mut self,
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
        ctx: &mut ModelContext<Self>,
    ) -> Option<QueuedQuery> {
        let state = self.queues.get_mut(&conversation_id)?;
        let idx = state.queue.iter().position(|q| q.id == query_id)?;
        // Deliberately not gated on `is_locked()`. The lock exists to stop a row being
        // *submitted* while an action snapshot is pending -- that is the CliAgentUserQuery/LRC
        // race. Removing a row cannot race anything: a row that is gone never fires, which is
        // strictly safer than leaving it queued. Refusing here left the user staring at a row
        // they could not delete for as long as the command ran, with no feedback at all, and
        // the panel agrees -- it disables only the Send Now button for these rows
        // (`queued_prompts_panel.rs:400`), never the trash.
        let removed = state.queue.remove(idx);
        if state.editing == Some(query_id) {
            state.editing = None;
        }
        ctx.emit(QueuedQueryEvent::Removed {
            conversation_id,
            query_id,
        });
        self.persist_queue(conversation_id, ctx);
        Some(removed)
    }

    /// Moves the row identified by `source_id` to position `target_index` within
    /// `conversation_id`'s queue. `target_index` is interpreted as the index in the post-removal
    /// list and is clamped to the queue length. No-ops when the source row is locked
    /// ([`QueuedQuery::is_locked`]) or when the move would displace a locked row off the head of
    /// the queue.
    pub fn reorder(
        &mut self,
        conversation_id: AIConversationId,
        source_id: QueuedQueryId,
        target_index: usize,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let Some(source_idx) = state.queue.iter().position(|q| q.id == source_id) else {
            return;
        };
        let head_is_locked = state.queue.first().is_some_and(|row| row.is_locked());
        if state.queue[source_idx].is_locked() || (target_index == 0 && head_is_locked) {
            return;
        }
        let row = state.queue.remove(source_idx);
        let clamped = target_index.min(state.queue.len());
        state.queue.insert(clamped, row);
        ctx.emit(QueuedQueryEvent::Reordered { conversation_id });
        self.persist_queue(conversation_id, ctx);
    }

    /// Enters edit mode for `query_id` in `conversation_id`'s queue. If another row was being
    /// edited, that edit is cancelled (its text is unchanged, per the spec). No-ops when the
    /// target row is locked ([`QueuedQuery::is_locked`]).
    pub fn enter_edit_mode(
        &mut self,
        conversation_id: AIConversationId,
        query_id: QueuedQueryId,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        // Also not gated on `is_locked()`, for the same reason as `remove_by_id`: editing a
        // row's text does not submit it. The row still cannot fire while locked, because
        // `peek_autofire` and `pop_front` keep their guards.
        if !state.queue.iter().any(|q| q.id == query_id) {
            return;
        }
        let prev_edit = state.editing.replace(query_id);
        if let Some(prev) = prev_edit
            && prev != query_id
        {
            ctx.emit(QueuedQueryEvent::EditCancelled {
                conversation_id,
                query_id: prev,
            });
        }
        ctx.emit(QueuedQueryEvent::EditEntered {
            conversation_id,
            query_id,
        });
    }

    /// Commits the in-progress edit in `conversation_id` by replacing the row's text with
    /// `new_text` and clearing edit state. An empty `new_text` cancels the edit and leaves the
    /// original row text untouched.
    pub fn commit_edit(
        &mut self,
        conversation_id: AIConversationId,
        new_text: String,
        ctx: &mut ModelContext<Self>,
    ) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let Some(query_id) = state.editing.take() else {
            return;
        };
        if new_text.is_empty() {
            ctx.emit(QueuedQueryEvent::EditCancelled {
                conversation_id,
                query_id,
            });
            return;
        }
        if let Some(row) = state.queue.iter_mut().find(|q| q.id == query_id) {
            row.text = new_text;
        }
        ctx.emit(QueuedQueryEvent::EditCommitted {
            conversation_id,
            query_id,
        });
        self.persist_queue(conversation_id, ctx);
    }

    /// Cancels the in-progress edit in `conversation_id` without modifying the row's text.
    pub fn cancel_edit(&mut self, conversation_id: AIConversationId, ctx: &mut ModelContext<Self>) {
        let Some(state) = self.queues.get_mut(&conversation_id) else {
            return;
        };
        let Some(query_id) = state.editing.take() else {
            return;
        };
        ctx.emit(QueuedQueryEvent::EditCancelled {
            conversation_id,
            query_id,
        });
    }
}

/// Returns true when queue mode is auto-enabled for `conversation_id`: an agent controls
/// `active_block`'s agent-requested long-running command, and the user's settings opt into
/// queueing prompts for the duration of such commands.
pub(crate) fn is_lrc_auto_queue_active(
    active_block: &Block,
    conversation_id: AIConversationId,
    app: &AppContext,
) -> bool {
    let ai_settings = AISettings::as_ref(app);
    FeatureFlag::QueueSlashCommand.is_enabled()
        && ai_settings.default_prompt_submission_mode == PromptSubmissionMode::Interrupt
        && ai_settings.long_running_command_submission_mode
            == LongRunningCommandSubmissionMode::QueueUntilCommandCompletes
        && active_block.is_agent_in_control()
        && active_block.is_agent_requested_command()
        && active_block.ai_conversation_id() == Some(conversation_id)
}

/// Placeholder shown in an empty input while queue mode is armed (#1043). `shortcut` is the
/// displayed keybinding for the toggle (from the keymap, so Mac shows its own glyphs).
pub(crate) fn queue_mode_armed_hint(shortcut: Option<&str>) -> String {
    match shortcut {
        Some(shortcut) => crate::t!("terminal-input-queue-mode-armed-hint", shortcut = shortcut),
        None => crate::t!("terminal-input-queue-mode-armed-hint-no-shortcut"),
    }
}

/// Label on the queue chip: reads as a state rather than a bare shortcut.
pub(crate) fn queue_mode_chip_label(is_on: bool) -> String {
    if is_on {
        crate::t!("terminal-queue-mode-chip-on")
    } else {
        crate::t!("terminal-queue-mode-chip-off")
    }
}

/// Ephemeral toast text shown when queue mode is toggled.
pub(crate) fn queue_mode_toast_text(is_on: bool, shortcut: Option<&str>) -> String {
    match (is_on, shortcut) {
        (true, Some(shortcut)) => crate::t!("terminal-queue-mode-toast-on", shortcut = shortcut),
        (false, Some(shortcut)) => crate::t!("terminal-queue-mode-toast-off", shortcut = shortcut),
        (true, None) => crate::t!("terminal-queue-mode-toast-on-no-shortcut"),
        (false, None) => crate::t!("terminal-queue-mode-toast-off-no-shortcut"),
    }
}

#[cfg(test)]
#[path = "queued_query_tests.rs"]
mod tests;
