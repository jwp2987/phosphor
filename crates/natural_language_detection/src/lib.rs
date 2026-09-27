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
