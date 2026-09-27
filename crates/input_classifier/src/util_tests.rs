use warp_completer::util::parse_current_commands_and_tokens;
use warp_completer::ParsedTokensSnapshot;

use super::*;
use crate::test_utils::CompletionContext;

// The shell-command heuristic tests are paired: a `_for_nld_heuristic_v1`
// variant (gated on v1 with v2 off) and a `_for_nld_heuristic_v2` variant
// (gated on v2). Both `is_likely_shell_command` code paths are restored, so the
// scenarios where the stricter v2 heuristic disagrees with v1 (shell-syntax
// voting, the below-threshold described-token majority, the log-path prompt)
// each assert the opposite outcome under v2. `nld_heuristic_v1` is a default
// feature, so `cargo test -p input_classifier` runs the v1 variants; run with
// `--features nld_heuristic_v2` for the v2 variants.
//
// The one-off shell keyword set previously dropped "agy" and "omp" from
// `ONE_OFF_SHELL_COMMAND_KEYWORDS` (GitHub issue #19); they have been
// restored to match Warp's oracle list.

async fn mock_parsed_input_token(buffer_text: String) -> ParsedTokensSnapshot {
    warp_features::mark_initialized();
    let completion_context = CompletionContext::new();
    parse_current_commands_and_tokens(buffer_text, &completion_context).await
}

fn clear_all_token_descriptions(snapshot: &mut ParsedTokensSnapshot) {
    for token in snapshot.parsed_tokens.iter_mut() {
        token.token_description = None;
    }
}

