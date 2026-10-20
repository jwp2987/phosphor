use warp_completer::completer::{Description, SuggestionType, TopLevelCommandCaseSensitivity};
use warp_completer::meta::SpannedItem;
use warp_completer::{ParsedTokenData, ParsedTokensSnapshot};

use super::*;
use crate::{ClassificationResult, Context, HeuristicClassifier, InputType};

/// A fake classifier that always says Shell, no matter what. Wrapping it in
/// [`SafetyGatedClassifier`] and checking that the two observed exploit prompts still come out as
/// AI proves the safety invariant is enforced by the wrapper itself — independent of which real
/// classifier (ONNX, fasttext, heuristic) is loaded, and independent of whether that classifier's
/// own internal reasoning happens to already avoid the bug (#696).
struct AlwaysShellClassifier;

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl InputClassifier for AlwaysShellClassifier {
    async fn detect_input_type(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> InputClassificationResult {
        InputClassificationResult::new(
            InputType::Shell,
            InputClassifierDecisionSource::ShellHeuristic,
        )
    }

    async fn classify_input(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> anyhow::Result<ClassificationResult> {
        Ok(ClassificationResult::pure_shell(
            InputClassifierDecisionSource::ShellHeuristic,
        ))
    }
}

/// `commands_fully_loaded` selects which of the gate's two regimes a test exercises: `true`
/// simulates a completion context with a complete view of top-level commands (a finished
/// `$PATH` scan, if any), where dictionary membership alone is trusted evidence of prose;
/// `false` simulates one that is still in flight or has none at all (`EmptyCompletionContext`),
/// where only a stronger prose *shape* may override. See `util::first_token_forces_ai_override`.
fn context(commands_fully_loaded: bool) -> Context {
    Context {
        current_input_type: InputType::AI,
        is_agent_follow_up: false,
        commands_fully_loaded,
        in_active_agent_view: false,
    }
}

/// Builds a [`ParsedTokensSnapshot`] with no completer knowledge at all (every
/// `token_description` is `None`), matching a context with nothing indexed yet
/// (`EmptyCompletionContext`, or `external_commands` not having loaded).
fn snapshot_without_descriptions(buffer_text: &str) -> ParsedTokensSnapshot {
    let mut next_search_start = 0;
    let parsed_tokens = buffer_text
        .split_whitespace()
        .enumerate()
        .map(|(token_index, token)| {
            let token_start =
                buffer_text[next_search_start..].find(token).unwrap() + next_search_start;
            let token_end = token_start + token.len();
            next_search_start = token_end;

            ParsedTokenData {
                token: token.to_string().spanned((token_start, token_end)),
                token_index,
                token_description: None,
            }
        })
        .collect();

    ParsedTokensSnapshot {
        buffer_text: buffer_text.to_string(),
        parsed_tokens,
    }
}

/// Like [`snapshot_without_descriptions`], but gives the buffer's first token a
/// `token_description`, the way the real completer would for a first word that resolves to a
/// known external command, shell function, builtin, or alias:
/// `CompletionContext::top_level_commands()` chains external commands together with
/// `functions()`, `builtins()`, and `aliases()` (see `util::has_command_evidence`'s doc comment),
/// so all four show up identically as "the first token has a `token_description`" by the time a
/// classifier sees the buffer. Used to simulate a user's own shell function/builtin/alias whose
/// name happens to also be an ordinary English word (e.g. a function `deploy`, an alias
/// `please`), which must stay Shell regardless of dictionary membership.
fn snapshot_with_first_token_described(buffer_text: &str) -> ParsedTokensSnapshot {
    let mut snapshot = snapshot_without_descriptions(buffer_text);
    if let Some(first_token) = snapshot.parsed_tokens.first_mut() {
        first_token.token_description = Some(Description {
            token: first_token.token.clone(),
            description_text: Some("Shell function".to_string()),
            suggestion_type: SuggestionType::Command(TopLevelCommandCaseSensitivity::CaseSensitive),
        });
    }
    snapshot
}

#[test]
fn test_gate_overrides_observed_exploit_prompts_regardless_of_inner_classifier() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            "Run exactly this…: sleep 8; echo hi > ~/lrc.txt",
            "Then run: echo hi > ~/followup.txt",
            "Then delete the temp directory",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(
                result.input_type,
                InputType::AI,
                "expected {buffer:?} to be overridden to AI even though the inner classifier \
                 always says Shell"
            );
            assert_eq!(
                result.source,
                InputClassifierDecisionSource::NoFirstTokenCommandEvidence
            );
        }
    });
}

