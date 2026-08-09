use std::{collections::HashMap, path::PathBuf, sync::Arc};

use chrono::Local;
use lazy_static::lazy_static;
use regex::Regex;
use warp_core::features::FeatureFlag;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::{AppContext, SingletonEntity};

use crate::{
    ai::{
        agent::{
            conversation::AIConversationId, AIAgentAttachment, AIAgentContext,
            DocumentContentAttachmentSource, DriveObjectPayload,
        },
        block_context::BlockContext,
        blocklist::{BlocklistAIContextModel, PendingFile},
        document::ai_document_model::{AIDocumentId, AIDocumentModel},
        facts::AIFactObjectModel,
        skills::list_skills,
    },
    cloud_object::{
        model::{
            generic_string_model::{GenericStringObjectId, StoredStringObject},
            persistence::ObjectStoreModel,
        },
        GenericStoredObject, GenericStringObjectFormat, JsonObjectType, ObjectType,
    },
    terminal::{
        model::{block::BlockId, session::active_session::ActiveSession},
        TerminalView,
    },
};
lazy_static! {
    // Regex to match <block:[block_id]> patterns
    pub static ref BLOCK_CONTEXT_ATTACHMENT_REGEX: Regex = Regex::new(r"<block:([^>]+)>")
        .expect("Block context attachment regex should be parsed");
    // Regex to match zap drive objects inserted via at-context. Ex: <notebook:[workflow_id]>
    pub static ref DRIVE_OBJECT_ATTACHMENT_REGEX: Regex = Regex::new(r"<(workflow|notebook|plan|rule):([^>]+)>")
        .expect("Drive object attachment regex should be parsed");
    // Regex to match <change:filename:line_start-line_end> patterns
    pub static ref DIFF_HUNK_ATTACHMENT_REGEX: Regex = Regex::new(r"<change:([^>]+)>")
        .expect("Diff hunk attachment regex should be parsed");
}

// Returns the context to be attached to the AIAgentInput sent in a request.
// If `is_user_query` is true, includes selected blocks, text, and images from the context model.
// Always includes base context like current time, execution environment, and codebase info.
pub(super) fn input_context_for_request(
    is_user_query: bool,
    context_model: &BlocklistAIContextModel,
    active_session: &ActiveSession,
    // Kept as a parameter: several upstream callers' signatures aren't being
    // touched for now. env stability is now handled by the `<environment_context>`
    // block at the end of the message, so caching per-conversation is no longer
    // needed here.
    _conversation_id: Option<AIConversationId>,
    additional_context: Vec<AIAgentContext>,
    app: &AppContext,
) -> Arc<[AIAgentContext]> {
    let mut context = context_model.pending_context(app, is_user_query);

    context.push(AIAgentContext::CurrentTime {
        current_time: Local::now(),
    });

    // cwd / execution environment always use the **current** resolved result — no
    // freezing, no per-session fallback.
    //
    // This used to freeze pwd per conversation: once a valid value was seen, it
    // locked in and got reused for the whole session. The goal was to keep the
    // <env> section of the system prompt stable (if message[0] changes, the FLM's
    // matched count drops to 0 and it does a full re-prefill). The cost was that
    // after a `cd`, the model was told the wrong directory and had to run `pwd`
    // itself to figure out where it was — trading correctness for cache hits, which
    // wasn't worth it.
    //
    // Now cwd / git / date have moved out of the system prompt into the
    // `<environment_context>` block at the end of the message list (see
    // `user_context::render_environment_context`), so the system prompt no longer
    // contains any environment field that changes, meaning there's no longer any
    // reason to freeze it: the system section stays byte-for-byte constant, while
    // the environment state faithfully reflects the present on every round.
    if let Some(env) = active_session.ai_execution_environment(app) {
        context.push(AIAgentContext::ExecutionEnvironment(env));
    }

    if FeatureFlag::ListSkills.is_enabled() {
        // Now that the project has moved off the cloud, the system prompt is fully
        // re-rendered client-side every round (BYOP is stateless), so skills must be
        // sent in full every round rather than as a diff. Also doesn't push when the
        // list is empty, keeping context compact (the template's `{% if skills %}`
        // guard can then omit the section normally).
        let skills = list_skills(
            active_session
                .current_working_directory()
                .map(|cwd| LocalOrRemotePath::Local(PathBuf::from(cwd)))
                .as_ref(),
            app,
        );
        if !skills.is_empty() {
            context.push(AIAgentContext::Skills { skills });
        }
    }

    context.extend(additional_context);

    context.into()
}