async fn one_off_keyword_short_circuits() {
    let mut token = mock_parsed_input_token("sudo apt update".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    assert!(is_likely_shell_command(&token, word_tokens_count).await);

    let mut token = mock_parsed_input_token("echo hello world".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    assert!(is_likely_shell_command(&token, word_tokens_count).await);

    let mut token = mock_parsed_input_token("agy doctor".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    assert!(is_likely_shell_command(&token, word_tokens_count).await);

    let mut token = mock_parsed_input_token("omp --help".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    assert!(is_likely_shell_command(&token, word_tokens_count).await);

    let mut token = mock_parsed_input_token("warp agent run".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    assert!(is_likely_shell_command(&token, word_tokens_count).await);
}

async fn first_token_with_description_short_input_is_shell() {
    let token = mock_parsed_input_token("cargo --version".to_string()).await;
    assert!(is_likely_shell_command(&token, 2).await);
}

async fn no_descriptions_returns_false() {
    let mut token = mock_parsed_input_token("install --foo=bar baz".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    assert!(!is_likely_shell_command(&token, word_tokens_count).await);
}

async fn shell_syntax_tokens_with_only_first_token_description() -> bool {
    let mut token = mock_parsed_input_token("git --foo=bar /path/to/file --baz".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();

    for (idx, token) in token.parsed_tokens.iter_mut().enumerate() {
        if idx != 0 {
            token.token_description = None;
        }
    }

    assert!(word_tokens_count >= 3);
    is_likely_shell_command(&token, word_tokens_count).await
}

async fn described_token_majority_below_v2_threshold() -> bool {
    let mut token = mock_parsed_input_token("cargo build --release --workspace".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    assert!(word_tokens_count >= 3);

    let description = token
        .parsed_tokens
        .iter()
        .find_map(|token| token.token_description.clone())
        .expect("test input should include at least one described token");
    for token in token.parsed_tokens.iter_mut() {
        token.token_description = Some(description.clone());
    }
    token
        .parsed_tokens
        .last_mut()
        .expect("test input should include tokens")
        .token_description = None;

    is_likely_shell_command(&token, word_tokens_count).await
}

async fn downloads_log_path_in_nl_prompt_is_shell() -> bool {
    let command_token = mock_parsed_input_token("cargo --version".to_string()).await;
    let command_description = command_token
        .parsed_tokens
        .first()
        .and_then(|token| token.token_description.clone())
        .expect("test input should include a described command token");

    let mut token = mock_parsed_input_token(
        "look at this /users/ewanlockwood/downloads/logs_58498936986".to_string(),
    )
    .await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    token
        .parsed_tokens
        .first_mut()
        .expect("test input should include tokens")
        .token_description = Some(command_description);
    is_likely_shell_command(&token, word_tokens_count).await
}

async fn file_path_in_nl_prompt_is_shell() -> bool {
    let mut token =
        mock_parsed_input_token("look at this /users/foo/bar.log file".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    is_likely_shell_command(&token, word_tokens_count).await
}

async fn majority_described_tokens_returns_true() {
    let token =
        mock_parsed_input_token("cargo build --release --workspace --all-features".to_string())
            .await;
    let word_tokens_count = token.parsed_tokens.len();
    assert!(is_likely_shell_command(&token, word_tokens_count).await);
}

// Cases where nld_heuristic_v1 and nld_heuristic_v2 should both mark input as shell.
#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_one_off_keyword_short_circuits_true_for_nld_heuristic_v1() {
    futures::executor::block_on(one_off_keyword_short_circuits());
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_one_off_keyword_short_circuits_true_for_nld_heuristic_v2() {
    futures::executor::block_on(one_off_keyword_short_circuits());
}

#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_first_token_with_description_short_input_true_for_nld_heuristic_v1()
{
    futures::executor::block_on(first_token_with_description_short_input_is_shell());
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_first_token_with_description_short_input_true_for_nld_heuristic_v2()
{
    futures::executor::block_on(first_token_with_description_short_input_is_shell());
}

#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_majority_described_tokens_true_for_nld_heuristic_v1() {
    futures::executor::block_on(majority_described_tokens_returns_true());
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_majority_described_tokens_true_for_nld_heuristic_v2() {
    futures::executor::block_on(majority_described_tokens_returns_true());
}

// Cases where nld_heuristic_v1 and nld_heuristic_v2 should both not mark input as shell.
#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_no_descriptions_false_for_nld_heuristic_v1() {
    futures::executor::block_on(no_descriptions_returns_false());
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_no_descriptions_false_for_nld_heuristic_v2() {
    futures::executor::block_on(no_descriptions_returns_false());
}

#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_file_path_in_nl_prompt_false_for_nld_heuristic_v1() {
    futures::executor::block_on(async move {
        assert!(!file_path_in_nl_prompt_is_shell().await);
    });
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_file_path_in_nl_prompt_false_for_nld_heuristic_v2() {
    futures::executor::block_on(async move {
        assert!(!file_path_in_nl_prompt_is_shell().await);
    });
}

// Cases where nld_heuristic_v1 marks input as shell and the stricter
// nld_heuristic_v2 does not.
#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_shell_syntax_votes_true_for_nld_heuristic_v1() {
    futures::executor::block_on(async move {
        assert!(shell_syntax_tokens_with_only_first_token_description().await);
    });
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_shell_syntax_does_not_vote_false_for_nld_heuristic_v2() {
    futures::executor::block_on(async move {
        assert!(!shell_syntax_tokens_with_only_first_token_description().await);
    });
}

#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_described_token_majority_true_for_nld_heuristic_v1() {
    futures::executor::block_on(async move {
        assert!(described_token_majority_below_v2_threshold().await);
    });
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_described_token_majority_false_for_nld_heuristic_v2() {
    futures::executor::block_on(async move {
        assert!(!described_token_majority_below_v2_threshold().await);
    });
}

#[cfg(all(feature = "nld_heuristic_v1", not(feature = "nld_heuristic_v2")))]
#[test]
fn test_is_likely_shell_command_downloads_log_path_true_for_nld_heuristic_v1() {
    futures::executor::block_on(async move {
        assert!(downloads_log_path_in_nl_prompt_is_shell().await);
    });
}

#[cfg(feature = "nld_heuristic_v2")]
#[test]
fn test_is_likely_shell_command_downloads_log_path_false_for_nld_heuristic_v2() {
    futures::executor::block_on(async move {
        assert!(!downloads_log_path_in_nl_prompt_is_shell().await);
    });
}

// Regression test for #696: `token.token_index` is relative to its own parsed command and
// resets to 0 for every `;` / `&&` / `||` / newline-separated command in the buffer, so checking
// only `token.token_index == 0` for the one-off keyword allowlist fired for a keyword like `echo`
// landing at the start of a *later* command, classifying an entire English sentence as Shell
// purely because of where the keyword happened to land (e.g. "Run exactly this: sleep 8; echo
// hi" was sent to bash as two separate commands). Only the true first token of the whole buffer
// may trigger that allowlist. This test is not feature-gated: the one-off keyword check runs
// unconditionally, before the nld_heuristic_v1 / v2 split.
async fn one_off_keyword_after_semicolon_does_not_short_circuit() -> bool {
    let mut token = mock_parsed_input_token("Run exactly this: sleep 8; echo hi".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    is_likely_shell_command(&token, word_tokens_count).await
}

#[test]
fn test_is_likely_shell_command_one_off_keyword_after_semicolon_is_not_shell() {
    futures::executor::block_on(async move {
        assert!(!one_off_keyword_after_semicolon_does_not_short_circuit().await);
    });
}

// The allowlist must still fire when the keyword genuinely is the first token of the buffer,
// even when other commands with their own token_index 0 follow it.
async fn one_off_keyword_at_true_start_still_short_circuits() -> bool {
    let mut token = mock_parsed_input_token("echo hi; ls -la".to_string()).await;
    let word_tokens_count = token.parsed_tokens.len();
    clear_all_token_descriptions(&mut token);
    is_likely_shell_command(&token, word_tokens_count).await
}

#[test]
fn test_is_likely_shell_command_one_off_keyword_at_true_start_is_shell() {
    futures::executor::block_on(async move {
        assert!(one_off_keyword_at_true_start_still_short_circuits().await);
    });
}

// Regression tests for #696's second round: `first_token_forces_ai_override` (used by
// `SafetyGatedClassifier`) must fire only for an ordinary-English first word (matched
// case-insensitively) with no command evidence — never merely because a word lacks a
// `token_description`, since that can just mean the completer hasn't indexed a real command yet
// (`external_commands` loads once per session) or can't see it at all (`EmptyCompletionContext`
// for shared-session viewers). `commands_fully_loaded` distinguishes those two cases; see
// `first_token_forces_ai_override`'s doc comment for the full rule.
async fn first_token_forces_ai_override_for(buffer: &str, commands_fully_loaded: bool) -> bool {
    let mut token = mock_parsed_input_token(buffer.to_string()).await;
    clear_all_token_descriptions(&mut token);
    first_token_forces_ai_override(&token, commands_fully_loaded)
}

#[test]
fn test_first_token_forces_ai_override_for_english_words_when_commands_fully_loaded() {
    futures::executor::block_on(async move {
        // The two observed exploit prompts (#696), and their lowercase forms: the shipped ONNX
        // classifier's tokenizer lowercases its input before scoring, so both forms must be
        // caught identically once the command index is known to be complete.
        for buffer in [
            "Run exactly this: sleep 8; echo hi",
            "run exactly this: sleep 8; echo hi",
            "Then run: echo hi",
            "then run: echo hi",
        ] {
            assert!(
                first_token_forces_ai_override_for(buffer, true).await,
                "expected {buffer:?} to be overridden with the command index fully loaded"
            );
        }
    });
}

#[test]
fn test_first_token_forces_ai_override_via_prose_shape_when_commands_not_loaded() {
    futures::executor::block_on(async move {
        // Same buffers, but simulating a command index that hasn't finished loading: dictionary
        // membership alone isn't trusted, but each buffer still has a later token ending in ':'
        // before its first shell operator, which is enough on its own.
        for buffer in [
            "Run exactly this: sleep 8; echo hi",
            "run exactly this: sleep 8; echo hi",
            "Then run: echo hi",
            "then run: echo hi",
        ] {
            assert!(
                first_token_forces_ai_override_for(buffer, false).await,
                "expected {buffer:?} to be overridden via prose shape with the command index not \
                 yet loaded"
            );
        }

        // No colon anywhere: not enough evidence when the index isn't known to be complete.
        assert!(!first_token_forces_ai_override_for("Then delete the temp directory", false).await);
    });
}

#[test]
fn test_first_token_forces_ai_override_does_not_fire_when_commands_not_loaded() {
    futures::executor::block_on(async move {
        // Unknown / not-yet-indexed lowercase commands: no evidence, and (with the index not
        // known to be complete) no trustworthy prose signal either.
        assert!(!first_token_forces_ai_override_for("mynewtool arg1 arg2", false).await);
        assert!(!first_token_forces_ai_override_for("cargo --version", false).await);
        assert!(!first_token_forces_ai_override_for("rvm install 3.3", false).await);
        // Path-like tokens are evidence of intent even with no token_description.
        assert!(!first_token_forces_ai_override_for("./script.sh a b", false).await);
        assert!(!first_token_forces_ai_override_for("~/bin/x a b", false).await);
        // A leading `NAME=value` assignment carries no evidence itself; the word after it does,
        // and here that word is an unindexed lowercase command, not English prose.
        assert!(!first_token_forces_ai_override_for("FOO=1 mycmd", false).await);
        // One-off shell keywords are evidence even with no token_description.
        assert!(!first_token_forces_ai_override_for("sudo apt update", false).await);
    });
}

#[test]
fn test_first_token_forces_ai_override_does_not_fire_for_non_dictionary_words_even_when_loaded() {
    futures::executor::block_on(async move {
        // Even with a fully-loaded index, a word that isn't in the English dictionary at all
        // (an unknown tool name, an acronym) is never treated as prose.
        assert!(!first_token_forces_ai_override_for("mynewtool arg1 arg2", true).await);
        assert!(!first_token_forces_ai_override_for("rvm install 3.3", true).await);
    });
}

#[test]
fn test_first_token_forces_ai_override_fires_for_unindexed_english_word_when_loaded() {
    futures::executor::block_on(async move {
        // "cargo" is an ordinary English dictionary word; with the index fully loaded and no
        // evidence for it, that is sufficient on its own -- no colon or other shape required.
        assert!(first_token_forces_ai_override_for("cargo --version", true).await);
    });
}

#[test]
fn test_first_token_forces_ai_override_prose_colon_shape_does_not_match_realistic_commands() {
    futures::executor::block_on(async move {
        for buffer in ["docker run -p 80:80", "scp host:path .", "echo a: b"] {
            assert!(
                !first_token_forces_ai_override_for(buffer, false).await,
                "expected {buffer:?} to not be overridden by the prose colon shape"
            );
        }
    });
}

#[test]
fn test_is_agent_follow_up_input() {
    for input in ["yes", "continue", "do it", "approve"] {
        assert!(
            is_agent_follow_up_input(input),
            "expected {input:?} to be recognized as an agent follow-up input"
        );
    }
    assert!(!is_agent_follow_up_input("no"));
    assert!(!is_agent_follow_up_input("approved"));
}
