//! Bidirectional translation registry for OpenAI tool calling in BYOP mode.
//!
//! Each built-in warp tool (a variant of `api::message::tool_call::Tool`) maps to one
//! [`OpenAiTool`] descriptor: function name + JSON Schema + reverse-parsing of args +
//! serializing the execution result into a string the upstream model can read.
//!
//! ## Currently implemented subset (Phase 3a, first batch)
//!
//! - `run_shell_command`
//! - `read_files`
//!
//! Later rounds will add: `grep` / `file_glob_v2` / `apply_file_diffs` / `call_mcp_tool`, etc.
//!
//! ## Closed-loop overview
//!
//! The model returns `tool_calls` → `from_args` converts it into a `tool_call::Tool` → we emit
//! `Message::ToolCall { tool_call_id, tool }` → warp's own `convert_from.rs`
//! automatically converts it into an `AIAgentAction` → the executor runs it through profile
//! permissions/prompts → executes → the result is written back into the conversation
//! automatically → triggering the next byop request round → our `result_to_json`
//! serializes the result into `role=tool, tool_call_id=...` content for the upstream model.

pub mod ask;
pub mod codebase;
pub mod codebase_runtime;
pub mod get_relevant_files;
pub mod get_relevant_files_runtime;
pub mod coerce;
pub mod computer;
pub mod documents;
pub mod edit;
pub mod exa;
pub mod files;
pub mod long_shell;
pub mod markers;
pub mod mcp;
pub mod search;
pub mod shell;
pub mod skill;
pub mod suggest;
pub mod todowrite;
pub mod web_runtime;
pub mod webfetch;
pub mod websearch;

use anyhow::Result;
use serde_json::Value;
use warp_multi_agent_api as api;

use crate::ai::agent::AIAgentActionResult;

/// A bidirectional adapter descriptor for one tool.
///
/// **Naming history**: BYOP originally only spoke the OpenAI-compatible protocol, then
/// switched to the genai SDK across 5 adapters (OpenAI / OpenAIResp / Gemini / Anthropic /
/// Ollama). The struct name kept `OpenAiTool` to preserve git blame, but the JSON Schema it
/// carries is the OpenAPI standard; each adapter is automatically rewritten internally by
/// genai into its own native format (e.g. Anthropic's input_schema, Gemini's
/// function_declarations).
pub struct OpenAiTool {
    /// Function name given to the upstream LLM (the model calls it by this name in its response).
    pub name: &'static str,
    /// Description given to the LLM.
    pub description: &'static str,
    /// Parameter JSON Schema (OpenAPI standard). Returns a closure to avoid constructing a
    /// serde_json::Value in a const.
    pub parameters: fn() -> Value,
    /// Reverse parsing: the args JSON string returned by the upstream model → warp's internal
    /// `tool_call::Tool` variant.
    pub from_args: fn(args: &str) -> Result<api::message::tool_call::Tool>,
    /// Converts the `Result` variant in ToolCallResult that corresponds to this tool into JSON
    /// readable by the upstream model. Returns `None` when there's no matching variant (letting
    /// the caller fall back to generic serialization).
    pub result_to_json: fn(&api::message::tool_call_result::Result) -> Option<Value>,
}

impl OpenAiTool {
    /// Converts to a genai `Tool` (used to feed `ChatRequest.tools`).
    pub fn to_genai_tool(&self) -> genai::chat::Tool {
        genai::chat::Tool::new(self.name)
            .with_description(self.description)
            .with_schema((self.parameters)())
    }
}

