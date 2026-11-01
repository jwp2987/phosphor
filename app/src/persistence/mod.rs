#![cfg_attr(not(feature = "local_fs"), allow(dead_code))]

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        pub mod agent;
        mod block_list;
        mod cloud_objects;
        mod sqlite;
        pub mod commands;
    }
}

mod database_notice;
pub use database_notice::{DatabaseProblem, DatabaseProblemNotice};
#[cfg(test)]
pub(crate) use database_notice::record_partial_restore;

pub use persistence::model;
#[cfg_attr(not(feature = "local_fs"), expect(unused_imports))]
pub use persistence::schema;

#[cfg(feature = "integration_tests")]
pub mod testing;

use instant::Instant;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::SyncSender;
use std::sync::Arc;
use std::thread::JoinHandle;

use ai::project_context::model::ProjectRulePath;
use chrono::{DateTime, Local, Utc};
use uuid::Uuid;
use warp_core::command::ExitCode;
use warp_multi_agent_api as api;
use warpui::{AppContext, Entity, SingletonEntity};

use lsp::supported_servers::LSPServerType;

use crate::ai::blocklist::PersistedAIInput;
use crate::ai::mcp::TemplatableMCPServerInstallation;
use crate::ai::persisted_workspace::EnablementState;
use crate::app_state::AppState;
use crate::auth::PersistedCurrentUserInformation;
use crate::cloud_object::model::actions::ObjectAction;
use crate::cloud_object::model::generic_string_model::StoredStringObject;

use crate::cloud_object::{ObjectIdType, StoredObject, StoredObjectMetadata};
use crate::drive::folders::FolderObject;
use crate::notebooks::NotebookObject;
use crate::server::experiments::ServerExperiment;
use crate::server::ids::SyncId;
use crate::suggestions::ignored_suggestions_model::SuggestionType;
use crate::terminal::history::PersistedCommand;
use crate::terminal::model::block::{
    DeferredBlockContent, SerializedAgentViewVisibility, SerializedBlock,
};
use crate::terminal::model::session::SessionId;
use crate::workflows::WorkflowObject;
use crate::workspaces::user_profiles::UserProfileWithUID;
use crate::workspaces::workspace::{Workspace as WorkspaceMetadata, WorkspaceUid};
// Aliased because `crate::workspaces::workspace::Workspace` is already imported
// above as `WorkspaceMetadata`. That one is a (cloud) team workspace; this one is
// a local repository root tracked by `PersistedWorkspace`.
use ai::workspace::WorkspaceMetadata as CodeWorkspaceMetadata;

use self::model::{AgentConversation, AgentConversationData, Project};

#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::database_file_path;
#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::establish_ro_connection;
#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::shared_ro_connection;
/// Read paths for the codebase embedding index. New in this fork — at the pin
/// this data was on the server, so nothing local read it.
#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::{
    codebase_index_children, codebase_index_has_embeddings, codebase_index_node_summaries,
    codebase_index_vectors, known_codebase_index_hashes,
};
/// Direct write paths for the codebase embedding index, plus the standalone
/// connection that owns them. Used only by the remote-server daemon, which has
/// no `PersistenceWriter` to route writes through — see
/// [`sqlite::establish_codebase_index_connection`].
#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::{
    establish_codebase_index_connection, save_codebase_index_embeddings,
    save_codebase_index_node_summaries, save_codebase_index_nodes,
};

/// Initializes the persistence "subsystem".
///
/// Returns the previously-persisted data, if any, and handles for
/// writing updated data to persist, if the persistence subsystem is
/// available.
#[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
pub fn initialize(ctx: &mut AppContext) -> (Option<PersistedData>, Option<WriterHandles>) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "local_fs")] {
            sqlite::initialize(ctx)
        } else {
            (None, None)
        }
    }
}

/// If startup found `warp.sqlite` unusable and moved it aside (#860), returns
/// where it went -- once -- so the UI can tell the user.
pub fn take_database_recovery_notice() -> Option<PathBuf> {
    cfg_if::cfg_if! {
        if #[cfg(feature = "local_fs")] {
            sqlite::take_database_recovery_notice().map(|notice| notice.moved_to)
        } else {
            None
        }
    }
}

/// If startup could not open `warp.sqlite` (read-only file or folder, full
/// disk, ...), or writes to it started failing, returns -- once -- what to tell
/// the user. Never involves moving the database (#860).
pub fn take_database_problem_notice() -> Option<DatabaseProblemNotice> {
    database_notice::take_database_problem_notice()
}