/// Parses context reference strings like <block:123> from the user query and returns
/// a map of reference strings to AIAgentAttachment objects.
///
/// This searches across ALL TerminalModels, not just the active session, to find
/// the requested blocks.
pub(super) fn parse_context_attachments(
    query: &str,
    context_model: &BlocklistAIContextModel,
    ctx: &AppContext,
) -> HashMap<String, AIAgentAttachment> {
    let mut referenced_attachments = HashMap::new();

    // Parse block attachments
    for capture in BLOCK_CONTEXT_ATTACHMENT_REGEX.captures_iter(query) {
        if let (Some(full_match), Some(block_id_match)) = (capture.get(0), capture.get(1)) {
            let reference_string = full_match.as_str().to_string();
            let block_id_str = block_id_match.as_str();

            let block_id = BlockId::from(block_id_str.to_string());

            // Search across ALL TerminalModels to find the block
            if let Some(attachment) = find_block_attachment_in_all_terminals(&block_id, ctx) {
                referenced_attachments.insert(reference_string, attachment);
            }
        }
    }

    // Parse drive object attachments (notebooks, workflows, etc)
    for capture in DRIVE_OBJECT_ATTACHMENT_REGEX.captures_iter(query) {
        if let (Some(full_match), Some(object_type_match), Some(object_id_match)) =
            (capture.get(0), capture.get(1), capture.get(2))
        {
            let reference_string = full_match.as_str().to_string();
            let object_type_str = object_type_match.as_str();
            let id_str = object_id_match.as_str();

            if object_type_str == "plan" {
                if let Some(attachment) = plan_attachment_for_reference(id_str, ctx) {
                    referenced_attachments.insert(reference_string, attachment);
                }
            } else {
                let object_type = match object_type_str {
                    "workflow" => ObjectType::Workflow,
                    "notebook" => ObjectType::Notebook,
                    "rule" => ObjectType::GenericStringObject(GenericStringObjectFormat::Json(
                        JsonObjectType::AIFact,
                    )),
                    _ => continue, // Skip unknown object types
                };

                let attachment = drive_object_attachment_for_reference(id_str, object_type, ctx);
                referenced_attachments.insert(reference_string, attachment);
            }
        }
    }

    // Parse diff hunk attachments
    for capture in DIFF_HUNK_ATTACHMENT_REGEX.captures_iter(query) {
        if let (Some(full_match), Some(diff_hunk_match)) = (capture.get(0), capture.get(1)) {
            let reference_string = full_match.as_str().to_string();
            let diff_hunk_key = diff_hunk_match.as_str();

            // Check if we have a stored diff hunk attachment for this key
            if let Some(attachment) = context_model.get_diff_hunk_attachment(diff_hunk_key) {
                referenced_attachments.insert(reference_string, attachment.clone());
            }
        }
    }

    referenced_attachments.extend(context_model.referenced_at_context_attachments(query));

    // Pending file attachments are *not* added here: unlike the reference kinds above (which
    // are all keyed off of what's literally written in `query`), FilePathReference entries
    // depend on which attachment set the caller resolved for this request (a fired queued row's
    // captured files, or live staging), not on "whatever files happen to be staged in this
    // model". Callers add them via `add_pending_file_attachments` after resolving that set.

    // Add pending AI document as attachment if present
    if let Some(document_id) = context_model.pending_document_id() {
        if let Some(content) = AIDocumentModel::as_ref(ctx).get_document_content(&document_id, ctx)
        {
            let document_id_str = document_id.to_string();
            let attachment = AIAgentAttachment::DocumentContent {
                document_id: document_id_str.clone(),
                content,
                source: DocumentContentAttachmentSource::PlanEdited,
                line_range: None,
            };
            // Use the document ID as the reference key
            referenced_attachments.insert(document_id_str, attachment);
        }
    }

    referenced_attachments
}

/// Adds `file_attachments` to `referenced_attachments` as `FilePathReference` entries.
/// Duplicate basenames get a (1), (2), ... suffix to avoid collisions, matching the legacy
/// attachment-key pattern.
///
/// Split out from `parse_context_attachments` so callers can supply their own resolved
/// attachment set (a fired queued row's captured files, or live staging) instead of the model's
/// currently-staged files.
pub(super) fn add_pending_file_attachments(
    referenced_attachments: &mut HashMap<String, AIAgentAttachment>,
    file_attachments: Vec<PendingFile>,
) {
    for file in file_attachments {
        let attachment = AIAgentAttachment::FilePathReference {
            file_id: uuid::Uuid::new_v4().to_string(),
            file_name: file.file_name.clone(),
            file_path: file.file_path.to_string_lossy().to_string(),
        };
        let mut key = file.file_name.clone();
        if referenced_attachments.contains_key(&key) {
            let mut suffix = 1;
            loop {
                key = format!("{} ({suffix})", file.file_name);
                if !referenced_attachments.contains_key(&key) {
                    break;
                }
                suffix += 1;
            }
        }
        referenced_attachments.insert(key, attachment);
    }
}