/// Registry: all currently supported BYOP tools.
pub const REGISTRY: &[&OpenAiTool] = &[
    &shell::RUN_SHELL_COMMAND,
    &files::READ_FILES,
    &search::GREP,
    &search::FILE_GLOB_V2,
    &edit::APPLY_FILE_DIFFS,
    &long_shell::WRITE_TO_LONG_RUNNING_SHELL_COMMAND,
    &long_shell::READ_SHELL_COMMAND_OUTPUT,
    &ask::ASK_USER_QUESTION,
    &skill::READ_SKILL,
    // Local document system (AIDocumentModel)
    &documents::READ_DOCUMENTS,
    &documents::EDIT_DOCUMENTS,
    &documents::CREATE_DOCUMENTS,
    // User-suggestion tools (local channel + UI)
    &suggest::SUGGEST_NEW_CONVERSATION,
    &suggest::SUGGEST_PROMPT,
    // UI markers (no side effects, just signals to the frontend)
    &markers::OPEN_CODE_REVIEW,
    &markers::TRANSFER_SHELL_CONTROL,
    // Local todo list (BYOP synthesizes Message::UpdateTodos itself; doesn't go through the
    // protobuf executor)
    &todowrite::TODOWRITE,
    // BYOP-only network tools: not mapped to a protobuf executor variant; chat_stream
    // intercepts them by name before parse_incoming_tool_call and calls web_runtime directly
    // to run the HTTP request.
    // Gating: when profile.web_search_enabled=false, build_tools_array filters these out.
    &webfetch::WEBFETCH,
    &websearch::WEBSEARCH,
    // BYOP-only local codebase search: the cloud `Tool::SearchCodebase` proto variant was
    // deleted, so this tool is NOT mapped to a protobuf executor variant either. chat_stream
    // intercepts it by name before parse_incoming_tool_call and answers it from a local
    // RepoOutlines symbol snapshot (see codebase_runtime).
    // Gating: when profile.codebase_context_enabled=false, build_tools_array filters it out.
    &codebase::SEARCH_CODEBASE,
    // BYOP-only local relevance filter: given a query, a cheap on-device one-shot picks the
    // relevant repository files from the local RepoOutlines index. The BYOP realization of
    // Warp's cloud GetRelevantFiles action; not mapped to a protobuf executor variant.
    // chat_stream intercepts it by name and runs get_relevant_files_runtime directly.
    // Gating: shares codebase_context_enabled with search_codebase; filtered out when off.
    &get_relevant_files::GET_RELEVANT_FILES,
    // Computer use: drives the user's real mouse/keyboard through `crates/computer_use`.
    // Unlike every other entry, these two schemas have no pin to mirror — Warp's server owns
    // tool selection and holds the schema server-side, so `computer.rs` authors them from the
    // Rust types. `request_computer_use` must precede `use_computer`: it is where the user
    // approves, and `UseComputerExecutor::should_autoexecute` returns true on the strength of
    // that approval.
    // Gating: `build_tools_array` / `available_tool_names` filter both out unless
    // `RequestParams::computer_use_enabled` is set, which already folds together
    // `FeatureFlag::AgentModeComputerUse`, the profile's computer-use permission,
    // `computer_use::is_supported_on_current_platform()` and
    // `FeatureFlag::LocalComputerUse`/ambient-agent (see `ai/agent/api.rs`). Both are also in
    // `PLAN_MODE_BLOCKED_TOOLS`.
    &computer::REQUEST_COMPUTER_USE,
    &computer::USE_COMPUTER,
];

/// Looks up the registry by OpenAI function name.
pub fn lookup(name: &str) -> Option<&'static OpenAiTool> {
    REGISTRY.iter().copied().find(|t| t.name == name)
}

/// Given a ToolCallResult, first tries to find the matching tool in REGISTRY and serialize
/// with its `result_to_json`; if not found, tries generic MCP serialization; falls back to a
/// short description as a last resort to avoid panicking.
pub fn serialize_result(result: &api::message::ToolCallResult) -> String {
    let inner = match &result.result {
        Some(r) => r,
        None => return r#"{"status":"cancelled"}"#.to_owned(),
    };
    stringify_result_value(&serialize_result_value(inner))
}

/// The structured form of [`serialize_result`], before it is flattened to a string.
///
/// Callers that need to *amend* a result before sending it — `chat_stream` rewriting
/// `screenshot.attached` / `screenshot.note` once it knows whether the image travels with the
/// request, see [`computer::annotate_screenshot_delivery`] — must go through this rather than
/// re-parsing the string, which would silently swallow a serialization change.
pub fn serialize_result_value(inner: &api::message::tool_call_result::Result) -> Value {
    // Fallback: unrecognized variant (tools not yet registered in later user rounds also land
    // here).
    try_serialize_result_value(inner)
        .unwrap_or_else(|| serde_json::json!({ "status": "unsupported_tool_result" }))
}

/// Renders a result value the way the tool-result channel expects it.
///
/// `to_string` on a `Value` built from owned data cannot realistically fail; `{}` is the
/// inert fallback rather than a panic, matching the pre-existing behaviour.
pub fn stringify_result_value(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_owned())
}

/// Shared lookup: the first registry descriptor that claims this result, else MCP.
///
/// `None` means *nothing* recognized the variant — the callers that have a better fallback
/// than `{"status":"unsupported_tool_result"}` (the current-turn `ActionResult` path, which
/// can still render the result's `Display`) depend on being able to tell the two apart.
pub fn try_serialize_result_value(inner: &api::message::tool_call_result::Result) -> Option<Value> {
    for t in REGISTRY {
        if let Some(json) = (t.result_to_json)(inner) {
            return Some(json);
        }
    }
    mcp::serialize_result(inner)
}

