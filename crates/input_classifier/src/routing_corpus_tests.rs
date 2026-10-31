//! Runs the consolidated routing corpus (`routing_corpus.rs`, #1234) against the decision
//! procedure.
//!
//! Stage 1 replays the pre-#1234 procedure exactly as the app runs it, from the public rules of
//! this crate and the order `BlocklistAIInputModel::detect_and_set_input_type` /
//! `settle_pending_detection_for_submit` apply them. The model's own vote is the one thing the
//! replay cannot know, so every case is routed for both votes and for both timings:
//!
//! - the keystroke path, where the parse has landed and the classifier answers (synchronous rules,
//!   then the deterministic prelude every shipped classifier runs, then the vote, then the safety
//!   gate), with the model voting Shell and voting AI;
//! - the Enter path when Enter beat the parse (an empty token list, the inherited input type of the
//!   previous buffer): the Enter-time rules, then the bounded verdict wait for text with prose
//!   evidence (which resolves to the keystroke verdict), otherwise the inherited type.
//!
//! A case passes when every outcome is the expected route (`Expect::Model` accepts anything). A
//! case with `v1_xfail` must fail here: an unexpected pass means a rule changed and the corpus
//! entry must be revisited.

use async_trait::async_trait;
use warp_completer::completer::{Description, SuggestionType, TopLevelCommandCaseSensitivity};
use warp_completer::meta::SpannedItem;
use warp_completer::{ParsedTokenData, ParsedTokensSnapshot};

use crate::routing_corpus::{Case, Expect, Head, Surface, Typeahead, all_cases};
use crate::util::{
    has_no_prose_evidence, is_agent_follow_up_input, is_likely_shell_command,
    is_one_off_natural_language_word, is_one_off_shell_command_keyword,
    is_unambiguous_shell_command, lacks_command_evidence_in_agent_conversation,
    reads_as_prose_with_unknown_head, reads_as_request_to_agent, strip_trailing_comment,
};
use crate::{
    ClassificationResult, Context, InputClassificationResult, InputClassifier,
    InputClassifierDecisionSource, InputType, SafetyGatedClassifier,
};

/// The buffer as the app parses it: whitespace tokens, and for an installed head the first token
/// that is not a `NAME=value` assignment carries the completer's command description (an external
/// command, alias, function, builtin or keyword all show up that way). With `parsed == false` the
/// token list is empty, which is what an Enter that beat the debounced parse works from.
fn snapshot(case: &Case, parsed: bool) -> ParsedTokensSnapshot {
    let buffer_text = case.line.to_owned();
    if !parsed {
        return ParsedTokensSnapshot {
            buffer_text,
            parsed_tokens: Vec::new(),
        };
    }
    let mut next_search_start = 0;
    let mut parsed_tokens: Vec<ParsedTokenData> = buffer_text
        .split_whitespace()
        .enumerate()
        .map(|(token_index, token)| {
            let start = buffer_text[next_search_start..].find(token).unwrap() + next_search_start;
            let end = start + token.len();
            next_search_start = end;
            ParsedTokenData {
                token: token.to_owned().spanned((start, end)),
                token_index,
                token_description: None,
            }
        })
        .collect();
    if case.head == Head::Installed
        && let Some(head) = parsed_tokens
            .iter_mut()
            .find(|token| !token.token.as_str().contains('='))
    {
        head.token_description = Some(Description {
            token: head.token.clone(),
            description_text: Some("Command".to_owned()),
            suggestion_type: SuggestionType::Command(TopLevelCommandCaseSensitivity::CaseSensitive),
        });
    }
    ParsedTokensSnapshot {
        buffer_text,
        parsed_tokens,
    }
}

fn in_conversation(case: &Case) -> bool {
    case.surface == Surface::Conversation
}

/// The classifier context the app builds for the case: an agent conversation is a follow-up with
/// AI as the live type; a complete command index is what lets "not described" mean "absent".
fn context(case: &Case) -> Context {
    Context {
        current_input_type: if in_conversation(case) {
            InputType::AI
        } else {
            InputType::Shell
        },
        is_agent_follow_up: in_conversation(case),
        commands_fully_loaded: case.head != Head::Unknown,
        in_active_agent_view: in_conversation(case),
        command_data_current: true,
    }
}