/// Searches for a block across all terminal models in the application.
/// Returns an AIAgentAttachment if the block is found.
fn find_block_attachment_in_all_terminals(
    block_id: &BlockId,
    ctx: &AppContext,
) -> Option<AIAgentAttachment> {
    // Iterate over all window IDs to search across all terminal views
    for window_id in ctx.window_ids() {
        // Try to get all terminal views for this window
        if let Some(terminal_views) = ctx.views_of_type::<TerminalView>(window_id) {
            for terminal_view_handle in terminal_views {
                let terminal_view = terminal_view_handle.as_ref(ctx);
                let terminal_model = terminal_view.model.lock();
                let block_list = terminal_model.block_list();

                if let Some(block) = block_list.block_with_id(block_id) {
                    // Create an AIAgentAttachment for the block
                    return Some(AIAgentAttachment::Block(BlockContext {
                        id: block.id().clone(),
                        index: block.index(),
                        command: block.command_to_string(),
                        output: block.output_to_string(),
                        exit_code: block.exit_code(),
                        is_auto_attached: false,
                        started_ts: block.start_ts().cloned(),
                        finished_ts: block.completed_ts().cloned(),
                        pwd: None,
                        shell: None,
                        username: None,
                        hostname: None,
                        git_branch: None,
                        os: None,
                        session_id: None,
                    }));
                }
            }
        }
    }

    None
}

pub(crate) fn drive_object_attachment_for_reference(
    uid: &str,
    object_type: ObjectType,
    ctx: &AppContext,
) -> AIAgentAttachment {
    AIAgentAttachment::DriveObject {
        uid: uid.to_string(),
        payload: get_object_attachment_payload(uid, object_type, ctx),
    }
}

pub(crate) fn plan_attachment_for_reference(
    ai_document_uid: &str,
    ctx: &AppContext,
) -> Option<AIAgentAttachment> {
    let ai_doc_id = match AIDocumentId::try_from(ai_document_uid) {
        Ok(id) => id,
        Err(_) => {
            log::warn!("Invalid ai_document_id in plan reference: {ai_document_uid}");
            return None;
        }
    };

    let content = AIDocumentModel::as_ref(ctx)
        .get_document_content(&ai_doc_id, ctx)
        .or_else(|| {
            ObjectStoreModel::as_ref(ctx)
                .get_all_active_notebooks()
                .find(|nb| nb.model().ai_document_id.as_ref() == Some(&ai_doc_id))
                .map(|nb| nb.model().data.clone())
        });

    if let Some(content) = content {
        return Some(AIAgentAttachment::DocumentContent {
            document_id: ai_document_uid.to_string(),
            content,
            source: DocumentContentAttachmentSource::UserAttached,
            line_range: None,
        });
    }

    log::warn!("Plan not found for ai_document_id: {ai_doc_id}");
    None
}

/// Fetches an object's payload from ObjectStoreModel by UID and type.
/// Returns None if the object isn't found.
fn get_object_attachment_payload(
    uid: &str,
    object_type: ObjectType,
    ctx: &AppContext,
) -> Option<DriveObjectPayload> {
    match object_type {
        ObjectType::Workflow => {
            ObjectStoreModel::as_ref(ctx)
                .get_workflow_by_uid(uid)
                .map(|workflow| {
                    let workflow_data = &workflow.model().data;
                    DriveObjectPayload::Workflow {
                        name: workflow_data.name().to_string(),
                        description: workflow_data.description().cloned().unwrap_or_default(),
                        command: workflow_data.content().to_string(),
                    }
                })
        }
        ObjectType::Notebook => {
            ObjectStoreModel::as_ref(ctx)
                .get_notebook_by_uid(uid)
                .map(|notebook| DriveObjectPayload::Notebook {
                    title: notebook.model().title.clone(),
                    content: notebook.model().data.clone(),
                })
        }
        ObjectType::GenericStringObject(_) => {
            // For generic string objects, we only support AI facts (rules) for now
            ObjectStoreModel::as_ref(ctx)
                .get_by_uid(&uid.to_string())
                .and_then(|object| {
                    if let Some(ai_fact) = object.as_any().downcast_ref::<GenericStoredObject<GenericStringObjectId, AIFactObjectModel>>() {
                        let string_object = ai_fact as &dyn StoredStringObject;
                        let object_type =
                            generic_string_object_format_name(string_object.generic_string_object_format());
                        Some(DriveObjectPayload::GenericStringObject {
                            payload: string_object.serialized().model_as_str().to_string(),
                            object_type,
                        })
                    } else {
                        None
                    }
                })
        }
        _ => None, // Other object types not supported for drive object attachments
    }
}

fn generic_string_object_format_name(format: GenericStringObjectFormat) -> String {
    match format {
        GenericStringObjectFormat::Json(JsonObjectType::Preference) => "JsonPreference",
        GenericStringObjectFormat::Json(JsonObjectType::EnvVarCollection) => "JsonEnvVarCollection",
        GenericStringObjectFormat::Json(JsonObjectType::WorkflowEnum) => "JsonWorkflowEnum",
        GenericStringObjectFormat::Json(JsonObjectType::AIFact) => "JsonAIFact",
        GenericStringObjectFormat::Json(JsonObjectType::MCPServer) => "JsonMCPServer",
        GenericStringObjectFormat::Json(JsonObjectType::AIExecutionProfile) => {
            "JsonAIExecutionProfile"
        }
        GenericStringObjectFormat::Json(JsonObjectType::TemplatableMCPServer) => {
            "JsonTemplatableMCPServer"
        }
    }
    .to_string()
}