/// Serializes an `AIAgentActionResult` that just finished *client-side execution this round*
/// into a JSON string to feed to the upstream model (as `role=tool` content).
///
/// ## Why not just use `AIAgentActionResultType::Display`
///
/// The `Display` impl renders structured results (especially `LongRunningCommandSnapshot`)
/// into one-line strings like `"Command 'bun repl' is long-running"`, **completely discarding
/// critical fields like block_id (=command_id), grid_contents, is_alt_screen_active**, which
/// causes the next round's model to lose access to command_id and be unable to continue
/// read/write_to_long_running_*, completely breaking long-running commands.
///
/// ## How it works
///
/// 1. Reuse the existing `TryFrom<AIAgentActionResult> for
///    api::request::input::user_inputs::user_input::Input` in
///    `app/src/ai/agent/api/convert_to.rs` (covering all 25+ ActionResult variants), to get
///    `Input::ToolCallResult { result, .. }`
/// 2. The inner `*Result` type (e.g. `RunShellCommandResult`) shares the same protobuf message
///    with `api::message::tool_call_result::Result`; only the outer enum's namespace differs,
///    so it can be rewrapped in the outer enum and reuse the existing per-tool
///    `result_to_json` in `tools::REGISTRY` (see `shell.rs::result_to_json`, which flattens
///    `LongRunningCommandSnapshot` into complete JSON including
///    command_id/output/is_alt_screen_active)
/// 3. Unrecognized variants return `None`, and the caller falls back to Display
///
/// ## Maintenance note
///
/// When adding a new BYOP tool, **the enum match here must be updated with the new variant**,
/// otherwise that tool's current-round ActionResult will fall back to Display and lose its
/// structured fields.
pub fn serialize_action_result(action: &AIAgentActionResult) -> Option<String> {
    let msg_side = action_result_to_msg_result(action)?;
    Some(stringify_result_value(&try_serialize_result_value(
        &msg_side,
    )?))
}

/// Converts an `AIAgentActionResult` that finished client-side execution this round into an
/// `api::message::tool_call_result::Result` enum, for BYOP to persist as task.message.
///
/// Shares the ReqR → MsgR mapping with `serialize_action_result`; the caller wraps the result
/// into `Message::ToolCallResult { result: Some(...), context: None, tool_call_id }`.
pub fn action_result_to_msg_result(
    action: &AIAgentActionResult,
) -> Option<api::message::tool_call_result::Result> {
    use api::message::tool_call_result::Result as MsgR;
    use api::request::input::tool_call_result::Result as ReqR;
    use api::request::input::user_inputs::user_input::Input;

    let input: Input = action.clone().try_into().ok()?;
    let req_input: ReqR = match input {
        Input::ToolCallResult(tcr) => tcr.result?,
        _ => return None,
    };
    let msg_side = match req_input {
        ReqR::RunShellCommand(r) => MsgR::RunShellCommand(r),
        ReqR::WriteToLongRunningShellCommand(r) => MsgR::WriteToLongRunningShellCommand(r),
        ReqR::ReadShellCommandOutput(r) => MsgR::ReadShellCommandOutput(r),
        ReqR::ReadFiles(r) => MsgR::ReadFiles(r),
        ReqR::Grep(r) => MsgR::Grep(r),
        ReqR::FileGlobV2(r) => MsgR::FileGlobV2(r),
        ReqR::ApplyFileDiffs(r) => MsgR::ApplyFileDiffs(r),
        ReqR::CallMcpTool(r) => MsgR::CallMcpTool(r),
        ReqR::ReadMcpResource(r) => MsgR::ReadMcpResource(r),
        ReqR::AskUserQuestion(r) => MsgR::AskUserQuestion(r),
        ReqR::ReadSkill(r) => MsgR::ReadSkill(r),
        ReqR::ReadDocuments(r) => MsgR::ReadDocuments(r),
        ReqR::EditDocuments(r) => MsgR::EditDocuments(r),
        ReqR::CreateDocuments(r) => MsgR::CreateDocuments(r),
        ReqR::SuggestNewConversation(r) => MsgR::SuggestNewConversation(r),
        ReqR::SuggestPrompt(r) => MsgR::SuggestPrompt(r),
        ReqR::OpenCodeReview(r) => MsgR::OpenCodeReview(r),
        ReqR::TransferShellCommandControlToUser(r) => MsgR::TransferShellCommandControlToUser(r),
        // Note the asymmetric field names: the request-side oneof field is
        // `request_computer_use`, the message-side one is `request_computer_use_result`, so
        // the generated variant names differ on the two sides of this mapping.
        ReqR::UseComputer(r) => MsgR::UseComputer(r),
        ReqR::RequestComputerUse(r) => MsgR::RequestComputerUseResult(r),
        _ => return None,
    };
    Some(msg_side)
}
