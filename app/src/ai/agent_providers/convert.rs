//! Conversions from application types to MAA API types (still used by BYOP tool plumbing).

use agent_proto as api;
use ai::agent::convert::ConvertToAPITypeError;
use chrono::{DateTime, Local};

use crate::ai::{
    agent::{AIAgentActionResult, AIAgentActionResultType},
    block_context::BlockContext,
};

fn local_datetime_to_timestamp(timestamp: DateTime<Local>) -> prost_types::Timestamp {
    prost_types::Timestamp {
        seconds: timestamp.timestamp(),
        nanos: timestamp.timestamp_subsec_nanos() as i32,
    }
}

impl TryFrom<AIAgentActionResult> for api::request::input::user_inputs::user_input::Input {
    type Error = ConvertToAPITypeError;

    fn try_from(action_result: AIAgentActionResult) -> Result<Self, Self::Error> {
        let result = match action_result.result {
            AIAgentActionResultType::RequestCommandOutput(request_command_result) => {
                Some(request_command_result.try_into()?)
            }
            AIAgentActionResultType::WriteToLongRunningShellCommand(result) => {
                Some(result.try_into()?)
            }
            AIAgentActionResultType::ReadFiles(read_files_result) => {
                Some(read_files_result.try_into()?)
            }
            AIAgentActionResultType::RequestFileEdits(request_file_edits_result) => {
                Some(request_file_edits_result.try_into()?)
            }
            AIAgentActionResultType::Grep(grep_result) => Some(grep_result.try_into()?),
            AIAgentActionResultType::FileGlob(file_glob_result) => {
                Some(file_glob_result.try_into()?)
            }
            AIAgentActionResultType::FileGlobV2(file_glob_result) => {
                Some(file_glob_result.try_into()?)
            }
            AIAgentActionResultType::ReadMCPResource(read_mcp_resource_result) => {
                Some(read_mcp_resource_result.try_into()?)
            }
            AIAgentActionResultType::CallMCPTool(call_mcp_tool_result) => {
                Some(call_mcp_tool_result.try_into()?)
            }
            AIAgentActionResultType::ReadSkill(read_skill_result) => {
                Some(read_skill_result.try_into()?)
            }
            AIAgentActionResultType::SuggestNewConversation(suggest_new_conversation_result) => {
                Some(suggest_new_conversation_result.try_into()?)
            }
            AIAgentActionResultType::SuggestPrompt(suggest_prompt_result) => {
                Some(suggest_prompt_result.try_into()?)
            }
            AIAgentActionResultType::OpenCodeReview => Some(
                agent_proto::request::input::tool_call_result::Result::OpenCodeReview(
                    agent_proto::OpenCodeReviewResult {},
                ),
            ),
            AIAgentActionResultType::InsertReviewComments(insert_review_comments_result) => {
                Some(insert_review_comments_result.try_into()?)
            }
            AIAgentActionResultType::InitProject => Some(
                agent_proto::request::input::tool_call_result::Result::InitProject(
                    agent_proto::InitProjectResult {},
                ),
            ),
            AIAgentActionResultType::ReadDocuments(read_documents_result) => {
                Some(read_documents_result.try_into()?)
            }
            AIAgentActionResultType::EditDocuments(edit_documents_result) => {
                Some(edit_documents_result.try_into()?)
            }
            AIAgentActionResultType::CreateDocuments(create_documents_result) => {
                Some(create_documents_result.try_into()?)
            }
            AIAgentActionResultType::ReadShellCommandOutput(read_shell_command_output_result) => {
                Some(read_shell_command_output_result.try_into()?)
            }
            AIAgentActionResultType::UseComputer(use_computer_result) => {
                Some(use_computer_result.try_into()?)
            }
            AIAgentActionResultType::RequestComputerUse(request_computer_use_result) => {
                Some(request_computer_use_result.try_into()?)
            }
            AIAgentActionResultType::TransferShellCommandControlToUser(transfer_control_result) => {
                Some(transfer_control_result.try_into()?)
            }
            AIAgentActionResultType::AskUserQuestion(ask_user_question_result) => {
                Some(ask_user_question_result.into())
            }
            AIAgentActionResultType::SendMessageToAgent(send_message_result) => {
                Some(send_message_result.into())
            }
        };
        Ok(
            api::request::input::user_inputs::user_input::Input::ToolCallResult(
                api::request::input::ToolCallResult {
                    tool_call_id: action_result.id.into(),
                    result,
                },
            ),
        )
    }
}

impl From<BlockContext> for api::ExecutedShellCommand {
    fn from(block: BlockContext) -> Self {
        api::ExecutedShellCommand {
            command: block.command,
            output: block.output,
            exit_code: block.exit_code.value(),
            command_id: block.id.into(),
            is_auto_attached: block.is_auto_attached,
            started_ts: block.started_ts.map(local_datetime_to_timestamp),
            finished_ts: block.finished_ts.map(local_datetime_to_timestamp),
        }
    }
}
