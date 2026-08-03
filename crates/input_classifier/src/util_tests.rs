use warp_completer::util::parse_current_commands_and_tokens;
use warp_completer::ParsedTokensSnapshot;

use super::*;
use crate::test_utils::CompletionContext;

// NOTE: This fork's `crates/input_classifier/src/util.rs` no longer has the
// `nld_heuristic_v1` / `nld_heuristic_v2` cargo features that Warp's
// `util_tests.rs` gates each test variant behind (see `Cargo.toml`: neither
// feature is declared here). The `is_likely_shell_command` product code was
// simplified to always follow the old `nld_heuristic_v1` code path (the
// `use_nld_heuristic_v2` branches were deleted outright, not merely
// feature-gated out), so only the Warp tests gated on
// `nld_heuristic_v1` are meaningful here; the `nld_heuristic_v2`-gated
// variants test a heuristic that no longer exists in this fork and are not
// ported (see PORT task report, NEEDS-ADAPTATION).
//
// The one-off shell keyword set was also trimmed in this fork
// (`ONE_OFF_SHELL_COMMAND_KEYWORDS` dropped "agy" and "omp"), so
// `test_is_likely_shell_command_one_off_keyword_short_circuits` — which
// exercises those two words as part of the same helper as "sudo"/"echo" — is
// ported unmodified from Warp and is expected to fail on the "agy"/"omp"
// cases; see PORT task report (FOUND REGRESSION).

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

#[test]
fn test_is_likely_shell_command_one_off_keyword_short_circuits() {
    futures::executor::block_on(one_off_keyword_short_circuits());
}

#[test]
fn test_is_likely_shell_command_first_token_with_description_short_input_true() {
    futures::executor::block_on(first_token_with_description_short_input_is_shell());
}

#[test]
fn test_is_likely_shell_command_majority_described_tokens_true() {
    futures::executor::block_on(majority_described_tokens_returns_true());
}

#[test]
fn test_is_likely_shell_command_no_descriptions_false() {
    futures::executor::block_on(no_descriptions_returns_false());
}

#[test]
fn test_is_likely_shell_command_file_path_in_nl_prompt_false() {
    futures::executor::block_on(async move {
        assert!(!file_path_in_nl_prompt_is_shell().await);
    });
}

#[test]
fn test_is_likely_shell_command_shell_syntax_votes_true() {
    futures::executor::block_on(async move {
        assert!(shell_syntax_tokens_with_only_first_token_description().await);
    });
}

#[test]
fn test_is_likely_shell_command_described_token_majority_true() {
    futures::executor::block_on(async move {
        assert!(described_token_majority_below_v2_threshold().await);
    });
}

#[test]
fn test_is_likely_shell_command_downloads_log_path_true() {
    futures::executor::block_on(async move {
        assert!(downloads_log_path_in_nl_prompt_is_shell().await);
    });
}
