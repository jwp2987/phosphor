use std::borrow::Cow;

use lazy_static::lazy_static;
use regex::Regex;
use rust_stemmers::{Algorithm, Stemmer};
use word_list::{COMMAND_LIST, STACK_OVERFLOW_LIST, WORD_LIST};

mod word_list;

lazy_static! {
    /// Regex for matching contractions to reduce them into root forms. E.g. he's => he, mustn't => must.
    static ref CONTRACTION_REGEX: Regex = Regex::new(r"('s|'re|n't|'t|'m|'ve|'ll)$")
        .expect("End of word punctuation regex should be parsed");
}

const RESERVED_KEYWORDS: [&str; 1] = ["what"];

#[derive(PartialEq, Eq)]
pub enum WordDb {
    English,
    StackOverflow,
    Command,
}

pub fn is_word(word: &str, db: WordDb) -> bool {
    match db {
        WordDb::English => WORD_LIST.contains(word),
        WordDb::StackOverflow => STACK_OVERFLOW_LIST.contains(word),
        WordDb::Command => COMMAND_LIST.contains(word),
    }
}

/// Calculate the NL score for a vector of words: the number of tokens recognized as English,
/// StackOverflow, or command-dictionary words. An unrecognized token — including a bare shell
/// metacharacter or a quoted string — is neutral and neither adds to nor subtracts from the
/// score, since a metacharacter appearing anywhere in an otherwise-English sentence must not be
/// treated as stronger evidence of Shell than an ordinary unrecognized word would be.
pub fn natural_language_words_score(words: Vec<Cow<str>>, is_first_token_command: bool) -> usize {
    let en_stemmer = Stemmer::create(Algorithm::English);
    let mut natural_language_token_count: usize = 0;

    for (i, token) in words.into_iter().enumerate() {
        let token = token_preprocessing(&token);

        if i == 0
            && (is_word(&token, WordDb::Command)
                || (is_first_token_command && !RESERVED_KEYWORDS.contains(&token.as_str())))
        {
            // If the first word is a command, it is possible user want to run this command. We should skip
            // this token.
            continue;
        }

        if is_word(&token, WordDb::StackOverflow) || is_word(&token, WordDb::Command) {
            natural_language_token_count += 1;
        } else {
            let stemmed_word = en_stemmer.stem(&token);

            if is_word(&stemmed_word, WordDb::English)
                || is_word(&stemmed_word, WordDb::StackOverflow)
                || is_word(&stemmed_word, WordDb::Command)
            {
                natural_language_token_count += 1;
            } else if wrapped_in_quotes(&token) || check_if_token_has_shell_syntax(&token) {
                // A quoted string or a bare shell metacharacter (';', '>', '~', ...) is neither a
                // natural-language nor a shell-command signal on its own, so it must not move the
                // score in either direction. This used to subtract from the count, which meant an
                // English sentence containing an unquoted redirect or path (e.g. "... > ~/file")
                // was scored *more* shell-like than one containing an ordinary unrecognized word,
                // letting a bare metacharacter alone flip the whole sentence to Shell.
            }
        }
    }

    natural_language_token_count
}

/// Returns true iff `word` is a recognized ordinary English dictionary word, after the same
/// lowercasing / contraction-expansion / English-stemming preprocessing
/// `natural_language_words_score` applies. Deliberately consults only the English word list, not
/// the command or StackOverflow dictionaries: this is used to decide whether a word an agent
/// classifier flagged as Shell is unmistakably English prose (e.g. "delete", "please"), so a word
/// that is *also* a real or plausible tool/tag name should not qualify.
pub fn is_ordinary_english_word(word: &str) -> bool {
    let en_stemmer = Stemmer::create(Algorithm::English);
    let preprocessed = token_preprocessing(word);
    is_word(&en_stemmer.stem(&preprocessed), WordDb::English)
}

pub fn check_if_token_has_shell_syntax(word: &str) -> bool {
    // List of special characters from https://mywiki.wooledge.org/BashGuide/SpecialCharacters.
    // Note that here we check if the word contains whitespace first to make sure we are running
    // on a single token.
    !word.contains(' ')
        && word.contains([
            '$', '=', '{', '}', '[', ']', '>', '<', '*', '~', '&', '(', ')', '|', '/', '-',
        ])
}

fn wrapped_in_quotes(word: &str) -> bool {
    (word.starts_with('"') && word.ends_with('"'))
        || (word.starts_with('\'') && word.ends_with('\''))
}

/// Pre-process a token so it's ready for checking against dictionary. This includes:
/// 1) Convert to lowercase
/// 2) Expand contraction
fn token_preprocessing(token: &str) -> String {
    let mut token = token.to_lowercase();

    // "can" is a special case in the contraction matching logic.
    if token == "can't" {
        return "can".to_string();
    }

    if let Some(captures) = CONTRACTION_REGEX.captures(&token) {
        let contraction_capture = captures
            .get(1)
            .map(|number| number.as_str().len())
            .unwrap_or(0);

        token.truncate(token.len() - contraction_capture);
    }

    token
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use super::*;

    /// #965: `fish`/`ksh` are real binaries but also Stack Overflow tags / English words. They are
    /// deliberately NOT in the overlap list: a leading one is skipped as a command only when the
    /// caller says the program is installed (or a known builtin/function/alias), so
    /// `fish is great` stays prose when fish is absent.
    #[test]
    fn leading_binary_name_is_a_command_only_when_installed() {
        for word in ["fish", "ksh"] {
            assert!(!is_word(word, WordDb::Command), "{word}");
            let line = || {
                vec![
                    Cow::Borrowed(word),
                    Cow::Borrowed("is"),
                    Cow::Borrowed("great"),
                ]
            };
            assert_eq!(natural_language_words_score(line(), false), 3, "{word}");
            assert_eq!(natural_language_words_score(line(), true), 2, "{word}");
        }
    }

    /// Prose words must stay out of the overlap list (a first-word hit is skipped when scoring).
    #[test]
    fn overlap_list_excludes_sentence_openers() {
        for word in [
            "file", "date", "set", "wait", "return", "history", "cut", "patch", "ruby", "swift",
            "java", "fish", "ksh",
        ] {
            assert!(!is_word(word, WordDb::Command), "{word}");
        }
    }

    /// #964: `swift vs kotlin?` is prose; `vs` must score as an ordinary word so a three-word
    /// comparison of tool names is not left one point short (thresholds are 1.0 up to 3 tokens).
    #[test]
    fn vs_is_an_ordinary_word() {
        assert!(is_word("vs", WordDb::English));
        assert_eq!(
            natural_language_words_score(
                vec![
                    Cow::Borrowed("swift"),
                    Cow::Borrowed("vs"),
                    Cow::Borrowed("kotlin")
                ],
                false
            ),
            3
        );
    }
}