/// The exploit prompts are only ever reported to us capitalized, but the shipped ONNX
/// classifier's tokenizer lowercases its input before scoring
/// (`models/onnx/bert_tiny_tokenizer.json`'s `BertNormalizer { lowercase: true }`), so a
/// lowercase prompt gets exactly the same Shell verdict from the model as its capitalized form.
/// The gate has to close for both, purely from dictionary evidence, when the completion context
/// has a complete view of top-level commands.
#[test]
fn test_gate_overrides_lowercase_exploit_prompts_when_commands_fully_loaded() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            "run exactly this: sleep 8; echo hi > ~/lrc.txt",
            "then run: echo hi > ~/followup.txt",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(
                result.input_type,
                InputType::AI,
                "expected lowercase {buffer:?} to be overridden to AI with the command index \
                 fully loaded"
            );
        }
    });
}

/// Same lowercase exploit prompts, but with the command index *not* fully loaded (e.g. an
/// in-flight `$PATH` scan, or `EmptyCompletionContext`). Dictionary membership alone isn't
/// trusted here, but both prompts still have a later token ending in `:` before their first
/// shell operator ("this:", "run:"), which is the prose-shape signal that closes the gate even
/// without loaded-index evidence.
#[test]
fn test_gate_overrides_lowercase_exploit_prompts_via_prose_shape_when_commands_not_loaded() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            "run exactly this: sleep 8; echo hi > ~/lrc.txt",
            "then run: echo hi > ~/followup.txt",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(false)).await;
            assert_eq!(
                result.input_type,
                InputType::AI,
                "expected lowercase {buffer:?} to be overridden to AI via its prose shape even \
                 with the command index not yet loaded"
            );
        }
    });
}

/// An ordinary English word that simply isn't in a *fully loaded* index (as opposed to one the
/// index hasn't gotten to yet) is prose, full stop — no colon or other special shape required.
#[test]
fn test_gate_overrides_english_word_not_in_loaded_index() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        let input = snapshot_without_descriptions("run this; echo hi > x");
        let result = gated.detect_input_type(input, &context(true)).await;
        assert_eq!(
            result.input_type,
            InputType::AI,
            "expected an ordinary English word absent from a fully-loaded command index to be \
             overridden to AI"
        );
    });
}

/// The override must not fire for input that merely lacks command evidence because the
/// completion context hasn't finished resolving it yet: an unknown binary, a path, a leading
/// environment assignment, or a real command the completer hasn't indexed yet all have to stay
/// Shell (i.e. the inner classifier's decision passes through unchanged). This is exactly the
/// "commands not fully loaded" regime, so `context(false)` is used throughout.
#[test]
fn test_gate_does_not_override_commands_without_evidence_when_index_not_loaded() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            "mynewtool arg1 arg2",
            "./script.sh a b",
            "~/bin/x a b",
            "FOO=1 mycmd",
            // Simulates cargo not yet being indexed by `external_commands` (loads once per
            // session): no token_description anywhere, and "cargo" is an ordinary English
            // dictionary word too, but the index isn't fully loaded, so dictionary membership
            // alone must not be trusted.
            "cargo --version",
            // A real command with a described flag but an unindexed lowercase command name.
            "rvm install 3.3",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(false)).await;
            assert_eq!(
                result.input_type,
                InputType::Shell,
                "expected {buffer:?} to stay Shell: no command evidence, but also no trustworthy \
                 prose signal"
            );
        }
    });
}

/// Realistic command lines that could plausibly trip the prose-shape check (a later token ending
/// in `:`) if it weren't scoped correctly. Checked with the command index not loaded, since
/// that's the only regime where the shape check runs at all.
#[test]
fn test_gate_prose_colon_shape_does_not_match_realistic_commands() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            // The colon sits inside a token ("80:80"), never at the end of one.
            "docker run -p 80:80",
            // Same: "host:path" doesn't end with ':'.
            "scp host:path .",
            // "a:" does end with ':', but "echo" is a one-off shell keyword with command
            // evidence of its own, so this buffer never reaches the prose-shape check.
            "echo a: b",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(false)).await;
            assert_eq!(
                result.input_type,
                InputType::Shell,
                "expected {buffer:?} to stay Shell: not an English-word-led buffer with a prose \
                 colon shape"
            );
        }
    });
}