/// Whether writes succeeded again after a "database is locked" notice was
/// raised -- once -- so the UI can withdraw that toast (#971).
pub fn take_database_lock_recovered() -> bool {
    database_notice::take_lock_recovered()
}

/// Returns, once, a receiver that gets a message whenever a database notice is
/// recorded, so the UI can show it without waiting for a window change (#860).
pub fn take_database_notice_wake_receiver() -> Option<async_channel::Receiver<()>> {
    database_notice::take_notice_wake_receiver()
}

/// Prewarms the SQLite connection + runs migrations on a background thread.
/// Should be called before `app_builder.run`, so SQLite initialization can
/// run concurrently with winit / wgpu initialization, saving ~70-90ms on cold
/// start. Calling multiple times is idempotent. Not calling it falls back to the synchronous path.
#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
pub fn prewarm_db_in_background() {
    cfg_if::cfg_if! {
        if #[cfg(feature = "local_fs")] {
            sqlite::prewarm_db_in_background();
        }
    }
}

// Remove sqlite database as part of Logout v0.
// TODO: Implement per user scoping of sqlite.
#[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
pub fn remove(sender: &Option<SyncSender<ModelEvent>>) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "local_fs")] {
            if let Some(sender) = sender.clone() {
                sqlite::remove(sender);
            }
        } else {
            log::info!("Local filesystem persistence is not enabled.");
        }
    }
}

// Reconstruct sqlite database as part of Logout v0.
#[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
pub fn reconstruct(sender: &Option<SyncSender<ModelEvent>>) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "local_fs")] {
            if let Some(sender) = sender.clone() {
                sqlite::reconstruct(sender);
            }
        } else {
            log::info!("Local filesystem persistence is not enabled.");
        }
    }
}

/// Holds interfaces to the writer thread.
pub struct WriterHandles {
    pub handle: JoinHandle<()>,
    pub sender: SyncSender<ModelEvent>,
}

/// Model for interacting with the writer thread.
pub struct PersistenceWriter {
    thread_handle: Option<JoinHandle<()>>,
    model_event_sender: Option<SyncSender<ModelEvent>>,
}

impl PersistenceWriter {
    pub fn new(handle: Option<WriterHandles>) -> Self {
        let (thread_handle, model_event_sender) = match handle {
            Some(handle) => (Some(handle.handle), Some(handle.sender)),
            None => (None, None),
        };
        Self {
            thread_handle,
            model_event_sender,
        }
    }

    /// Sending half for sending model updates to the persistence writer thread.
    pub fn sender(&self) -> Option<SyncSender<ModelEvent>> {
        self.model_event_sender.clone()
    }

    /// Synchronously terminate the SQLite writer thread.
    pub fn terminate(&mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let start = Instant::now();
            let Some(sender) = self.sender() else {
                log::error!("Model event sender should exist if thread handle is set");
                return;
            };
            // Writes that found the channel full are still on their way (#1251).
            if !wait_for_deferred_sends(DEFERRED_SENDS_QUIT_WAIT) {
                log::warn!(
                    "{} deferred SQLite write(s) did not reach the writer within {:?}; stopping \
                     the writer without them",
                    outstanding_deferred_sends(),
                    DEFERRED_SENDS_QUIT_WAIT
                );
            }
            if let Err(err) = sender.send(ModelEvent::Terminate) {
                log::error!("Could not terminate SQLite writer thread: {err}");
            }
            if handle.join().is_err() {
                // When crash reporting is enabled, the panic hook has already
                // written to the local log; this adds thread-level context.
                log::error!("SQLite writer thread panicked");
            }
            log::info!("Shut down SQLite writer in {:?}", start.elapsed());
        }
    }
}

impl Drop for PersistenceWriter {
    fn drop(&mut self) {
        self.terminate();
    }
}

/// The answer side of [`ModelEvent::WriteBarrier`]: the writer sends `()` when it reaches the
/// barrier; a dropped sender (the writer stopped, the event was dropped from a full retry
/// queue) cancels the receiver, which callers treat as "not written".
pub struct WriteBarrier(futures::channel::oneshot::Sender<()>);

impl WriteBarrier {
    /// A barrier and the receiver that resolves when the writer reaches it.
    pub fn new() -> (Self, futures::channel::oneshot::Receiver<()>) {
        let (sender, receiver) = futures::channel::oneshot::channel();
        (Self(sender), receiver)
    }

    /// Answers the barrier. Called by the writer thread, in event order.
    pub(crate) fn reached(self) {
        let _ = self.0.send(());
    }
}

impl std::fmt::Debug for WriteBarrier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WriteBarrier")
    }
}

