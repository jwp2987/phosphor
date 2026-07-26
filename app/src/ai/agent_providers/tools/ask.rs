//! `ask_user_question`: lets the model proactively ask the user a question when a
//! key piece of information is missing (single-select / multi-select / free-form).
//!
//! warp's own version is `AskUserQuestion`, which internally uses a single
//! `MultipleChoice` Question type (whether multiselect is allowed / whether free-form
//! "Other" text is allowed is decided by internal bools).
//!
//! ## Usage guidance (written into the description so the model sees it)
//!
//! Don't use this tool to ask trivial questions like "should I continue?" /
//! "are you sure?" — just follow the default response strategy.
//! Use it only when the user's instruction admits multiple reasonable
//! interpretations and picking the wrong one is costly.

use anyhow::Result;
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;
use warp_multi_agent_api as api;

use super::OpenAiTool;

#[derive(Debug, Deserialize)]
struct Args {
    questions: Vec<QuestionArg>,
}

#[derive(Debug, Deserialize)]
struct QuestionArg {
    question: String,
    options: Vec<String>,
    /// 0-based index of the recommended option. Defaults to 0.
    #[serde(default)]
    recommended_index: i32,
    /// Whether multiple selection is allowed.
    #[serde(default)]
    multi_select: bool,
    /// Whether the user may type free-form "Other" text.
    #[serde(default)]
    supports_other: bool,
}

fn parameters() -> Value {
    json!({
        "type": "object",
        "properties": {
            "questions": {
                "type": "array",
                "description": "Questions to ask the user (usually 1 is enough; only send several when genuinely distinct dimensions need clarification).",
                "items": {
                    "type": "object",
                    "properties": {
                        "question": {
                            "type": "string",
                            "description": "The question text, short and specific, written in the same language as the user's messages."
                        },
                        "options": {
                            "type": "array",
                            "items": {"type": "string"},
                            "minItems": 2,
                            "maxItems": 4,
                            "description": "Option labels (2-4); each should concretely describe the consequence of choosing it."
                        },
                        "recommended_index": {
                            "type": "integer",
                            "description": "0-based index of the recommended option.",
                            "default": 0
                        },
                        "multi_select": {
                            "type": "boolean",
                            "description": "Whether the user may select multiple options.",
                            "default": false
                        },
                        "supports_other": {
                            "type": "boolean",
                            "description": "Whether the user may type free-form \"Other\" text.",
                            "default": false
                        }
                    },
                    "required": ["question", "options"]
                }
            }
        },
        "required": ["questions"],
        "additionalProperties": false
    })
}

fn from_args(args: &str) -> Result<api::message::tool_call::Tool> {
    let parsed: Args = serde_json::from_str(args)?;
    use api::ask_user_question::question::QuestionType;
    use api::ask_user_question::{MultipleChoice, Option as PbOption, Question};

    let questions: Vec<Question> = parsed
        .questions
        .into_iter()
        .map(|q| {
            let options: Vec<PbOption> = q
                .options
                .into_iter()
                .map(|label| PbOption { label })
                .collect();
            Question {
                question_id: Uuid::new_v4().to_string(),
                question: q.question,
                question_type: Some(QuestionType::MultipleChoice(MultipleChoice {
                    options,
                    recommended_option_index: q.recommended_index,
                    is_multiselect: q.multi_select,
                    supports_other: q.supports_other,
                })),
            }
        })
        .collect();

    Ok(api::message::tool_call::Tool::AskUserQuestion(
        api::AskUserQuestion { questions },
    ))
}

fn result_to_json(result: &api::message::tool_call_result::Result) -> Option<Value> {
    use api::ask_user_question_result::answer_item::Answer as A;
    use api::ask_user_question_result::Result as AR;
    use api::message::tool_call_result::Result as R;
    let r = match result {
        R::AskUserQuestion(r) => r,
        _ => return None,
    };
    let value = match &r.result {
        Some(AR::Success(s)) => {
            let answers: Vec<Value> = s
                .answers
                .iter()
                .map(|item| match &item.answer {
                    Some(A::MultipleChoice(mc)) => json!({
                        "question_id": item.question_id,
                        "selected": mc.selected_options,
                        "other_text": if mc.other_text.is_empty() {
                            Value::Null
                        } else {
                            Value::String(mc.other_text.clone())
                        },
                    }),
                    Some(A::Skipped(_)) => json!({
                        "question_id": item.question_id,
                        "skipped": true,
                    }),
                    None => json!({ "question_id": item.question_id, "no_answer": true }),
                })
                .collect();
            json!({ "status": "ok", "answers": answers })
        }
        Some(AR::Error(e)) => json!({ "status": "error", "message": e.message }),
        None => json!({ "status": "cancelled" }),
    };
    Some(value)
}

pub static ASK_USER_QUESTION: OpenAiTool = OpenAiTool {
    name: "ask_user_question",
    description: include_str!("../prompts/tool_descriptions/ask_user_question.md"),
    parameters,
    from_args,
    result_to_json,
};
