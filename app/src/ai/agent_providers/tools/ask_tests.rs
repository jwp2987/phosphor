use warp_multi_agent_api as api;

use super::*;

/// `t!` returns the raw fluent key when i18n has not been initialised, and nextest gives
/// each test its own process — so every test asserting on user-visible text initialises it
/// first. Same reason the six existing files listed in `HANDOFF.md` do.
fn init_i18n() {
    crate::i18n::init(Some("en"));
}

fn questions(args: &str) -> Vec<api::ask_user_question::Question> {
    match from_args(args).expect("from_args should accept these arguments") {
        api::message::tool_call::Tool::AskUserQuestion(ask) => ask.questions,
        _ => panic!("expected an AskUserQuestion tool call"),
    }
}

fn multiple_choice(
    question: &api::ask_user_question::Question,
) -> &api::ask_user_question::MultipleChoice {
    match question.question_type.as_ref() {
        Some(api::ask_user_question::question::QuestionType::MultipleChoice(mc)) => mc,
        _ => panic!("expected a MultipleChoice question"),
    }
}

fn labels(question: &api::ask_user_question::Question) -> Vec<String> {
    multiple_choice(question)
        .options
        .iter()
        .map(|option| option.label.clone())
        .collect()
}

/// The exact payload `gpt-oss:20b` sent, from `zap.log`:
///
/// ```text
/// [byop] from_args failed: tool=ask_user_question
///   err=invalid type: string "Do you want me to delete all local branches except 'main'?",
///       expected struct QuestionArg at line 1 column 74
/// ```
///
/// It killed the whole tool call, so the question never reached the user and the model
/// retried eight times without ever changing the shape.
#[test]
fn bare_string_question_is_accepted() {
    init_i18n();
    let questions = questions(
        r#"{"questions":["Do you want me to delete all local branches except 'main'?"]}"#,
    );

    assert_eq!(questions.len(), 1);
    assert_eq!(
        questions[0].question,
        "Do you want me to delete all local branches except 'main'?"
    );
    assert_eq!(labels(&questions[0]), vec!["Yes", "No"]);
}

/// A question whose options we had to invent must not pre-recommend one of them — the
/// payload that motivated this parse was a request to delete branches.
#[test]
fn synthesized_options_recommend_nothing() {
    init_i18n();
    let questions = questions(r#"{"questions":["Delete all local branches?"]}"#);

    assert_eq!(
        multiple_choice(&questions[0]).recommended_option_index,
        NO_RECOMMENDATION
    );
}

#[test]
fn full_object_form_is_unchanged() {
    init_i18n();
    let questions = questions(
        r#"{"questions":[{"question":"Which target?","options":["Debug","Release"],
             "recommended_index":1,"multi_select":true,"supports_other":true}]}"#,
    );

    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].question, "Which target?");
    assert_eq!(labels(&questions[0]), vec!["Debug", "Release"]);
    let mc = multiple_choice(&questions[0]);
    assert_eq!(mc.recommended_option_index, 1);
    assert!(mc.is_multiselect);
    assert!(mc.supports_other);
}

/// The schema documents `recommended_index` as defaulting to 0, and a model that supplied
/// real options gets that default — the `NO_RECOMMENDATION` sentinel is only for options we
/// synthesized ourselves.
#[test]
fn supplied_options_without_recommendation_default_to_the_first() {
    init_i18n();
    let questions =
        questions(r#"{"questions":[{"question":"Which target?","options":["Debug","Release"]}]}"#);

    assert_eq!(multiple_choice(&questions[0]).recommended_option_index, 0);
}

#[test]
fn object_without_options_falls_back_to_yes_no() {
    init_i18n();
    let questions = questions(r#"{"questions":[{"question":"Proceed?"}]}"#);

    assert_eq!(questions[0].question, "Proceed?");
    assert_eq!(labels(&questions[0]), vec!["Yes", "No"]);
    assert_eq!(
        multiple_choice(&questions[0]).recommended_option_index,
        NO_RECOMMENDATION
    );
}

#[test]
fn object_with_empty_options_falls_back_to_yes_no() {
    init_i18n();
    let questions = questions(r#"{"questions":[{"question":"Proceed?","options":[]}]}"#);

    assert_eq!(labels(&questions[0]), vec!["Yes", "No"]);
}

/// A model that gets the shape right for one question and wrong for the next keeps both.
#[test]
fn bare_and_full_questions_mix_in_one_call() {
    init_i18n();
    let questions = questions(
        r#"{"questions":["Delete the branches?",
             {"question":"Which target?","options":["Debug","Release"]}]}"#,
    );

    assert_eq!(questions.len(), 2);
    assert_eq!(labels(&questions[0]), vec!["Yes", "No"]);
    assert_eq!(labels(&questions[1]), vec!["Debug", "Release"]);
}

/// Every question gets its own id — the answer is routed back by `question_id`, so two
/// questions sharing one would misattribute the user's answer.
#[test]
fn each_question_gets_a_distinct_id() {
    init_i18n();
    let questions = questions(r#"{"questions":["First?","Second?"]}"#);

    assert_eq!(questions.len(), 2);
    assert!(!questions[0].question_id.is_empty());
    assert_ne!(questions[0].question_id, questions[1].question_id);
}

/// The tolerance above is for *shape*, not for missing content: an object with no question
/// text still fails, rather than reaching the user as an unanswerable blank prompt.
#[test]
fn object_without_question_text_is_still_rejected() {
    init_i18n();
    assert!(from_args(r#"{"questions":[{"options":["A","B"]}]}"#).is_err());
}

#[test]
fn non_object_non_string_question_is_still_rejected() {
    init_i18n();
    assert!(from_args(r#"{"questions":[42]}"#).is_err());
}

/// Observed payload: the model named the field `text` and the call died with
/// `missing field 'question'`. The shape tolerance above does not cover this — the item IS
/// an object, just with a synonym — so it needs the `#[serde(alias)]`.
#[test]
fn question_accepts_the_text_synonym() {
    init_i18n();
    let questions =
        questions(r#"{"questions":[{"required":true,"text":"Would you like me to proceed?"}]}"#);

    assert_eq!(questions.len(), 1);
    assert_eq!(questions[0].question, "Would you like me to proceed?");
    assert_eq!(labels(&questions[0]), vec!["Yes", "No"]);
}
