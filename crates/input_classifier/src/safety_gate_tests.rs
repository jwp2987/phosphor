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