/// A first token that resolves to a user's own shell function, builtin, or alias must stay Shell
/// even when its name happens to also be an ordinary English word, and even with the command
/// index fully loaded (the regime where dictionary membership alone would otherwise be enough to
/// override) -- the `token_description` this token carries is command evidence that short-circuits
/// the dictionary/prose checks entirely.
#[test]
fn test_gate_does_not_override_known_function_builtin_or_alias() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            // A user-defined shell function named "deploy".
            "deploy --prod",
            // A user-defined alias named "please".
            "please clean up",
        ] {
            let input = snapshot_with_first_token_described(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(
                result.input_type,
                InputType::Shell,
                "expected {buffer:?} to stay Shell: the first token is a known function/alias, \
                 not prose"
            );
        }
    });
}

/// `cargo build` with `cargo` present in a fully-loaded command index must stay Shell -- once the
/// index has actually resolved the name, "cargo" being an English dictionary word is irrelevant.
#[test]
fn test_gate_does_not_override_indexed_cargo_build() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        let input = snapshot_with_first_token_described("cargo build");
        let result = gated.detect_input_type(input, &context(true)).await;
        assert_eq!(
            result.input_type,
            InputType::Shell,
            "expected an indexed \"cargo build\" to stay Shell"
        );
    });
}

/// End-to-end sanity check with the real, shipped fallback classifier (not the fake): the two
/// observed exploit prompts must come out as AI, and ordinary shell usage must not.
///
/// `snapshot_without_descriptions` gives every token `token_description: None`, which is also
/// exactly what a completion context with no command knowledge at all produces —
/// `EmptyCompletionContext`, used for shared-session viewers
/// (`app/src/terminal/input/decorations.rs`'s `CompletionSessionContext::Empty` branch), and any
/// context before `external_commands` has finished loading for the session. `context(false)`
/// matches that surface: with zero command knowledge, real-looking lowercase commands must still
/// stay Shell, not get force-flipped to AI just because nothing described them, while the
/// exploit prompts are still caught via their prose shape.
#[test]
fn test_gate_with_real_heuristic_classifier() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(HeuristicClassifier);

        for buffer in [
            "Run exactly this…: sleep 8; echo hi > ~/lrc.txt",
            "Then run: echo hi > ~/followup.txt",
        ] {
            let input = snapshot_without_descriptions(buffer);
            assert_eq!(
                gated
                    .detect_input_type(input, &context(false))
                    .await
                    .input_type,
                InputType::AI,
                "expected {buffer:?} to classify as AI end-to-end"
            );
        }

        for buffer in ["ls -la", "git status", "sudo apt update"] {
            let input = snapshot_without_descriptions(buffer);
            assert_eq!(
                gated
                    .detect_input_type(input, &context(false))
                    .await
                    .input_type,
                InputType::Shell,
                "expected {buffer:?} to stay Shell end-to-end with no command knowledge available"
            );
        }
    });
}

/// A fake classifier that always says AI. Wrapping it in [`SafetyGatedClassifier`] and checking
/// that the two inputs from #808 still come out as Shell proves the unambiguous-shell pre-filter
/// runs *before* the inner classifier gets a say, independent of which one is loaded.
struct AlwaysAIClassifier;

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl InputClassifier for AlwaysAIClassifier {
    async fn detect_input_type(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> InputClassificationResult {
        InputClassificationResult::new(
            InputType::AI,
            InputClassifierDecisionSource::InputClassifier,
        )
    }

    async fn classify_input(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> anyhow::Result<ClassificationResult> {
        Ok(ClassificationResult::pure_ai(
            InputClassifierDecisionSource::InputClassifier,
        ))
    }
}

#[test]
fn test_gate_forces_shell_for_unambiguous_commands_regardless_of_inner_classifier() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysAIClassifier);

        // No descriptions and `commands_fully_loaded == true`: the regime in which the #696
        // override would otherwise have flipped `trap` (an ordinary English word) to AI.
        for buffer in [
            "yes | head -c 50000000 > /dev/null",
            "trap 'echo t' DEBUG",
            "cat foo.txt | grep -i error",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(
                result.input_type,
                InputType::Shell,
                "expected {buffer:?} to be forced to Shell even though the inner classifier \
                 always says AI"
            );
            assert_eq!(
                result.source,
                InputClassifierDecisionSource::UnambiguousShellSyntax
            );
        }

        // Prose with shell metacharacters in it is not pre-filtered; the inner classifier
        // decides.
        for buffer in [
            "what does > mean?",
            "run exactly this: sleep 8; echo hi > ~/lrc.txt",
            // An assignment prefix must not turn the #696 exploit into head evidence.
            "FOO=1 run exactly this: sleep 8; echo hi > ~/x",
            "find all files > 1MB",
            "make sure the build passes; rm -rf target and rebuild",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(
                result.input_type,
                InputType::AI,
                "expected {buffer:?} to reach the inner classifier"
            );
        }
    });
}