/// Sends still parked on "SQLite deferred send" threads (see [`send_model_event_nonblocking`]).
/// [`PersistenceWriter::terminate`] waits for them before it sends `Terminate`, or a write that
/// was only waiting for room in the channel would arrive after the writer stopped and be lost
/// (#1251).
static DEFERRED_SENDS: std::sync::Mutex<usize> = std::sync::Mutex::new(0);
static DEFERRED_SENDS_CHANGED: std::sync::Condvar = std::sync::Condvar::new();

fn note_deferred_send_started() {
    let mut count = DEFERRED_SENDS.lock().unwrap_or_else(|e| e.into_inner());
    *count += 1;
}

fn note_deferred_send_finished() {
    let mut count = DEFERRED_SENDS.lock().unwrap_or_else(|e| e.into_inner());
    *count = count.saturating_sub(1);
    DEFERRED_SENDS_CHANGED.notify_all();
}

/// How many deferred sends have not reached the channel yet.
pub(crate) fn outstanding_deferred_sends() -> usize {
    *DEFERRED_SENDS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Waits until no deferred send is outstanding, at most `timeout`. Returns whether all of them
/// reached the channel.
pub(crate) fn wait_for_deferred_sends(timeout: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + timeout;
    let mut count = DEFERRED_SENDS.lock().unwrap_or_else(|e| e.into_inner());
    while *count > 0 {
        let now = std::time::Instant::now();
        if now >= deadline {
            return false;
        }
        count = DEFERRED_SENDS_CHANGED
            .wait_timeout(count, deadline - now)
            .unwrap_or_else(|e| e.into_inner())
            .0;
    }
    true
}

/// Longest the quit path waits for deferred sends before it stops the writer anyway: a writer
/// wedged on a lock would otherwise hold the quit for as long as the lock.
const DEFERRED_SENDS_QUIT_WAIT: std::time::Duration = std::time::Duration::from_secs(2);

/// The newest session snapshot that found the writer channel full, waiting for room. One slot:
/// a newer snapshot replaces an older one that has not been sent yet, and while the delivering
/// thread is alive every snapshot goes through the slot, so an older snapshot can never reach the
/// writer after a newer one (#1251).
static DEFERRED_SNAPSHOT: std::sync::Mutex<DeferredSnapshot> =
    std::sync::Mutex::new(DeferredSnapshot {
        state: None,
        thread_running: false,
    });

struct DeferredSnapshot {
    state: Option<AppState>,
    thread_running: bool,
}

/// Sends a session snapshot without ever blocking the caller. On a full channel the snapshot is
/// kept (newest wins) and delivered by a short-lived thread as soon as there is room, so the last
/// snapshot before a quit is not skipped when the writer is behind (#1251). Delivery counts as a
/// deferred send for [`wait_for_deferred_sends`].
pub fn send_snapshot_nonblocking(sender: &SyncSender<ModelEvent>, state: AppState) {
    let mut deferred = DEFERRED_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
    let state = if deferred.thread_running {
        state
    } else {
        match sender.try_send(ModelEvent::Snapshot(state)) {
            Ok(()) => return,
            Err(std::sync::mpsc::TrySendError::Full(ModelEvent::Snapshot(state))) => state,
            Err(std::sync::mpsc::TrySendError::Full(_)) => return,
            Err(err @ std::sync::mpsc::TrySendError::Disconnected(_)) => {
                log::error!("Error trying to send model event {err:?}");
                return;
            }
        }
    };
    log::warn!("SQLite writer channel is full; the session snapshot waits for room");
    deferred.state = Some(state);
    if deferred.thread_running {
        return;
    }
    deferred.thread_running = true;
    note_deferred_send_started();
    let sender = sender.clone();
    let spawned = std::thread::Builder::new()
        .name("SQLite deferred snapshot".to_owned())
        .spawn(move || {
            loop {
                let state = {
                    let mut deferred = DEFERRED_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
                    match deferred.state.take() {
                        Some(state) => state,
                        None => {
                            deferred.thread_running = false;
                            break;
                        }
                    }
                };
                if let Err(err) = sender.send(ModelEvent::Snapshot(state)) {
                    log::warn!("Deferred session snapshot send failed: {err:?}");
                    let mut deferred = DEFERRED_SNAPSHOT.lock().unwrap_or_else(|e| e.into_inner());
                    deferred.state = None;
                    deferred.thread_running = false;
                    break;
                }
            }
            note_deferred_send_finished();
        });
    if spawned.is_err() {
        // No thread to deliver it: dropped, like a skipped snapshot before #1251, rather than
        // left in the slot to be sent after a newer one.
        deferred.state = None;
        deferred.thread_running = false;
        note_deferred_send_finished();
        log::warn!("Could not start the deferred snapshot thread; this session snapshot is skipped");
    }
}

/// Hands `event` to the writer thread without ever blocking the caller.
///
/// The channel is bounded, and `SyncSender::send` parks the calling thread while
/// it is full, which on the main thread freezes the UI behind a wedged writer
/// (#1155). When the channel is full the event is returned so the caller can
/// deliver it off-thread; a disconnected writer is an error. Nothing is dropped.
pub fn try_send_or_defer(
    sender: &SyncSender<ModelEvent>,
    event: ModelEvent,
) -> Result<Option<ModelEvent>, std::sync::mpsc::SendError<ModelEvent>> {
    match sender.try_send(event) {
        Ok(()) => Ok(None),
        Err(std::sync::mpsc::TrySendError::Full(event)) => Ok(Some(event)),
        Err(std::sync::mpsc::TrySendError::Disconnected(event)) => {
            Err(std::sync::mpsc::SendError(event))
        }
    }
}

/// Events that found the writer channel full, in the order they were produced, and whether the
/// single delivery thread is alive. While that thread is alive (or events are queued) EVERY event
/// sent through [`send_model_event_nonblocking`] goes through this queue, so a later event can
/// never overtake an earlier deferred one (#1000, #1367).
static ORDERED_SENDS: std::sync::Mutex<OrderedSends> = std::sync::Mutex::new(OrderedSends {
    queue: std::collections::VecDeque::new(),
    thread_running: false,
});

struct OrderedSends {
    queue: std::collections::VecDeque<(SyncSender<ModelEvent>, ModelEvent)>,
    thread_running: bool,
}

/// Hands `event` to the writer without ever blocking the caller, and in order: events sent
/// through this function reach the channel in the order they were passed in.
///
/// With room in the channel and nothing queued it is a plain `try_send`. Otherwise the event
/// joins a FIFO queue drained by ONE short-lived delivery thread (not a thread per event) that
/// blocks on the channel, so nothing is dropped and nothing is reordered. Delivery counts as a
/// deferred send for [`wait_for_deferred_sends`].
pub fn send_model_event_nonblocking(
    sender: &SyncSender<ModelEvent>,
    event: ModelEvent,
) -> Result<(), std::sync::mpsc::SendError<ModelEvent>> {
    let mut ordered = ORDERED_SENDS.lock().unwrap_or_else(|e| e.into_inner());
    let event = if ordered.thread_running || !ordered.queue.is_empty() {
        event
    } else {
        match try_send_or_defer(sender, event)? {
            None => return Ok(()),
            Some(event) => event,
        }
    };
    log::warn!("SQLite writer channel is full; the model event waits for room in order");
    ordered.queue.push_back((sender.clone(), event));
    if ordered.thread_running {
        return Ok(());
    }
    ordered.thread_running = true;
    note_deferred_send_started();
    let spawned = std::thread::Builder::new()
        .name("SQLite deferred send".to_owned())
        .spawn(move || {
            loop {
                let next = {
                    let mut ordered = ORDERED_SENDS.lock().unwrap_or_else(|e| e.into_inner());
                    let next = ordered.queue.pop_front();
                    if next.is_none() {
                        ordered.thread_running = false;
                    }
                    next
                };
                let Some((sender, event)) = next else { break };
                if let Err(err) = sender.send(event) {
                    log::warn!("Deferred SQLite writer send failed: {err:?}");
                }
            }
            note_deferred_send_finished();
        });
    if spawned.is_err() {
        // No thread to deliver: the queue held only this event (the thread was not running), so
        // fall back to the blocking send rather than lose the write.
        ordered.thread_running = false;
        let next = ordered.queue.pop_front();
        drop(ordered);
        note_deferred_send_finished();
        if let Some((sender, event)) = next {
            return sender.send(event);
        }
    }
    Ok(())
}

impl Entity for PersistenceWriter {
    type Event = ();
}

impl SingletonEntity for PersistenceWriter {}

/// A conversation summary derived at read time by `agent::read_agent_conversation_metadata`
/// for a pre-`summary`-column (or otherwise invalid-summary) row, queued so
/// `agent::backfill_conversation_summaries` can persist it once, keeping subsequent startups
/// metadata-only.
///
/// Ported from the pin (`app/src/persistence/mod.rs:144-155`, `02b53fcd8`) for #431. The pin
/// drains these into a `PersistenceWriter` event (`BackfillConversationSummaries`) handled by the
/// writer thread; this fork instead persists them synchronously in the same read-write connection
/// used by `sqlite::initialize` (see `agent::read_agent_conversation_metadata`'s caller), since
/// this fork's startup read already performs synchronous backfill writes on that connection
/// (previously inline in the eager `read_agent_conversations` this replaces).
#[derive(Debug)]
pub struct ConversationSummaryBackfill {
    pub conversation_id: String,
    /// Serialized [`model::AgentConversationSummary`].
    pub summary_json: String,
    /// The `summary` column value observed at read time (`None` or invalid
    /// JSON). The backfill only applies while the column still holds this
    /// value, so it never overwrites a newer write.
    pub previous_summary: Option<String>,
    /// The row's pre-backfill `last_modified_at`, restored after the
    /// update trigger bumps it.
    pub last_modified_at: chrono::NaiveDateTime,
}

/// TODO: all of this data should eventually be indexed by user_id so that
/// the logged in user sees the data for their user (and if another user logs in,
/// they see their respective data). To do this, we can simply return a mapping
/// of user ID->SqliteData and get the respective AppState after the user logs in.
///
/// For now, to address the global scoping here, we clear all persisted data on logout.
pub struct PersistedData {
    /// Session restoration data
    pub app_state: AppState,

    /// Shareable objects.
    pub cloud_objects: Vec<Box<dyn StoredObject>>,
    pub workspaces: Vec<WorkspaceMetadata>,
    pub current_workspace_uid: Option<WorkspaceUid>,
    pub command_history: Vec<PersistedCommand>,
    pub user_profiles: Vec<UserProfileWithUID>,
    pub time_of_next_force_object_refresh: Option<DateTime<Utc>>,
    pub object_actions: Vec<ObjectAction>,
    pub experiments: Vec<ServerExperiment>,
    pub ai_queries: Vec<PersistedAIInput>,
    /// Prompt text and submission time for NLD prompt-history matching, oldest-first.
    ///
    /// Separate from `ai_queries` even though both come from one read: `ai_queries` is the
    /// up-arrow list, capped at 100 and carrying whole `PersistedAIInput`s, while this is a
    /// much longer text-only candidate list. Empty unless `FeatureFlag::NldPromptHistoryMatch`
    /// is on -- off by default here and at the pin -- so the wider scan costs nothing until the
    /// flag is enabled. Consumed by `BlocklistAIHistoryModel::new` in `app/src/lib.rs`.
    pub nld_prompts: Vec<(String, DateTime<Local>)>,
    /// Recently-used repository roots, restored into `PersistedWorkspace` at
    /// startup. Named `codebase_indices` at the pin, where codebase indexing was
    /// the primary consumer; kept under that name so the diff against
    /// `02b53fcd8` stays readable.
    ///
    pub codebase_indices: Vec<CodeWorkspaceMetadata>,
    /// Per-workspace language-server enablement, restored into
    /// `PersistedWorkspace` alongside `codebase_indices`. Keyed by workspace
    /// root, then by server type.
    pub workspace_language_servers: HashMap<PathBuf, HashMap<LSPServerType, EnablementState>>,
    pub multi_agent_conversations: Vec<AgentConversation>,
    pub projects: Vec<Project>,
    pub project_rules: Vec<ProjectRulePath>,
    pub ignored_suggestions: Vec<(String, SuggestionType)>,
    pub mcp_server_installations: HashMap<Uuid, TemplatableMCPServerInstallation>,
    pub mcp_servers_to_restore: Vec<Uuid>,
    /// Prompts and commands that were still queued when the app last ran, in queue order per
    /// conversation (#1027). Restored by `QueuedQueryModel::restore_persisted`.
    pub queued_prompts: Vec<model::QueuedPromptRow>,
}

/// [`PersistedData`] without `cloud_objects`, so it can move between threads. `cloud_objects` holds
/// `Box<dyn StoredObject>`, which is not `Send`; every other field is checked by the compiler
/// (the `assert_send` below fails to build if one stops being `Send`). Used to hand a startup
/// read made on the prewarm thread to the main thread.
pub(crate) struct SendablePersistedData {
    app_state: AppState,
    workspaces: Vec<WorkspaceMetadata>,
    current_workspace_uid: Option<WorkspaceUid>,
    command_history: Vec<PersistedCommand>,
    user_profiles: Vec<UserProfileWithUID>,
    time_of_next_force_object_refresh: Option<DateTime<Utc>>,
    object_actions: Vec<ObjectAction>,
    experiments: Vec<ServerExperiment>,
    ai_queries: Vec<PersistedAIInput>,
    nld_prompts: Vec<(String, DateTime<Local>)>,
    codebase_indices: Vec<CodeWorkspaceMetadata>,
    workspace_language_servers: HashMap<PathBuf, HashMap<LSPServerType, EnablementState>>,
    multi_agent_conversations: Vec<AgentConversation>,
    projects: Vec<Project>,
    project_rules: Vec<ProjectRulePath>,
    ignored_suggestions: Vec<(String, SuggestionType)>,
    mcp_server_installations: HashMap<Uuid, TemplatableMCPServerInstallation>,
    mcp_servers_to_restore: Vec<Uuid>,
    queued_prompts: Vec<model::QueuedPromptRow>,
}

#[allow(dead_code)]
fn assert_send<T: Send>() {}
#[allow(dead_code)]
fn sendable_persisted_data_is_send() {
    assert_send::<SendablePersistedData>();
}

impl SendablePersistedData {
    /// `None` when `data` holds cloud objects, which cannot be moved. The destructuring names
    /// every field, so adding a field to `PersistedData` fails to build until it is handled here.
    pub(crate) fn from_persisted(data: PersistedData) -> Option<Self> {
        let PersistedData {
            app_state,
            cloud_objects,
            workspaces,
            current_workspace_uid,
            command_history,
            user_profiles,
            time_of_next_force_object_refresh,
            object_actions,
            experiments,
            ai_queries,
            nld_prompts,
            codebase_indices,
            workspace_language_servers,
            multi_agent_conversations,
            projects,
            project_rules,
            ignored_suggestions,
            mcp_server_installations,
            mcp_servers_to_restore,
            queued_prompts,
        } = data;
        if !cloud_objects.is_empty() {
            return None;
        }
        Some(Self {
            app_state,
            workspaces,
            current_workspace_uid,
            command_history,
            user_profiles,
            time_of_next_force_object_refresh,
            object_actions,
            experiments,
            ai_queries,
            nld_prompts,
            codebase_indices,
            workspace_language_servers,
            multi_agent_conversations,
            projects,
            project_rules,
            ignored_suggestions,
            mcp_server_installations,
            mcp_servers_to_restore,
            queued_prompts,
        })
    }

    pub(crate) fn clear_nld_prompts(&mut self) {
        self.nld_prompts.clear();
    }

    pub(crate) fn into_persisted(self) -> PersistedData {
        PersistedData {
            app_state: self.app_state,
            cloud_objects: Vec::new(),
            workspaces: self.workspaces,
            current_workspace_uid: self.current_workspace_uid,
            command_history: self.command_history,
            user_profiles: self.user_profiles,
            time_of_next_force_object_refresh: self.time_of_next_force_object_refresh,
            object_actions: self.object_actions,
            experiments: self.experiments,
            ai_queries: self.ai_queries,
            nld_prompts: self.nld_prompts,
            codebase_indices: self.codebase_indices,
            workspace_language_servers: self.workspace_language_servers,
            multi_agent_conversations: self.multi_agent_conversations,
            projects: self.projects,
            project_rules: self.project_rules,
            ignored_suggestions: self.ignored_suggestions,
            mcp_server_installations: self.mcp_server_installations,
            mcp_servers_to_restore: self.mcp_servers_to_restore,
            queued_prompts: self.queued_prompts,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BlockCompleted {
    pub pane_id: Vec<u8>,
    /// Indicates if the block was created locally (e.g. not in a remote session)
    pub is_local: bool,
    pub block: Arc<SerializedBlock>,
    /// When set, `block` carries no command or output bytes: the SQLite writer fills them in from
    /// this (the slow conversion to text, kept off the UI thread).
    pub deferred_content: Option<DeferredBlockContent>,
}

#[derive(Debug)]
pub struct StartedCommandMetadata {
    pub command: String,
    pub start_ts: Option<DateTime<Local>>,
    pub pwd: Option<String>,
    pub shell: Option<String>,
    pub username: Option<String>,
    pub hostname: Option<String>,
    pub session_id: Option<SessionId>,
    pub git_branch: Option<String>,
    pub cloud_workflow_id: Option<SyncId>,
    pub workflow_command: Option<String>,
    pub is_agent_executed: bool,
}

#[derive(Debug)]
pub struct FinishedCommandMetadata {
    pub exit_code: ExitCode,
    pub start_ts: DateTime<Local>,
    pub completed_ts: DateTime<Local>,
    pub session_id: SessionId,
}

/// How an [`ModelEvent::UpdateMultiAgentConversation`] write treats `agent_tasks` rows
/// that are not in its task snapshot, and whether it may replace the stored `summary`.
///
/// Before this existed, every write had replace semantics: the snapshot was taken as the
/// conversation's complete task set and every other row was deleted — including when the
/// snapshot was EMPTY, which `ne_all(&[])` turns into "delete every row". A conversation
/// restored without its tasks (a synthesized optimistic root, which has no protobuf
/// source and so contributes nothing to the snapshot) therefore wiped its own history on
/// the first follow-up. The pin shares that weakness (`4111d08f9:app/src/persistence/agent.rs`);
/// see `DECLINED.md` → `IMPROVED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PersistedTaskRetention {
    /// Replace semantics for a non-empty snapshot: rows absent from it are deleted (this is
    /// what keeps rewound or pruned subtasks from being resurrected on restore). An EMPTY
    /// snapshot deletes nothing and leaves a stored summary that names a real initial query
    /// in place, because "no tasks" is far more often "tasks not loaded" than "tasks removed".
    #[default]
    DeleteMissing,
    /// Replace semantics even for an empty snapshot. Only for a deliberate user action that
    /// leaves the conversation with no persisted tasks: a rewind that removes every exchange
    /// (`AIConversation::truncate_from_exchange` resetting the root to optimistic).
    DeleteMissingEvenIfEmpty,
}

#[derive(Debug)]
pub enum ModelEvent {
    SaveBlock(BlockCompleted),
    DeleteBlocks(Vec<u8>),
    Snapshot(AppState),
    UpsertWorkflows(Vec<WorkflowObject>),
    UpsertNotebooks(Vec<NotebookObject>),
    UpsertFolders(Vec<FolderObject>),
    IncrementRetryCount(String),
    UpsertGenericStringObject {
        object: Box<dyn StoredStringObject>,
    },
    UpsertGenericStringObjects(Vec<Box<dyn StoredStringObject>>),
    UpsertNotebook {
        notebook: NotebookObject,
    },
    UpsertWorkflow {
        workflow: WorkflowObject,
    },
    UpsertFolder {
        folder: FolderObject,
    },
    DeleteObjects {
        ids: Vec<(SyncId, ObjectIdType)>,
    },
    UpsertWorkspace {
        workspace: Box<WorkspaceMetadata>,
    },
    UpsertWorkspaces {
        workspaces: Vec<WorkspaceMetadata>,
    },
    SetCurrentWorkspace {
        workspace_uid: WorkspaceUid,
    },
    UpdateObjectMetadata {
        id: String,
        metadata: StoredObjectMetadata,
    },
    InsertCommand {
        metadata: StartedCommandMetadata,
    },
    UpdateFinishedCommand {
        metadata: FinishedCommandMetadata,
    },
    UpsertUserProfiles {
        profiles: Vec<UserProfileWithUID>,
    },
    ClearUserProfiles,
    RecordTimeOfNextRefresh {
        timestamp: DateTime<Utc>,
    },
    SaveExperiments {
        experiments: Vec<ServerExperiment>,
    },
    // `PauseAndRemoveDatabase` and `ReconstructAndResume` are used to pause and resume the writer thread.
    // These are employed as part of Logout v0 to ensure that the writer thread
    // does not continue writing to the DB after the user has logged out and the DB is deleted.
    PauseAndRemoveDatabase,
    #[cfg(feature = "local_fs")]
    ReconstructAndResume,
    InsertObjectAction {
        object_action: ObjectAction,
    },
    SyncObjectActions {
        actions_to_sync: Vec<ObjectAction>,
    },
    /// Close the SQLite writer thread when the app is about to quit.
    Terminate,
    /// Answered once every event queued before it has been written (or parked behind the same
    /// lock, in order), so a caller can learn that the database holds what it sent (#1252).
    WriteBarrier(WriteBarrier),
    UpsertAIQuery {
        query: Arc<PersistedAIInput>,
    },
    /// Delete the AI query and related data for a given conversation.
    DeleteAIConversation {
        conversation_id: String,
    },
    UpdateMultiAgentConversation {
        conversation_id: String,
        updated_tasks: Vec<Arc<api::Task>>,
        conversation_data: AgentConversationData,
        /// What the write may delete or replace; see [`PersistedTaskRetention`].
        task_retention: PersistedTaskRetention,
    },
    DeleteMultiAgentConversations {
        conversation_ids: Vec<String>,
    },
    /// Replaces the stored queued-prompt rows of one conversation with its current queue; an
    /// empty `rows` clears them (#1027). The whole queue is rewritten on every change, so
    /// ordering through the writer channel is all that is needed for the last write to win.
    ReplaceQueuedPrompts {
        conversation_id: String,
        rows: Vec<model::QueuedPromptRow>,
    },

    UpsertCurrentUserInformation {
        user_information: PersistedCurrentUserInformation,
    },
    /// Writes one `workspace_metadata` row (insert-or-update on `repo_path`).
    ///
    /// Keeps the pin's `…CodebaseIndexMetadata` name even though this is
    /// recently-navigated-workspace bookkeeping (`ai::workspace::WorkspaceMetadata`:
    /// `path`/`navigated_ts`/`modified_ts`/`queried_ts`), not the embedding-based
    /// codebase index — that index now exists too (`app/src/remote_server/codebase_index_model.rs`,
    /// local/BYOP embeddings via the user's own provider), with its own separate
    /// storage (`save_codebase_index_embeddings`, `save_codebase_index_nodes`).
    /// Renaming this variant would make the diff against `02b53fcd8` harder to
    /// read for no benefit, since the pin names it identically for the same reason.
    UpsertCodebaseIndexMetadata {
        index_metadata: Box<CodeWorkspaceMetadata>,
    },
    DeleteCodebaseIndexMetadata {
        repo_path: PathBuf,
    },
    /// Records intermediate merkle nodes of a codebase index.
    ///
    /// New in this fork. At the pin these went to Warp's server through
    /// `StoreClient::update_intermediate_nodes`; with no server, the local
    /// `StoreClient` writes them here. `embedding_space` is
    /// `EmbeddingConfig::storage_key()` and each entry is
    /// `(node_hash, child_hashes_json)`.
    UpsertCodebaseIndexNodes {
        embedding_space: String,
        nodes: Vec<(String, String)>,
    },
    /// Records embedding vectors for code fragments.
    ///
    /// New in this fork, for the same reason as `UpsertCodebaseIndexNodes`. Each
    /// entry is `(content_hash, dimensions, little_endian_f32_bytes)`. Batches
    /// arrive one embedding request at a time (`embedding_generation_batch_size`,
    /// default 100), not one row at a time, so this does not flood the writer
    /// channel.
    UpsertCodebaseIndexEmbeddings {
        embedding_space: String,
        embeddings: Vec<(String, i32, Vec<u8>)>,
    },
    /// Records the codebase index's per-node search summaries.
    ///
    /// New in this fork. These are what let a query prune a subtree instead of
    /// scoring every fragment beneath it; see
    /// `crates/ai/src/index/full_source_code_embedding/vector_index.rs`. Each
    /// entry is `(node_hash, leaf_count, radius_radians, dimensions,
    /// little_endian_f32_mean)`.
    ///
    /// Written once per index build rather than per sync batch, so this is a
    /// rarer and larger event than `UpsertCodebaseIndexEmbeddings`.
    UpsertCodebaseIndexNodeSummaries {
        embedding_space: String,
        summaries: Vec<(String, i32, f32, i32, Vec<u8>)>,
    },
    /// Writes one `workspace_language_server` row (insert-or-update on the
    /// (workspace, server) pair). The workspace must already have a
    /// `workspace_metadata` row — the FK is enforced.
    UpsertWorkspaceLanguageServer {
        workspace_path: PathBuf,
        lsp_type: LSPServerType,
        enabled: EnablementState,
    },
    UpsertProject {
        project: Project,
    },
    DeleteProject {
        path: String,
    },
    UpsertMCPServerEnvironmentVariables {
        mcp_server_uuid: Vec<u8>,
        environment_variables: String,
    },
    UpsertProjectRules {
        project_rule_paths: Vec<ProjectRulePath>,
    },
    DeleteProjectRules {
        path: Vec<PathBuf>,
    },
    AddIgnoredSuggestion {
        suggestion: String,
        suggestion_type: SuggestionType,
    },
    RemoveIgnoredSuggestion {
        suggestion: String,
        suggestion_type: SuggestionType,
    },
    UpsertMCPServerInstallation {
        mcp_server_installation: TemplatableMCPServerInstallation,
    },
    DeleteMCPServerInstallations {
        installation_uuids: Vec<Uuid>,
    },
    DeleteMCPServerInstallationsByTemplateUuid {
        template_uuid: Uuid,
    },
    UpdateMCPInstallationRunning {
        installation_uuid: Uuid,
        running: bool,
    },
    UpdateBlockAgentViewVisibility {
        block_id: String,
        agent_view_visibility: SerializedAgentViewVisibility,
    },
    SaveAIDocumentContent {
        document_id: String,
        content: String,
        version: i32,
        title: String,
    },
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
