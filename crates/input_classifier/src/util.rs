use std::collections::HashSet;

use lazy_static::lazy_static;
use natural_language_detection::{check_if_token_has_shell_syntax, is_ordinary_english_word};
use warp_completer::ParsedTokensSnapshot;

/// The percentage of input tokens that can be described by our completion engine before
/// we consider the input as a shell command. This could be tuned.
const DETECT_AS_COMMAND_THRESHOLD: f32 = 0.5;

/// Threshold for the case when we have a low number of input tokens and require a higher
/// confidence level. This could be tuned.
const DETECT_AS_COMMAND_LOW_TOKEN_THRESHOLD: f32 = 0.7;

lazy_static! {
    /// One-off commands / keywords that should trigger a shell command classification.
    ///
    /// `claude`, `codex`, `gemini`, and `warp` are not actually _really_ one-off shell command
    /// keywords, but false-positive NL classifications for these inputs (where the user was trying
    /// to use claude code, codex CLI, gemini CLI, or the Warp Agent CLI) suck, because the user
    /// often thinks we're intentionally trying to push them away from those CLIs into Agent Mode,
    /// so we mitigate the risk by always treating as shell. `warp` is the command that launches the
    /// Warp Agent CLI / TUI, so bare `warp` (and `warp …` invocations) should classify as shell.
    static ref ONE_OFF_SHELL_COMMAND_KEYWORDS: HashSet<&'static str> = HashSet::from(["#", "echo", "man", "sudo", "claude", "codex", "gemini", "agy", "omp", "warp"]);

    static ref ONE_OFF_NATURAL_LANGUAGE_WORDS: HashSet<&'static str> = HashSet::from(["hello", "hi", "hey", "hola", "thanks", "explain", "yes", "no", "what", "nice", "1. "]);

    /// A set of words that should trigger an AI classification if they are the entire input
    /// and the input is a follow-up to an agent response.
    static ref AGENT_FOLLOW_UP_INPUTS: HashSet<&'static str> = HashSet::from(["yes", "continue", "do it", "approve"]);
}

pub fn is_agent_follow_up_input(input: &str) -> bool {
    AGENT_FOLLOW_UP_INPUTS.contains(input)
}

pub fn is_one_off_shell_command_keyword(word: &str) -> bool {
    ONE_OFF_SHELL_COMMAND_KEYWORDS.contains(word)
}

/// Returns true if the word is a one-off natural language word or a prefix of a one-off natural language word.
pub fn is_one_off_natural_language_word_or_prefix(word: &str) -> bool {
    is_one_off_natural_language_word(word) || is_prefix_of_natural_language_word(word)
}

// Returns true if the word is a one-off natural language word.
pub fn is_one_off_natural_language_word(word: &str) -> bool {
    ONE_OFF_NATURAL_LANGUAGE_WORDS.contains(word)
}

/// Checks if the input string is a prefix of any word in the ONE_OFF_NATURAL_LANGUAGE_WORDS set.
/// This helps with progressive typing detection to avoid mode flipping.
pub fn is_prefix_of_natural_language_word(input: &str) -> bool {
    // input is already lowercase from caller
    ONE_OFF_NATURAL_LANGUAGE_WORDS
        .iter()
        .any(|word| word.starts_with(input))
}

/// Shell-vs-natural-language heuristic. Two variants, selected at compile time:
/// - `nld_heuristic_v1`: uses `check_if_token_has_shell_syntax` and a threshold
///   that loosens with input length (the fork's historical behavior).
/// - `nld_heuristic_v2`: drops `check_if_token_has_shell_syntax` and pins the
///   threshold to 1 for all inputs, so a shell classification requires every
///   token to be a recognized command. Enabled for the TUI (matching Warp),
///   where it wins if both features are set.
pub async fn is_likely_shell_command(
    input: &ParsedTokensSnapshot,
    word_tokens_count: usize,
) -> bool {
    const YIELD_BATCH_SIZE: usize = 5;
    let use_nld_heuristic_v2 = cfg!(feature = "nld_heuristic_v2");

    let mut likely_command_token_count = 0;
    let total_token_count = input.parsed_tokens.len();
    let mut is_first_token_command = false;
    for (idx, token) in input.parsed_tokens.iter().enumerate() {
        // Periodically, yield to the executor so this task can be aborted if
        // requested.
        if idx % YIELD_BATCH_SIZE == 0 {
            futures_lite::future::yield_now().await;
        }
        // Early return if the very first token of the whole buffer is a one-off command /
        // keyword. `token.token_index` is relative to *its own* parsed command and resets to 0
        // for every `;` / `&&` / `||` / newline-separated command in the buffer, so checking it
        // alone would fire this shortcut for a one-off keyword anywhere a later command starts
        // (e.g. "Run exactly this: sleep 1; echo hi" hits it on "echo"), classifying an entire
        // English sentence as Shell just because of where a keyword happened to land. `idx == 0`
        // is the true first token of the buffer, which is the only position this allowlist is
        // meant to gate on.
        if idx == 0 && ONE_OFF_SHELL_COMMAND_KEYWORDS.contains(&token.token.as_str()) {
            return true;
        }

        let check_if_token_has_shell_syntax =
            !use_nld_heuristic_v2 && check_if_token_has_shell_syntax(token.token.as_str());
        if token.token_description.is_some() || check_if_token_has_shell_syntax {
            likely_command_token_count += 1;
        }

        if idx == 0 {
            is_first_token_command = token.token_description.is_some();
        }
    }

    // When token count is lower than 2, we should make sure all tokens
    // are matching the target classification category.
    let command_threshold = if use_nld_heuristic_v2 || total_token_count <= 2 {
        1.0
    } else if total_token_count <= 4 {
        DETECT_AS_COMMAND_LOW_TOKEN_THRESHOLD
    } else {
        DETECT_AS_COMMAND_THRESHOLD
    };

    // Classify as shell if:
    // 1) We hit significant threshold of likely shell command tokens.
    // 2) When there are fewer than 3 tokens, the first token is a valid top-level command.
    if likely_command_token_count >= (total_token_count as f32 * command_threshold) as usize
        || (word_tokens_count < 3 && is_first_token_command)
    {
        return true;
    }

    false
}