/// A classifier whose model votes one way, behind the deterministic prelude every shipped
/// classifier runs before its model (`onnx/mod.rs`, `heuristic_classifier/mod.rs`): a lone one-off
/// natural-language word is AI, a one-off shell keyword head is Shell, and
/// [`is_likely_shell_command`] (described tokens, shell-syntax tokens, a described head on a line
/// of fewer than three words) is Shell.
struct ModelVotes(InputType);

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl InputClassifier for ModelVotes {
    async fn detect_input_type(
        &self,
        input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> InputClassificationResult {
        let words: Vec<&str> = input.buffer_text.split_whitespace().collect();
        if let Some(first) = words.first() {
            let first = first.to_lowercase();
            if words.len() == 1 && is_one_off_natural_language_word(&first) {
                return InputClassificationResult::new(
                    InputType::AI,
                    InputClassifierDecisionSource::NaturalLanguageOneOffAllowlist,
                );
            }
            if is_one_off_shell_command_keyword(&first) {
                return InputClassificationResult::new(
                    InputType::Shell,
                    InputClassifierDecisionSource::ShellCommandAllowList,
                );
            }
        }
        if is_likely_shell_command(&input, words.len()).await {
            return InputClassificationResult::new(
                InputType::Shell,
                InputClassifierDecisionSource::ShellHeuristic,
            );
        }
        InputClassificationResult::new(self.0, InputClassifierDecisionSource::InputClassifier)
    }

    async fn classify_input(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> anyhow::Result<ClassificationResult> {
        Ok(match self.0 {
            InputType::Shell => {
                ClassificationResult::pure_shell(InputClassifierDecisionSource::InputClassifier)
            }
            InputType::AI => {
                ClassificationResult::pure_ai(InputClassifierDecisionSource::InputClassifier)
            }
        })
    }
}

/// The keystroke path of the current procedure, with the parse landed and the model voting
/// `model`: `detect_and_set_input_type`'s synchronous rules (the denylist and typeahead are not
/// modelled), the one-off shortcuts of its spawned task, then the gated classifier.
async fn v1_keystroke(case: &Case, model: InputType) -> InputType {
    let input = snapshot(case, true);
    let classify = strip_trailing_comment(&input).into_owned();
    let conversation = in_conversation(case);
    let no_evidence = conversation && lacks_command_evidence_in_agent_conversation(&classify);
    if is_unambiguous_shell_command(&classify) && !no_evidence {
        return InputType::Shell;
    }
    // In a conversation the last type the user chose is AI, which arms the one-off shortcut.
    let text = input.buffer_text.trim();
    let first = text.split_whitespace().next().unwrap_or("").to_lowercase();
    if conversation && is_one_off_natural_language_word(&first) {
        return InputType::AI;
    }
    if conversation && is_agent_follow_up_input(&text.to_lowercase()) {
        return InputType::AI;
    }
    SafetyGatedClassifier::new(ModelVotes(model))
        .detect_input_type(classify, &context(case))
        .await
        .input_type
}

/// The Enter path of the current procedure when Enter beat the parse: the Enter-time rules of
/// `settle_pending_detection_for_submit` on the parse-less buffer with `inherited` as the live
/// type, then the verdict wait for text with prose evidence (resolving to the keystroke verdict),
/// otherwise the inherited type.
async fn v1_enter(case: &Case, inherited: InputType, model: InputType) -> InputType {
    let input = snapshot(case, false);
    let classify = strip_trailing_comment(&input).into_owned();
    let text = input.buffer_text.trim();
    let conversation = in_conversation(case);
    let no_evidence = conversation && lacks_command_evidence_in_agent_conversation(&classify);
    let head_known = case.head == Head::Installed;
    let head_unknown = case.head == Head::Absent;
    if (case.ran_before && !no_evidence)
        || (is_unambiguous_shell_command(&classify) && !no_evidence)
    {
        return InputType::Shell;
    }
    if inherited == InputType::Shell
        && (reads_as_request_to_agent(text)
            || no_evidence
            || (head_unknown && reads_as_prose_with_unknown_head(text)))
    {
        return InputType::AI;
    }
    if inherited == InputType::AI
        && head_known
        && !no_evidence
        && !text.contains('\n')
        && !reads_as_request_to_agent(text)
    {
        return InputType::Shell;
    }
    if !has_no_prose_evidence(text) {
        return v1_keystroke(case, model).await;
    }
    inherited
}

fn matches(expect: Expect, route: InputType) -> bool {
    match expect {
        Expect::Shell => route == InputType::Shell,
        Expect::Agent => route == InputType::AI,
        Expect::Model => true,
    }
}

fn describe(case: &Case) -> String {
    format!(
        "{:?} [{:?} {:?} head={:?} ran_before={} typeahead={:?}] expected {:?} ({})",
        case.line,
        case.shell,
        case.surface,
        case.head,
        case.ran_before,
        case.typeahead,
        case.expect,
        case.issue
    )
}

/// Every outcome of the current procedure for the case, labelled by path.
async fn v1_outcomes(case: &Case) -> Vec<(String, InputType)> {
    let mut outcomes = Vec::new();
    for model in [InputType::Shell, InputType::AI] {
        outcomes.push((
            format!("keystroke, model votes {model}"),
            v1_keystroke(case, model).await,
        ));
        for inherited in [InputType::Shell, InputType::AI] {
            outcomes.push((
                format!("Enter before the parse, inherited {inherited}, model votes {model}"),
                v1_enter(case, inherited, model).await,
            ));
        }
    }
    outcomes
}

#[test]
fn corpus_is_consistent() {
    let cases = all_cases();
    assert!(cases.len() >= 400, "{} cases", cases.len());
    for case in &cases {
        assert!(!case.line.trim().is_empty(), "{}", describe(case));
        assert!(!case.issue.is_empty(), "{}", describe(case));
    }
    // The same facts never carry two expectations.
    for (index, case) in cases.iter().enumerate() {
        for other in &cases[index + 1..] {
            let same_facts = case.line == other.line
                && case.shell == other.shell
                && case.surface == other.surface
                && case.head == other.head
                && case.ran_before == other.ran_before
                && case.typeahead == other.typeahead;
            assert!(
                !same_facts || case.expect == other.expect,
                "conflicting expectations:\n  {}\n  {}",
                describe(case),
                describe(other)
            );
        }
    }
}

/// Stage 1 (#1234): the corpus against the current procedure. Typeahead provenance lives in the
/// app's model, not in this crate's rules, so those cases are left to the stage-2 harness.
#[test]
fn corpus_against_the_current_procedure() {
    futures::executor::block_on(async move {
        let mut passed = 0usize;
        let mut expected_failures = Vec::new();
        let mut failures = Vec::new();
        let mut unexpected_passes = Vec::new();
        for case in all_cases()
            .iter()
            .filter(|case| case.typeahead == Typeahead::None)
        {
            let outcomes = v1_outcomes(case).await;
            let misses: Vec<String> = outcomes
                .iter()
                .filter(|(_, route)| !matches(case.expect, *route))
                .map(|(path, route)| format!("    {path}: {route}"))
                .collect();
            match (case.v1_xfail, misses.is_empty()) {
                (None, true) => passed += 1,
                (None, false) => {
                    failures.push(format!("{}\n{}", describe(case), misses.join("\n")))
                }
                (Some(issue), false) => expected_failures.push(format!("{} -- {issue}", case.line)),
                (Some(issue), true) => unexpected_passes
                    .push(format!("{} -- marked xfail for {issue}", describe(case))),
            }
        }
        println!(
            "routing corpus against the current procedure: {passed} pass, {} expected failures, {} \
             failures, {} unexpected passes",
            expected_failures.len(),
            failures.len(),
            unexpected_passes.len()
        );
        for line in &expected_failures {
            println!("  xfail: {line}");
        }
        assert!(
            failures.is_empty(),
            "{} cases the current procedure misroutes and no issue tracks:\n{}",
            failures.len(),
            failures.join("\n")
        );
        assert!(
            unexpected_passes.is_empty(),
            "{} cases marked as known misses now pass; drop their `v1_xfail`:\n{}",
            unexpected_passes.len(),
            unexpected_passes.join("\n")
        );
    });
}
