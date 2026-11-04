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

// ---------------------------------------------------------------------------------------------
// Stage 2: the corpus against `util::routing::route` (#1234).
// ---------------------------------------------------------------------------------------------

use crate::routing_corpus::Shell;
use crate::util::routing::{
    self, Route, RoutingContext, ShellFamily, Surface as RoutingSurface,
    Typeahead as RoutingTypeahead,
};

/// The facts the app hands `route` at the keystroke (the parse has landed, the session has not
/// been asked about the head) or at Enter (no parse, the session's answer about the head).
fn routing_context(case: &Case, at_enter: bool) -> RoutingContext {
    RoutingContext {
        surface: if in_conversation(case) {
            RoutingSurface::AgentConversation
        } else {
            RoutingSurface::PlainTerminal
        },
        shell: match case.shell {
            Shell::Posix => ShellFamily::Posix,
            Shell::Fish => ShellFamily::Fish,
            Shell::PowerShell => ShellFamily::PowerShell,
            Shell::Nu => ShellFamily::Nu,
        },
        head_known_to_shell: if at_enter {
            match case.head {
                Head::Installed => Some(true),
                Head::Absent => Some(false),
                Head::Unknown => None,
            }
        } else {
            None
        },
        commands_fully_loaded: !at_enter && case.head != Head::Unknown,
        command_data_current: !at_enter,
        is_agent_follow_up: in_conversation(case),
        ran_before_exactly: case.ran_before,
        denylisted: false,
        typeahead: match case.typeahead {
            Typeahead::None => RoutingTypeahead::None,
            Typeahead::Running => RoutingTypeahead::Running,
        },
    }
}

fn route_matches(expect: Expect, route: Route) -> bool {
    match expect {
        Expect::Shell => route == Route::Shell,
        Expect::Agent => route == Route::Agent,
        Expect::Model => route == Route::Undecided,
    }
}

