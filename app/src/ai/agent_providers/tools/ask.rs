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
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};
use uuid::Uuid;
use warp_multi_agent_api as api;

use super::OpenAiTool;

/// "No option is recommended". `convert_from::convert_api_question` filters any index
/// outside the options range back to `None`, so this reaches the UI as nothing being
/// pre-recommended rather than as a highlighted first option.
const NO_RECOMMENDATION: i32 = -1;

#[derive(Debug, Deserialize)]
struct Args {
    questions: Vec<QuestionArg>,
}

#[derive(Debug)]
struct QuestionArg {
    question: String,
    options: Vec<String>,
    /// 0-based index of the recommended option, or [`NO_RECOMMENDATION`].
    recommended_index: i32,
    /// Whether multiple selection is allowed.
    multi_select: bool,
    /// Whether the user may type free-form "Other" text.
    supports_other: bool,
}

/// Options synthesized when the model asks a question but offers none. A question with zero
/// options renders as a prompt the user cannot answer, so a yes/no pair is strictly better
/// than dropping the question.
fn yes_no_options() -> Vec<String> {
    vec![crate::t!("common-yes"), crate::t!("common-no")]
}

/// Tolerant parse of one question, in the spirit of [`super::coerce`] and of
/// `apply_file_diffs`' optional `summary` (see `edit.rs`): the advertised schema in
/// [`parameters`] stays strict so well-behaved models keep sending the full object, but a
/// model that simplifies the shape does not lose the whole call in `serde_json::from_str`
/// before the question ever reaches the user.
///
/// Two simplifications are accepted, both observed from `gpt-oss:20b`, which retried eight
/// times and only ever reworded the question rather than restructuring it:
///
/// - `"questions": ["Delete the branches?"]` — the array item *is* the question text. This is
///   the same "bare string or full object" tolerance that
///   [`crate::settings::AgentProviderModel`] already applies to `models = [...]`.
/// - an object whose `options` are absent or empty.
///
/// Both survive with synthesized yes/no options and **no** recommended answer.
/// Deliberately not index 0: a question whose options we had to invent is exactly the kind
/// that turns out to be destructive ("delete all local branches except 'main'?"), and
/// pre-recommending an answer we are guessing at is worse than recommending nothing.
impl<'de> Deserialize<'de> for QuestionArg {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            /// The advertised shape. `question` stays required, so a malformed object is
            /// still rejected rather than silently becoming a question with no text.
            Full {
                /// `text` is an observed synonym, not a guess — one logged payload was
                /// `{"questions":[{"required":true,"text":"Would you like me to …"}]}`,
                /// which failed with `missing field 'question'`. Same treatment as
                /// `apply_file_diffs`' `path`/`contents` aliases; see `edit.rs`.
                #[serde(alias = "text")]
                question: String,
                #[serde(default)]
                options: Vec<String>,
                #[serde(default)]
                recommended_index: Option<i32>,
                #[serde(default)]
                multi_select: bool,
                #[serde(default)]
                supports_other: bool,
            },
            /// Just the question text.
            Bare(String),
        }

        Ok(match Either::deserialize(deserializer)? {
            Either::Full {
                question,
                options,
                recommended_index,
                multi_select,
                supports_other,
            } if !options.is_empty() => QuestionArg {
                question,
                options,
                // The model supplied real options, so the schema's documented default
                // applies: index 0 when it did not nominate one.
                recommended_index: recommended_index.unwrap_or(0),
                multi_select,
                supports_other,
            },
            Either::Full {
                question,
                multi_select,
                supports_other,
                ..
            } => QuestionArg {
                question,
                options: yes_no_options(),
                recommended_index: NO_RECOMMENDATION,
                multi_select,
                supports_other,
            },
            Either::Bare(question) => QuestionArg {
                question,
                options: yes_no_options(),
                recommended_index: NO_RECOMMENDATION,
                multi_select: false,
                supports_other: false,
            },
        })
    }
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

#[cfg(test)]
#[path = "ask_tests.rs"]
mod tests;