/// Whether the text contains CJK / kana / Korean / fullwidth characters. A hit
/// is treated as non-English natural-language input and classified directly as
/// AI (since the dictionary and ML model are both English, with no CJK training samples).
pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            c,
            '\u{4E00}'..='\u{9FFF}'   // CJK unified ideographs
            | '\u{3400}'..='\u{4DBF}' // CJK extension A
            | '\u{3040}'..='\u{309F}' // Hiragana
            | '\u{30A0}'..='\u{30FF}' // Katakana
            | '\u{AC00}'..='\u{D7AF}' // Hangul syllables
            | '\u{3000}'..='\u{303F}' // CJK punctuation (。、!?, etc.)
            | '\u{FF00}'..='\u{FFEF}' // Halfwidth/fullwidth forms (。、!?, etc.)
        )
    })
}

/// Returns true if the first token is a command that is installed on the system.
pub fn is_installed_binary(input: &ParsedTokensSnapshot) -> bool {
    input
        .parsed_tokens
        .first()
        .map(|token| token.token_description.is_some())
        .unwrap_or(false)
}

/// True iff `token` is a leading `NAME=value` environment assignment (`FOO=1`, `PAGER=less`),
/// which carries no command evidence of its own — the word that would actually run is whatever
/// comes after it (`env`/`export`-style invocations).
fn is_env_assignment_token(token: &str) -> bool {
    match token.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.chars().enumerate().all(|(i, c)| {
                    c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit())
                })
        }
        None => false,
    }
}

/// True iff `token` looks like a path the user meant to execute or reference directly
/// (`./script.sh`, `~/bin/x`, `/usr/bin/foo`) rather than a plain word. The completer may have no
/// `token_description` for a script that isn't itself a registered command (or when path
/// completion isn't available in this context), but a path is unambiguous evidence of intent, not
/// natural-language prose.
fn is_path_like_token(token: &str) -> bool {
    token.contains('/') || token.starts_with('.') || token.starts_with('~')
}

/// Returns true iff a Shell classification should be overridden back to AI for this buffer.
///
/// This is intentionally narrow: it only fires when the buffer's *effective* first token (the
/// first token that isn't a leading `NAME=value` environment assignment) both (a) has no evidence
/// of being a real command — no `token_description` from the completer, not a one-off shell
/// keyword, and not path-like — **and** (b) is itself an ordinary, capitalized English dictionary
/// word (e.g. "Run", "Then", "Please", "Delete"). Capitalization matters: genuine shell
/// invocations are essentially always typed lowercase, while a capitalized word starting an
/// English sentence is not.
///
/// Deliberately does *not* fire just because a token has no command evidence: the completer may
/// simply not have indexed a real, lowercase command yet (`external_commands` loads once per
/// session), or the word may be an unknown binary, a path, or a shell function/alias this
/// particular completion context can't see (e.g. `EmptyCompletionContext` for shared-session
/// viewers). None of those are natural-language prose, so none of them should be overridden.
pub fn first_token_forces_ai_override(input: &ParsedTokensSnapshot) -> bool {
    let mut tokens = input.parsed_tokens.iter();
    let Some(mut token) = tokens.next() else {
        return false;
    };
    while is_env_assignment_token(token.token.as_str()) {
        let Some(next) = tokens.next() else {
            return false;
        };
        token = next;
    }

    let word = token.token.as_str();
    if token.token_description.is_some()
        || is_one_off_shell_command_keyword(word)
        || is_path_like_token(word)
    {
        return false;
    }

    word.chars().next().is_some_and(|c| c.is_uppercase()) && is_ordinary_english_word(word)
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