/// Stage 2 (#1234): every case routes as expected at the keystroke and at Enter, and a case the
/// classifier must decide is left undecided on both.
#[test]
fn corpus_against_the_routing_procedure() {
    let mut failures = Vec::new();
    let cases = all_cases();
    for case in &cases {
        for at_enter in [false, true] {
            let input = snapshot(case, !at_enter);
            let decision = routing::route(&input, &routing_context(case, at_enter));
            if !route_matches(case.expect, decision.route) {
                failures.push(format!(
                    "{}\n    {}: {:?} by {}",
                    describe(case),
                    if at_enter { "Enter" } else { "keystroke" },
                    decision.route,
                    decision.rule
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} cases misroute:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}

/// The procedure never waits: a decision that is not `Undecided` is final, and the only
/// `Undecided` lines are the ones the corpus marks for the classifier.
#[test]
fn undecided_is_reserved_for_the_classifier() {
    for case in all_cases() {
        if case.expect == Expect::Model {
            continue;
        }
        let decision = routing::route(&snapshot(&case, true), &routing_context(&case, false));
        assert_ne!(
            decision.route,
            Route::Undecided,
            "{} would wait for a verdict",
            describe(&case)
        );
    }
}

/// #1290: in a conversation, a line that closes on `please` is a message to the agent even when
/// its unknown head carries paths (`XTOOLS ~/dir/fn.sh please` ran in the shell: "command not
/// found"). Checked against `route` directly rather than the corpus, which stage 1 also replays
/// against the pre-#1234 procedure. A bare data-shaped line with no `please` stays the
/// classifier's, an installed head keeps its own rule, and the plain terminal is unchanged.
#[test]
fn unknown_head_with_paths_and_a_closing_please_goes_to_the_agent() {
    for line in [
        "XTOOLS ~/tmp-ssh-matrix/fn.sh ~/tmp-ssh-matrix please",
        "XTOOLS /Users/winters/tmp-ssh-matrix/fn.sh please",
        "mytool --json please",
        "mytool ./out.txt please.",
    ] {
        for head in [Head::Absent, Head::Unknown] {
            let case = Case::model(line, "#1290").in_conversation().head(head);
            for at_enter in [false, true] {
                let decision = routing::route(
                    &snapshot(&case, !at_enter),
                    &routing_context(&case, at_enter),
                );
                assert_eq!(
                    decision.route,
                    Route::Agent,
                    "{} ({}): {}",
                    describe(&case),
                    if at_enter { "Enter" } else { "keystroke" },
                    decision.rule
                );
            }
        }
    }
    // Without the `please` the line is still the classifier's.
    let case = Case::model("XTOOLS ~/tmp-ssh-matrix/fn.sh ~/tmp-ssh-matrix", "#1290")
        .in_conversation()
        .head(Head::Absent);
    let decision = routing::route(&snapshot(&case, true), &routing_context(&case, false));
    assert_eq!(decision.route, Route::Undecided, "{}", decision.rule);
}

/// #1290 (owner, 2026-11-03: "an unknown word autodetected as shell" in agent mode): inside an
/// agent conversation a lone word nothing on the session answers to -- not installed, no shell
/// syntax, not a known command word or a path -- is a message to the agent, whether the session
/// knows the word is absent or cannot say, at the keystroke and at Enter, in every shell family,
/// in a fresh conversation (no agent block yet) and after an agent turn. The other direction
/// holds on the same facts: an installed lone word and an explicit path are commands, and clear
/// shell structure around an unknown head runs in the shell (#1560, owner decision).
#[test]
fn lone_unknown_word_in_a_conversation_goes_to_the_agent() {
    fn route_for(case: &Case, at_enter: bool, follow_up: bool) -> routing::Decision {
        let mut context = routing_context(case, at_enter);
        context.is_agent_follow_up = follow_up;
        routing::route(&snapshot(case, !at_enter), &context)
    }
    let shells = [Shell::Posix, Shell::Fish, Shell::PowerShell, Shell::Nu];
    for line in ["XTOOLS", "frobxone", "zzqx", "my_tool2", "foo-bar"] {
        for shell in shells {
            for follow_up in [false, true] {
                for at_enter in [false, true] {
                    for head in [Head::Absent, Head::Unknown] {
                        let case = Case::prose(line, "#1290")
                            .in_conversation()
                            .head(head)
                            .shell(shell);
                        let decision = route_for(&case, at_enter, follow_up);
                        assert_eq!(
                            decision.route,
                            Route::Agent,
                            "{} (follow-up {follow_up}, {}): {}",
                            describe(&case),
                            if at_enter { "Enter" } else { "keystroke" },
                            decision.rule
                        );
                    }
                    // Installed, the same word is a command.
                    let case = Case::cmd(line, "#1290").in_conversation().shell(shell);
                    let decision = route_for(&case, at_enter, follow_up);
                    assert_eq!(
                        decision.route,
                        Route::Shell,
                        "{} (follow-up {follow_up}, {}): {}",
                        describe(&case),
                        if at_enter { "Enter" } else { "keystroke" },
                        decision.rule
                    );
                }
            }
        }
    }
    for (line, shells) in [
        ("./xtools", &shells[..]),
        ("XTOOLS --json --quiet", &shells[..2]),
        ("XTOOLS > out.txt", &shells[..2]),
        ("XTOOLS $HOME/x", &shells[..2]),
    ] {
        for &shell in shells {
            for follow_up in [false, true] {
                for at_enter in [false, true] {
                    for head in [Head::Absent, Head::Unknown] {
                        let case = Case::model(line, "#1290")
                            .in_conversation()
                            .head(head)
                            .shell(shell);
                        let decision = route_for(&case, at_enter, follow_up);
                        assert_eq!(
                            decision.route,
                            Route::Shell,
                            "{} (follow-up {follow_up}, {}): {}",
                            describe(&case),
                            if at_enter { "Enter" } else { "keystroke" },
                            decision.rule
                        );
                    }
                }
            }
        }
    }
}

/// #1344:`@(1,2,3)` / `@{a=1}` are PowerShell literals only in a PowerShell session; everywhere
/// else, and for the `@` context mention (`@agent please help`, `@file.rs explain this`), the
/// line never takes the shell-syntax route. Checked against `route` directly: stage 1 replays the
/// corpus against the pre-#1234 procedure, where a Shell-voting model decides mentions.
#[test]
fn at_literals_are_shell_only_in_powershell_and_mentions_are_not() {
    for line in [
        "@agent please help",
        "@file.rs explain this",
        "@src/main.rs explain this",
    ] {
        for shell in [Shell::PowerShell, Shell::Posix, Shell::Fish] {
            for conversation in [false, true] {
                let mut case = Case::prose(line, "#1344 mention").shell(shell);
                if conversation {
                    case = case.in_conversation();
                }
                for at_enter in [false, true] {
                    let decision = routing::route(
                        &snapshot(&case, !at_enter),
                        &routing_context(&case, at_enter),
                    );
                    assert_ne!(
                        decision.route,
                        Route::Shell,
                        "{} ({}): {}",
                        describe(&case),
                        if at_enter { "Enter" } else { "keystroke" },
                        decision.rule
                    );
                }
            }
        }
    }
    // The literal is shell syntax only for PowerShell.
    for line in ["@(1,2,3)", "@{a=1}"] {
        let case = Case::model(line, "#1344")
            .shell(Shell::Posix)
            .in_conversation();
        let decision = routing::route(&snapshot(&case, true), &routing_context(&case, false));
        assert_ne!(decision.rule, routing::Rule::ShellSyntax, "{line}");
    }
}

/// The rule names are stable identifiers for the debug log.
#[test]
fn every_rule_names_itself() {
    use routing::Rule;
    for rule in [
        Rule::Empty,
        Rule::Denylist,
        Rule::RunningTypeahead,
        Rule::SlashSpelling,
        Rule::LoneWord,
        Rule::FollowUpWord,
        Rule::CommandShape,
        Rule::WrapperChain,
        Rule::ShellSyntax,
        Rule::RequestShape,
        Rule::SentenceShape,
        Rule::OneOffWord,
        Rule::ShellStructureUnknownHead,
        Rule::RanBefore,
        Rule::NoEvidenceInConversation,
        Rule::UnknownHeadInConversation,
        Rule::EvidenceInConversation,
        Rule::QuestionHead,
        Rule::InstalledHead,
        Rule::EnglishHeadNotInstalled,
        Rule::ProseWithUnknownHead,
        Rule::AbsentHeadWithProse,
        Rule::Model,
    ] {
        assert!(!rule.to_string().is_empty(), "{rule:?}");
    }
}

/// Lines with clear shell structure and a head the session does not know (#1560, owner decision):
/// the shell runs them (and says `command not found`), on the plain terminal and inside an agent
/// conversation, at the keystroke and at Enter, in every shell family that writes them this way.
const SHELL_STRUCTURE_UNKNOWN_HEAD_LINES: &[&str] = &[
    "print -r -- $EPOCHREALTIME > lat_0",
    "foo --bar | grep x",
    "notacmd $HOME/x",
    "xyz > out.txt",
    "frobnicate -r --force",
    "mytool ${CFG_DIR}/a.toml",
    "mytool $(pwd)",
    "mytool 2>&1",
    "mytool &> all.log",
    "mytool < in.txt",
    "FOO=1 mytool >> run_1",
];

/// Prose that mentions the same characters stays out of the shell: a comparison, a pipe between
/// two words, a flag in a question, a variable in a sentence, non-English text.
const PROSE_WITH_SHELL_CHARACTERS: &[&str] = &[
    "what is 5 > 3",
    "what's 5 > 3",
    "compare A | B",
    "tabs | spaces",
    "can you show me how to use --verbose?",
    "explain $PATH to me",
    "tell me what $HOME means",
    "list the files > 1GB",
    "use --verbose",
    "why does a > b hold",
    "解释 $PATH",
    "is x < y",
];

#[test]
fn clear_shell_structure_beats_an_unknown_head() {
    for line in SHELL_STRUCTURE_UNKNOWN_HEAD_LINES {
        for conversation in [false, true] {
            for head in [Head::Absent, Head::Unknown] {
                for shell in [Shell::Posix, Shell::Fish, Shell::Nu] {
                    let mut case = Case::model(line, "#1560").head(head).shell(shell);
                    if conversation {
                        case = case.in_conversation();
                    }
                    for at_enter in [false, true] {
                        let decision = routing::route(
                            &snapshot(&case, !at_enter),
                            &routing_context(&case, at_enter),
                        );
                        assert_eq!(
                            decision.route,
                            Route::Shell,
                            "{} ({}): {}",
                            describe(&case),
                            if at_enter { "Enter" } else { "keystroke" },
                            decision.rule
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn prose_with_shell_characters_never_takes_the_structure_rule() {
    for line in PROSE_WITH_SHELL_CHARACTERS {
        for conversation in [false, true] {
            for head in [Head::Absent, Head::Unknown] {
                let mut case = Case::model(line, "#1560").head(head);
                if conversation {
                    case = case.in_conversation();
                }
                for at_enter in [false, true] {
                    let decision = routing::route(
                        &snapshot(&case, !at_enter),
                        &routing_context(&case, at_enter),
                    );
                    assert_ne!(
                        decision.rule,
                        routing::Rule::ShellStructureUnknownHead,
                        "{} ({}): {:?}",
                        describe(&case),
                        if at_enter { "Enter" } else { "keystroke" },
                        decision.route
                    );
                }
            }
        }
    }
}

/// #1604: there is no size above which a line is not routed. A 5000-byte shell line, a 5000-byte
/// multi-line script and 5000 bytes of prose each route exactly as their short forms do (the
/// former `Rule::Oversized` left all three undecided, and the app ran them on the inherited
/// type). Both the keystroke facts (no parse, nothing known) and the Enter facts are tried.
#[test]
fn long_lines_route_like_their_short_forms() {
    fn context(surface: RoutingSurface) -> RoutingContext {
        RoutingContext {
            surface,
            shell: ShellFamily::Posix,
            head_known_to_shell: None,
            commands_fully_loaded: false,
            command_data_current: false,
            is_agent_follow_up: false,
            ran_before_exactly: false,
            denylisted: false,
            typeahead: RoutingTypeahead::None,
        }
    }
    fn unparsed(text: &str) -> ParsedTokensSnapshot {
        ParsedTokensSnapshot {
            buffer_text: text.to_owned(),
            parsed_tokens: Vec::new(),
        }
    }
    let shell_line = format!("echo {}; echo PASTE-LONG-END", "PASTE-G1 ".repeat(600));
    let script = vec![format!("echo {}; echo PASTE-H1", "PASTE-G1 ".repeat(12)); 40].join("\n");
    let prose = "please explain why the build failed ".repeat(140);
    for (short, long, expected) in [
        (
            "echo PASTE-G1; echo PASTE-LONG-END",
            shell_line,
            Route::Shell,
        ),
        (
            "echo PASTE-G1; echo PASTE-H1\necho PASTE-G1; echo PASTE-H1",
            script,
            Route::Shell,
        ),
        ("please explain why the build failed", prose, Route::Agent),
    ] {
        assert!(long.len() > 4096, "{}", long.len());
        for surface in [
            RoutingSurface::PlainTerminal,
            RoutingSurface::AgentConversation,
        ] {
            let short_decision = routing::route(&unparsed(short), &context(surface));
            let long_decision = routing::route(&unparsed(&long), &context(surface));
            assert_eq!(short_decision.route, expected, "{short:?} on {surface:?}");
            assert_eq!(
                long_decision.route,
                expected,
                "{} bytes on {surface:?}: {} (short form: {})",
                long.len(),
                long_decision.rule,
                short_decision.rule
            );
            assert_eq!(
                long_decision.rule,
                short_decision.rule,
                "{} bytes",
                long.len()
            );
        }
    }
}
