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
use crate::terminal::model::block::{SerializedAgentViewVisibility, SerializedBlock};
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
/// Read paths for the codebase embedding index. New in this fork — at the pin
/// this data was on the server, so nothing local read it.
#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::{codebase_index_children, codebase_index_vectors, known_codebase_index_hashes};
/// Direct write paths for the codebase embedding index, plus the standalone
/// connection that owns them. Used only by the remote-server daemon, which has
/// no `PersistenceWriter` to route writes through — see
/// [`sqlite::establish_codebase_index_connection`].
#[cfg(any(feature = "local_fs", feature = "integration_tests"))]
pub use sqlite::{
    establish_codebase_index_connection, save_codebase_index_embeddings, save_codebase_index_nodes,
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
}

#[derive(Clone, Debug)]
pub struct BlockCompleted {
    pub pane_id: Vec<u8>,
    /// Indicates if the block was created locally (e.g. not in a remote session)
    pub is_local: bool,
    pub block: Arc<SerializedBlock>,
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
    UpsertAIQuery {
        query: Arc<PersistedAIInput>,
    },
    /// Delete the AI query and related data for a given conversation.
    DeleteAIConversation {
        conversation_id: String,
    },
    UpdateMultiAgentConversation {
        conversation_id: String,
        updated_tasks: Vec<api::Task>,
        conversation_data: AgentConversationData,
    },
    DeleteMultiAgentConversations {
        conversation_ids: Vec<String>,
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