/// #834: prose that starts with an installed command is overridden to AI even though the first
/// word has command evidence, while real command lines (pipelines, lists, heredocs, quoted
/// arguments, one-off keyword heads, globs) stay Shell.
#[test]
fn test_gate_overrides_command_headed_prose_to_ai() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);
        for buffer in [
            "help me",
            "help me with this",
            "docker vs podman?",
            "why is the build slow",
            "can you fix this",
            "watch the logs for errors",
            "make sure the build passes",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(
                result.input_type,
                InputType::AI,
                "{buffer:?} should go to AI"
            );
        }
        for buffer in [
            "git status",
            "cargo build --release",
            "git commit -m \"feat: add the thing\"",
            "git commit -m \"fix the bug\"",
            "echo \"to be or not to be\"",
            "for i in 1 2 3; do echo $i; done",
            "if true; then echo y; fi",
            "cat a b > c.txt",
            "ls -la | grep to",
            "grep -r for src",
            "kubectl get pods -n the",
            "cd my",
            "ls me",
            "cd x && if test -f y; then echo z; fi",
            "ls file?",
            "rm -rf tmp?",
            "echo is it ok?",
            "sudo apt install it",
            "grep For file",
            "cat <<EOF\nto is and\nEOF",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context(false)).await;
            assert_eq!(
                result.input_type,
                InputType::Shell,
                "{buffer:?} should stay Shell"
            );
        }
    });
}

/// #834: the same shell lines stay Shell after the inner classifier says Shell with commands fully
/// loaded (the override path that previously only the prose list exercised).
#[test]
fn test_gate_keeps_pipelines_and_keyword_heads_shell_when_commands_loaded() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);
        for buffer in [
            "ls -la | grep to",
            "cd x && if test -f y; then echo z; fi",
            "echo is it ok?",
            "sudo apt install it",
            "cat <<EOF\nto is and\nEOF",
        ] {
            // With commands fully loaded a real head (`cd` is a builtin, and also an English
            // word) always resolves to a description; without one it would be overridden.
            let input = snapshot_with_first_token_described(buffer);
            let result = gated.detect_input_type(input, &context(true)).await;
            assert_eq!(result.input_type, InputType::Shell, "{buffer:?}");
        }
    });
}

/// A fake classifier that always says AI, like a model that scores every line with an English
/// word in it as a request (#906).
struct AlwaysAiClassifier;

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl InputClassifier for AlwaysAiClassifier {
    async fn detect_input_type(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> InputClassificationResult {
        InputClassificationResult::new(
            InputType::AI,
            InputClassifierDecisionSource::InputClassifier,
        )
    }

    async fn classify_input(
        &self,
        _input: ParsedTokensSnapshot,
        _context: &Context,
    ) -> anyhow::Result<ClassificationResult> {
        Ok(ClassificationResult::pure_shell(
            InputClassifierDecisionSource::ShellHeuristic,
        ))
    }
}

/// The first non-assignment, non-wrapper-free word of the line gets a `token_description`, the
/// way the completer describes the first word of a command; wrappers are described, their
/// arguments are not.
fn snapshot_head_known(buffer_text: &str) -> ParsedTokensSnapshot {
    let mut snapshot = snapshot_without_descriptions(buffer_text);
    let index = snapshot
        .parsed_tokens
        .iter()
        .position(|token| !token.token.as_str().contains('='))
        .unwrap_or(0);
    if let Some(token) = snapshot.parsed_tokens.get_mut(index) {
        token.token_description = Some(Description {
            token: token.token.clone(),
            description_text: Some("Command".to_string()),
            suggestion_type: SuggestionType::Command(TopLevelCommandCaseSensitivity::CaseSensitive),
        });
    }
    snapshot
}

#[test]
fn test_gate_vetoes_ai_for_known_command_corpus_through_the_whole_gate() {
    use crate::request_corpus::{PROSE_WITH_COMMAND_HEADS, REAL_COMMANDS_WITH_ENGLISH_ARGS};
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysAiClassifier);
        let mut leaked = Vec::new();
        for buffer in REAL_COMMANDS_WITH_ENGLISH_ARGS {
            for loaded in [true, false] {
                let result = gated
                    .detect_input_type(snapshot_head_known(buffer), &context(loaded))
                    .await;
                if result.input_type != InputType::Shell {
                    leaked.push(*buffer);
                }
            }
        }
        assert!(
            leaked.is_empty(),
            "{} leak to the agent: {leaked:#?}",
            leaked.len()
        );

        let mut stuck_shell = Vec::new();
        for buffer in PROSE_WITH_COMMAND_HEADS {
            let result = gated
                .detect_input_type(snapshot_head_known(buffer), &context(true))
                .await;
            if result.input_type != InputType::AI {
                stuck_shell.push(*buffer);
            }
        }
        assert!(
            stuck_shell.is_empty(),
            "prose vetoed to Shell: {stuck_shell:#?}"
        );
    });
}

