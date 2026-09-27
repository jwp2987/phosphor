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

fn context() -> Context {
    Context {
        current_input_type: InputType::AI,
        is_agent_follow_up: false,
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
            let result = gated.detect_input_type(input, &context()).await;
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

/// The override must not fire for input that merely lacks command evidence: an unknown binary,
/// a path, a leading environment assignment, or a real command the completer hasn't indexed yet
/// all have to stay Shell (i.e. the inner classifier's decision passes through unchanged).
#[test]
fn test_gate_does_not_override_commands_without_evidence() {
    futures::executor::block_on(async move {
        let gated = SafetyGatedClassifier::new(AlwaysShellClassifier);

        for buffer in [
            "mynewtool arg1 arg2",
            "./script.sh a b",
            "~/bin/x a b",
            "FOO=1 mycmd",
            // Simulates cargo not yet being indexed by `external_commands` (loads once per
            // session): no token_description anywhere, but "cargo" is lowercase, so it must not
            // be treated as ordinary English prose.
            "cargo --version",
            // A real command with a described flag but an unindexed lowercase command name.
            "rvm install 3.3",
        ] {
            let input = snapshot_without_descriptions(buffer);
            let result = gated.detect_input_type(input, &context()).await;
            assert_eq!(
                result.input_type,
                InputType::Shell,
                "expected {buffer:?} to stay Shell: no capitalized English word led the buffer"
            );
        }
    });
}

/// End-to-end sanity check with the real, shipped fallback classifier (not the fake): the two
/// observed exploit prompts must come out as AI, and ordinary shell usage must not.
///
/// `snapshot_without_descriptions` gives every token `token_description: None`, which is also
/// exactly what a completion context with no command knowledge at all produces —
/// `EmptyCompletionContext`, used for shared-session viewers
/// (`app/src/terminal/input/decorations.rs`'s `CompletionSessionContext::Empty` branch), and any
/// context before `external_commands` has finished loading for the session. The second loop below
/// is that surface: with zero command knowledge, real-looking lowercase commands must still stay
/// Shell, not get force-flipped to AI just because nothing described them.
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
                gated.detect_input_type(input, &context()).await.input_type,
                InputType::AI,
                "expected {buffer:?} to classify as AI end-to-end"
            );
        }

        for buffer in ["ls -la", "git status", "sudo apt update"] {
            let input = snapshot_without_descriptions(buffer);
            assert_eq!(
                gated.detect_input_type(input, &context()).await.input_type,
                InputType::Shell,
                "expected {buffer:?} to stay Shell end-to-end with no command knowledge available"
            );
        }
    });
}