#[test]
fn test_gate_keeps_ai_when_head_is_unknown_or_session_cannot_say() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysAiClassifier);
        for buffer in [
            "grep this that",
            "make the build",
            "nohup grep this the file",
        ] {
            // No token_description anywhere: unknown head / stale or absent session data.
            let result = gated
                .detect_input_type(snapshot_without_descriptions(buffer), &context(false))
                .await;
            assert_eq!(result.input_type, InputType::AI, "{buffer:?}");
        }
        // Multi-line pastes are not decided by the veto.
        let result = gated
            .detect_input_type(
                snapshot_head_known("find the bug\nin main.rs"),
                &context(true),
            )
            .await;
        assert_eq!(result.input_type, InputType::AI);
    });
}

/// #935: the context of a follow-up typed straight after an agent block inside an active agent
/// view (e.g. after a Ctrl+C reject or an approve).
fn agent_conversation_context() -> Context {
    Context {
        in_active_agent_view: true,
        is_agent_follow_up: true,
        ..context(true)
    }
}

#[test]
fn test_gate_sends_an_unknown_single_word_to_the_agent_inside_a_conversation() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);
        for buffer in ["frobxone", "zzqx", "frob-two", "frob_three9"] {
            let result = gated
                .detect_input_type(
                    snapshot_without_descriptions(buffer),
                    &agent_conversation_context(),
                )
                .await;
            assert_eq!(result.input_type, InputType::AI, "{buffer:?}");
            assert_eq!(
                result.source,
                InputClassifierDecisionSource::UnknownSingleWordInAgentConversation
            );
        }
    });
}

#[test]
fn test_gate_keeps_known_commands_and_other_shapes_shell_inside_a_conversation() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);
        // Known commands carry a description (`ls`, `git`, a user function): never rerouted.
        for buffer in ["ls", "git", "deploy"] {
            let result = gated
                .detect_input_type(
                    snapshot_with_first_token_described(buffer),
                    &agent_conversation_context(),
                )
                .await;
            assert_eq!(result.input_type, InputType::Shell, "{buffer:?}");
        }
        // Paths, assignments, keyworded or multi-word lines are not an "unknown single word".
        for buffer in [
            "./frobxone",
            "/bin/frobxone",
            "FOO=1",
            "~",
            "frobxone --help",
            "sudo",
        ] {
            let result = gated
                .detect_input_type(
                    snapshot_without_descriptions(buffer),
                    &agent_conversation_context(),
                )
                .await;
            assert_ne!(
                result.source,
                InputClassifierDecisionSource::UnknownSingleWordInAgentConversation,
                "{buffer:?}"
            );
        }
        // Outside a conversation, without a follow-up, or when the session cannot say whether
        // the word is a command, the classifier's Shell verdict stands.
        for context in [
            Context {
                in_active_agent_view: false,
                ..agent_conversation_context()
            },
            Context {
                is_agent_follow_up: false,
                ..agent_conversation_context()
            },
            Context {
                commands_fully_loaded: false,
                ..agent_conversation_context()
            },
        ] {
            let result = gated
                .detect_input_type(snapshot_without_descriptions("frobxone"), &context)
                .await;
            assert_eq!(result.input_type, InputType::Shell);
        }
    });
}

/// #935: `doas` is a privilege wrapper like `sudo`; on a host without it nothing describes the
/// head, and the one-off keyword must still keep the line out of the agent.
#[test]
fn test_gate_vetoes_ai_for_doas_wrapped_commands_without_any_session_data() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysAiClassifier);
        for buffer in ["doas grep this the file", "doas rg the src"] {
            for loaded in [true, false] {
                let result = gated
                    .detect_input_type(snapshot_without_descriptions(buffer), &context(loaded))
                    .await;
                assert_eq!(result.input_type, InputType::Shell, "{buffer:?}");
            }
        }
    });
}
