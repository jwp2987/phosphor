//! Shell-accurate enumeration of every command a command line would execute, for policy
//! decisions (the Agent Mode command denylist, `app/src/ai/blocklist/permissions.rs`).
//!
//! # Why this is not the completer parser
//!
//! [`super::decompose_command`] and friends serve command x-ray, error underlining and the
//! allowlist. They slice the source by span and return text *as typed*, and their
//! tokenisation is tuned for completion rather than for shell semantics: a redirect operator
//! is absorbed rather than delimiting a word, `{` always opens a group, and `then`/`do` are
//! ordinary command names. That is fine for display, and wrong for a security decision —
//! `>/dev/null rm -rf ~`, `rm>/dev/null -rf ~`, `{rm,-rf,~}` and
//! `if true; then rm -rf ~; fi` all run `rm`, and none of them surfaces `rm` as a command
//! name there (#678).
//!
//! Changing that tokeniser would change what x-ray shows and what the allowlist matches, and
//! widening an allowlist match is the unsafe direction. So this is a separate, purely additive
//! analysis with one job: name every command word the shell would execute, after quote
//! removal and brace expansion, and say so when it cannot.
//!
//! # Fail-closed contract
//!
//! [`ExecutedCommands::unresolved`] is non-empty whenever some executed command word cannot be
//! determined without running the shell — it comes from a parameter expansion or command
//! substitution (`$CMD`, `$(which rm)`), a glob (`/bin/r?`), history expansion (`!rm`), an
//! `eval`/`sh -c` string that is itself dynamic, or input the analysis could not parse
//! (unterminated quotes, unbalanced grouping, unfinished compound commands). A policy caller
//! must treat a non-empty `unresolved` as "cannot vouch for this command" and require
//! confirmation, never as "no match".
//!
//! # What it understands
//!
//! POSIX/bash word syntax (single, double and `$'…'` ANSI-C quoting with escape decoding,
//! backslash escapes, line continuations), redirections anywhere in a command including
//! glued (`rm>x`), leading (`>x rm`), fd-prefixed (`2>x`, `{fd}>x`) and here-documents,
//! `;` `&` `&&` `||` `|` `|&` and newlines, subshells `( … )`, groups `{ …; }`, `$( … )` and
//! backticks (recursively, including inside double quotes and unquoted here-documents),
//! process substitution `<( … )`, arithmetic `(( … ))`/`$(( … ))`, `${ … }`, the compound
//! commands `if`/`while`/`until`/`for`/`select`/`case`/`[[ … ]]` and function definitions,
//! leading assignments (including bash arrays `FOO=(a b)`), brace expansion (lists and
//! sequences), and precommand wrappers (`sudo`, `env`, `command`, `exec`, `nice`, `nohup`,
//! `timeout`, `xargs`, `stdbuf`, `time`, …, plus `eval`, `sh -c`, `find -exec`, `alias`,
//! `trap` and `watch`, whose arguments are themselves commands).
//!
//! PowerShell (`EscapeChar::Backtick`) is approximated with the same grammar and the backtick
//! as the escape character. That can only over-report — extra command candidates, or a
//! construct reported unresolved — which is the safe direction for a policy caller.

use warp_util::path::EscapeChar;

/// How deeply nested `$( … )`/backtick/`eval`/`sh -c` analysis may recurse before the result
/// is reported unresolved.
const MAX_DEPTH: usize = 16;

/// How deeply `( … )` / `$( … )` / `<( … )` may nest within one command line before the
/// result is reported unresolved. Bounds recursion on adversarial input.
const MAX_NESTING: usize = 64;

/// The most words a single brace expansion may produce before the result is reported
/// unresolved rather than enumerated.
const MAX_BRACE_EXPANSION: usize = 256;

/// The most brace groups a single word may contain before it is reported unresolved rather
/// than expanded. Bounds recursion on adversarial input.
const MAX_BRACE_GROUPS: usize = 32;

/// The most suffixes offered as candidates when a wrapper's options cannot be parsed.
const MAX_WRAPPER_SUFFIXES: usize = 16;

/// Every simple command a command line would execute, as far as can be decided statically.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ExecutedCommands {
    /// Each executed simple command as its words after quote removal and brace expansion,
    /// command word first, with leading assignments and all redirections removed.
    ///
    /// A command reached through a wrapper appears twice: once whole (`sudo rm -rf ~`) and
    /// once from the inner command word (`rm -rf ~`). A dynamic word is rendered as its
    /// source text (`$(which rm)`).
    pub commands: Vec<Vec<String>>,

    /// Why some executed command word could not be determined. Empty when every command word
    /// was resolved; see the module docs for the fail-closed contract.
    pub unresolved: Vec<String>,
}

impl ExecutedCommands {
    /// Whether every command word the line executes was determined statically.
    pub fn is_fully_resolved(&self) -> bool {
        self.unresolved.is_empty()
    }

    /// Every spelling of every executed command that a command-text policy should match:
    /// each command's words joined by single spaces, plus — when the command word carries a
    /// path (`/bin/rm`, `./rm`) or zsh's `=rm` prefix — the same with the bare program name.
    pub fn policy_spellings(&self) -> Vec<String> {
        let mut spellings: Vec<String> = Vec::new();
        let mut push = |spelling: String| {
            if !spelling.is_empty() && !spellings.contains(&spelling) {
                spellings.push(spelling);
            }
        };
        for words in &self.commands {
            let Some(first) = words.first() else {
                continue;
            };
            push(words.join(" "));
            let bare = program_name(first);
            if bare != first.as_str() && !bare.is_empty() {
                let mut bare_words = words.clone();
                bare_words[0] = bare.to_string();
                push(bare_words.join(" "));
            }
        }
        spellings
    }
}

/// Enumerates every command `source` would execute. See the module docs.
pub fn executed_commands(source: &str, escape_char: EscapeChar) -> ExecutedCommands {
    let mut out = ExecutedCommands::default();
    if matches!(escape_char, EscapeChar::Backslash) && source.trim_start().starts_with('^') {
        // bash/zsh quick substitution (`^old^new`) re-runs an edited history entry.
        out.unresolved
            .push("history quick substitution (`^old^new`)".to_string());
    }
    analyze(source, escape_char, 0, &mut out);
    out
}

/// The program a command word names: the last path component, without zsh's `=cmd` prefix.
fn program_name(word: &str) -> &str {
    let base = word.rsplit('/').next().unwrap_or(word);
    base.strip_prefix('=').unwrap_or(base)
}

fn analyze(source: &str, escape_char: EscapeChar, depth: usize, out: &mut ExecutedCommands) {
    if depth > MAX_DEPTH {
        out.unresolved
            .push("command nesting is too deep to analyse".to_string());
        return;
    }
    let mut analyzer = Analyzer::new(source, escape_char, depth, out);
    analyzer.parse_list(Term::Eof);
}

/// One character of a word, and whether quoting or escaping protected it from brace
/// expansion, globbing and keyword/assignment recognition.
#[derive(Clone, Copy, Debug)]
struct Seg {
    ch: char,
    quoted: bool,
    /// Part of the source text standing in for an expansion whose value is unknown.
    dynamic: bool,
}

impl Seg {
    fn literal(ch: char, quoted: bool) -> Self {
        Self {
            ch,
            quoted,
            dynamic: false,
        }
    }
}

#[derive(Default, Debug)]
struct Word {
    segs: Vec<Seg>,
    /// Contains an expansion whose value is only known at run time.
    dynamic: bool,
    /// Contains an *unquoted* expansion, whose value is also word-split, so it can supply
    /// any number of words — including the command word — whatever surrounds it.
    unquoted_expansion: bool,
    /// Contains an unquoted glob that the shell may expand against the filesystem.
    glob: bool,
    /// Any part of the word was quoted or escaped.
    any_quoted: bool,
}

impl Word {
    fn push(&mut self, ch: char, quoted: bool) {
        self.any_quoted |= quoted;
        self.segs.push(Seg::literal(ch, quoted));
    }

    /// Records an expansion whose value is unknown: its source text stands in for it, marked
    /// quoted so that no brace or glob processing is applied to the placeholder.
    /// `in_double_quotes` says whether the shell will word-split the value.
    fn push_dynamic(&mut self, source_text: &str, in_double_quotes: bool) {
        self.dynamic = true;
        self.unquoted_expansion |= !in_double_quotes;
        for ch in source_text.chars() {
            self.segs.push(Seg {
                ch,
                quoted: true,
                dynamic: true,
            });
        }
    }

    fn text(&self) -> String {
        self.segs.iter().map(|seg| seg.ch).collect()
    }

    /// Whether the word is exactly `s` with no quoting or expansion — the only form in which
    /// the shell recognises a reserved word.
    fn is_plain(&self, s: &str) -> bool {
        !self.any_quoted && !self.dynamic && self.segs.iter().map(|seg| seg.ch).eq(s.chars())
    }

    /// Length of a leading unquoted `NAME=` / `NAME+=`, if the word starts with one.
    fn assignment_prefix_len(&self) -> Option<usize> {
        let mut segs = self.segs.iter().enumerate();
        let (_, first) = segs.next()?;
        if first.quoted || !(first.ch.is_ascii_alphabetic() || first.ch == '_') {
            return None;
        }
        for (index, seg) in segs {
            if seg.quoted {
                return None;
            }
            match seg.ch {
                '=' => return Some(index + 1),
                '+' => {
                    return match self.segs.get(index + 1) {
                        Some(Seg {
                            ch: '=',
                            quoted: false,
                            ..
                        }) => Some(index + 2),
                        _ => None,
                    };
                }
                c if c.is_ascii_alphanumeric() || c == '_' => {}
                _ => return None,
            }
        }
        None
    }

    fn is_assignment(&self) -> bool {
        self.assignment_prefix_len().is_some()
    }

    /// Whether the word so far is exactly `NAME=` / `NAME+=`, so a following `(` opens a
    /// bash array value rather than ending the word.
    fn is_array_assignment_prefix(&self) -> bool {
        self.assignment_prefix_len() == Some(self.segs.len())
    }

    /// A file-descriptor prefix of a redirection: `2` in `2>x`, `{fd}` in `{fd}>x`.
    fn is_io_number(&self) -> bool {
        if self.any_quoted || self.dynamic || self.segs.is_empty() {
            return false;
        }
        let text = self.text();
        if text.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
        text.strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
            .is_some_and(|name| {
                let mut chars = name.chars();
                chars
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
            })
    }
}

/// A word after brace expansion.
#[derive(Clone, Debug)]
struct ExpandedWord {
    text: String,
    /// Some of the value is only known at run time (an expansion or a glob).
    dynamic: bool,
    /// Used as a command word, the program it names is only known at run time. A quoted
    /// expansion confined to the directory part (`"$HOME"/bin/tool`) does not count: the
    /// program name `tool` is still static, and that is what a policy rule names.
    name_unknown: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Term {
    Eof,
    Paren,
}

#[derive(Debug)]
enum Tok {
    Word(Word),
    Newline,
    Op(Op),
    Eof,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Op {
    Semi,
    /// `;;`, `;&` or `;;&`
    CaseBreak,
    Amp,
    AndIf,
    Pipe,
    OrIf,
    LParen,
    RParen,
    Redirect {
        heredoc: Option<bool>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    /// No command word yet: reserved words and assignments are recognised.
    CommandStart,
    /// The command word has been seen; further words are arguments.
    Arguments,
    /// Inside a `for`/`select` header, before its body.
    ForHeader { saw_in: bool, words: usize },
    /// After `case`, before `in`.
    CaseHeader,
    /// Reading `case` patterns, up to `)`.
    CasePattern,
    /// Inside `[[ … ]]`.
    DoubleBracket,
    /// After `function`, before the function name.
    FunctionName,
}

struct Heredoc {
    delimiter: String,
    strip_tabs: bool,
    expands: bool,
}

#[derive(Default)]
struct SimpleCommand {
    /// Leading assignments and redirections seen before the command word.
    prefix: usize,
    words: Vec<Word>,
}

impl SimpleCommand {
    fn is_empty(&self) -> bool {
        self.prefix == 0 && self.words.is_empty()
    }
}

struct Analyzer<'o> {
    chars: Vec<char>,
    pos: usize,
    escape: char,
    posix: bool,
    escape_char: EscapeChar,
    depth: usize,
    out: &'o mut ExecutedCommands,
    heredocs: Vec<Heredoc>,
    pushback: Option<Tok>,
    nesting: usize,
}

impl<'o> Analyzer<'o> {
    fn new(
        source: &str,
        escape_char: EscapeChar,
        depth: usize,
        out: &'o mut ExecutedCommands,
    ) -> Self {
        let posix = matches!(escape_char, EscapeChar::Backslash);
        Self {
            chars: source.chars().collect(),
            pos: 0,
            escape: if posix { '\\' } else { '`' },
            posix,
            escape_char,
            depth,
            out,
            heredocs: Vec::new(),
            pushback: None,
            nesting: 0,
        }
    }

    fn unresolved(&mut self, reason: impl Into<String>) {
        let reason = reason.into();
        if !self.out.unresolved.contains(&reason) {
            self.out.unresolved.push(reason);
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.chars.get(self.pos + offset).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn slice(&self, start: usize, end: usize) -> String {
        self.chars[start..end.min(self.chars.len())]
            .iter()
            .collect()
    }

    fn peek_nonblank(&self) -> Option<char> {
        self.chars[self.pos..]
            .iter()
            .copied()
            .find(|c| !matches!(c, ' ' | '\t' | '\r'))
    }

    /// Analyses `source` as a nested command line (`eval`, `sh -c`, backticks, …).
    fn analyze_nested(&mut self, source: &str, escape_char: EscapeChar) {
        analyze(source, escape_char, self.depth + 1, &mut *self.out);
    }

    // ---------------------------------------------------------------------------------------
    // Tokens
    // ---------------------------------------------------------------------------------------

    fn next_token(&mut self) -> Tok {
        if let Some(tok) = self.pushback.take() {
            return tok;
        }
        loop {
            match self.peek() {
                Some(' ' | '\t' | '\r') => self.pos += 1,
                Some(c) if c == self.escape && self.peek_at(1) == Some('\n') => self.pos += 2,
                Some('#') => {
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
        let Some(c) = self.peek() else {
            return Tok::Eof;
        };
        match c {
            '\n' => {
                self.pos += 1;
                self.read_heredoc_bodies();
                Tok::Newline
            }
            ';' => {
                self.pos += 1;
                match self.peek() {
                    Some(';') => {
                        self.pos += 1;
                        if self.peek() == Some('&') {
                            self.pos += 1;
                        }
                        Tok::Op(Op::CaseBreak)
                    }
                    Some('&') => {
                        self.pos += 1;
                        Tok::Op(Op::CaseBreak)
                    }
                    _ => Tok::Op(Op::Semi),
                }
            }
            '&' => {
                self.pos += 1;
                match self.peek() {
                    Some('&') => {
                        self.pos += 1;
                        Tok::Op(Op::AndIf)
                    }
                    Some('>') => {
                        // `&>file` / `&>>file`
                        self.pos += 1;
                        if self.peek() == Some('>') {
                            self.pos += 1;
                        }
                        Tok::Op(Op::Redirect { heredoc: None })
                    }
                    _ => Tok::Op(Op::Amp),
                }
            }
            '|' => {
                self.pos += 1;
                match self.peek() {
                    Some('|') => {
                        self.pos += 1;
                        Tok::Op(Op::OrIf)
                    }
                    Some('&') => {
                        self.pos += 1;
                        Tok::Op(Op::Pipe)
                    }
                    _ => Tok::Op(Op::Pipe),
                }
            }
            '(' => {
                self.pos += 1;
                Tok::Op(Op::LParen)
            }
            ')' => {
                self.pos += 1;
                Tok::Op(Op::RParen)
            }
            '<' | '>' if !(self.posix && self.peek_at(1) == Some('(')) => self.read_redirect_op(),
            _ => {
                let word = self.read_word();
                if matches!(self.peek(), Some('<' | '>'))
                    && !(self.posix && self.peek_at(1) == Some('('))
                    && word.is_io_number()
                {
                    return self.read_redirect_op();
                }
                Tok::Word(word)
            }
        }
    }

    /// Reads a redirection operator; the current character is `<` or `>`.
    fn read_redirect_op(&mut self) -> Tok {
        let heredoc = if self.bump() == Some('<') {
            match self.peek() {
                Some('<') => {
                    self.pos += 1;
                    match self.peek() {
                        // `<<<` here-string: an ordinary target word.
                        Some('<') => {
                            self.pos += 1;
                            None
                        }
                        Some('-') => {
                            self.pos += 1;
                            Some(true)
                        }
                        _ => Some(false),
                    }
                }
                Some('&' | '>') => {
                    self.pos += 1;
                    None
                }
                _ => None,
            }
        } else {
            if matches!(self.peek(), Some('>' | '&' | '|')) {
                self.pos += 1;
            }
            None
        };
        Tok::Op(Op::Redirect { heredoc })
    }

    fn read_word(&mut self) -> Word {
        let mut word = Word::default();
        let mut open_bracket = false;
        while let Some(c) = self.peek() {
            match c {
                ' ' | '\t' | '\r' | '\n' | ';' | '&' | '|' | ')' => break,
                '<' | '>' => {
                    if self.posix && self.peek_at(1) == Some('(') {
                        // Process substitution: its commands run.
                        let start = self.pos;
                        self.pos += 2;
                        self.parse_nested_paren();
                        let text = self.slice(start, self.pos);
                        // Expands to one `/dev/fd/N` path: never word-split.
                        word.push_dynamic(&text, true);
                    } else {
                        break;
                    }
                }
                '(' => {
                    let start = self.pos;
                    if word.is_array_assignment_prefix() {
                        self.pos += 1;
                        self.scan_balanced('(', ')', 1);
                        let text = self.slice(start, self.pos);
                        for ch in text.chars() {
                            word.push(ch, true);
                        }
                    } else if matches!(
                        word.segs.last(),
                        Some(Seg {
                            ch: '@' | '!' | '+' | '*' | '?',
                            quoted: false,
                            ..
                        })
                    ) {
                        // extglob pattern `@( … )` and friends.
                        self.pos += 1;
                        self.scan_balanced('(', ')', 1);
                        let text = self.slice(start, self.pos);
                        for ch in text.chars() {
                            word.push(ch, true);
                        }
                        word.glob = true;
                    } else {
                        break;
                    }
                }
                c if c == self.escape => {
                    self.pos += 1;
                    match self.bump() {
                        None => word.push(c, false),
                        Some('\n') => {}
                        Some(next) => word.push(next, true),
                    }
                }
                '\'' => {
                    self.pos += 1;
                    self.read_single_quoted(&mut word);
                }
                '"' => {
                    self.pos += 1;
                    self.read_double_quoted(&mut word);
                }
                '`' if self.posix => self.read_backtick(&mut word, false),
                '$' => self.read_dollar(&mut word, false),
                '*' | '?' => {
                    word.glob = true;
                    word.push(c, false);
                    self.pos += 1;
                }
                '[' => {
                    open_bracket = true;
                    word.push(c, false);
                    self.pos += 1;
                }
                ']' => {
                    if open_bracket {
                        word.glob = true;
                    }
                    word.push(c, false);
                    self.pos += 1;
                }
                '!' => {
                    self.check_history_expansion();
                    word.push(c, false);
                    self.pos += 1;
                }
                _ => {
                    word.push(c, false);
                    self.pos += 1;
                }
            }
        }
        word
    }

    /// bash and zsh expand `!event` from history before parsing, so the text that runs is not
    /// the text we see. The current character is `!`.
    fn check_history_expansion(&mut self) {
        if !self.posix {
            return;
        }
        match self.peek_at(1) {
            None | Some(' ' | '\t' | '\r' | '\n' | '=' | '(' | '"') => {}
            Some(_) => self.unresolved("history expansion (`!…`)"),
        }
    }

    /// Reads a single-quoted section; the opening quote has been consumed.
    fn read_single_quoted(&mut self, word: &mut Word) {
        word.any_quoted = true;
        loop {
            match self.bump() {
                None => {
                    self.unresolved("unterminated single quote");
                    return;
                }
                Some('\'') => {
                    if !self.posix && self.peek() == Some('\'') {
                        self.pos += 1;
                        word.push('\'', true);
                    } else {
                        return;
                    }
                }
                Some(c) => word.push(c, true),
            }
        }
    }

    /// Reads a double-quoted section; the opening quote has been consumed.
    fn read_double_quoted(&mut self, word: &mut Word) {
        word.any_quoted = true;
        loop {
            let Some(c) = self.peek() else {
                self.unresolved("unterminated double quote");
                return;
            };
            match c {
                '"' => {
                    self.pos += 1;
                    if !self.posix && self.peek() == Some('"') {
                        self.pos += 1;
                        word.push('"', true);
                    } else {
                        return;
                    }
                }
                c if c == self.escape => {
                    self.pos += 1;
                    match self.bump() {
                        None => word.push(c, true),
                        Some('\n') => {}
                        Some(next) if !self.posix || matches!(next, '$' | '`' | '"' | '\\') => {
                            word.push(next, true)
                        }
                        Some(next) => {
                            word.push(c, true);
                            word.push(next, true);
                        }
                    }
                }
                '`' if self.posix => self.read_backtick(word, true),
                '$' => self.read_dollar(word, true),
                '!' => {
                    self.check_history_expansion();
                    word.push(c, true);
                    self.pos += 1;
                }
                _ => {
                    word.push(c, true);
                    self.pos += 1;
                }
            }
        }
    }

    /// Reads an expansion introduced by `$`; the current character is `$`.
    fn read_dollar(&mut self, word: &mut Word, in_double_quotes: bool) {
        let start = self.pos;
        match self.peek_at(1) {
            Some('\'') if self.posix && !in_double_quotes => {
                self.pos += 2;
                self.read_ansi_c_quoted(word);
            }
            Some('"') if self.posix && !in_double_quotes => {
                self.pos += 2;
                self.read_double_quoted(word);
            }
            Some('(') => {
                if self.peek_at(2) == Some('(') {
                    self.pos += 3;
                    self.scan_arithmetic();
                } else {
                    self.pos += 2;
                    self.parse_nested_paren();
                }
                let text = self.slice(start, self.pos);
                word.push_dynamic(&text, in_double_quotes);
            }
            Some('{') => {
                self.pos += 2;
                self.scan_balanced('{', '}', 1);
                let text = self.slice(start, self.pos);
                word.push_dynamic(&text, in_double_quotes);
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                self.pos += 1;
                while let Some(c) = self.peek() {
                    if c.is_ascii_alphanumeric() || c == '_' || (!self.posix && c == ':') {
                        self.pos += 1;
                    } else {
                        break;
                    }
                }
                let text = self.slice(start, self.pos);
                word.push_dynamic(&text, in_double_quotes);
            }
            Some(c) if c.is_ascii_digit() || "@*#?$!-".contains(c) => {
                self.pos += 2;
                let text = self.slice(start, self.pos);
                word.push_dynamic(&text, in_double_quotes);
            }
            _ => {
                self.pos += 1;
                word.push('$', in_double_quotes);
            }
        }
    }

    /// Reads and decodes a `$'…'` section; `$'` has been consumed.
    fn read_ansi_c_quoted(&mut self, word: &mut Word) {
        word.any_quoted = true;
        loop {
            match self.bump() {
                None => {
                    self.unresolved("unterminated $'…' quote");
                    return;
                }
                Some('\'') => return,
                Some('\\') => {
                    let Some(escaped) = self.bump() else {
                        word.push('\\', true);
                        continue;
                    };
                    let decoded = match escaped {
                        'a' => Some('\u{07}'),
                        'b' => Some('\u{08}'),
                        'e' | 'E' => Some('\u{1b}'),
                        'f' => Some('\u{0c}'),
                        'n' => Some('\n'),
                        'r' => Some('\r'),
                        't' => Some('\t'),
                        'v' => Some('\u{0b}'),
                        '\\' | '\'' | '"' | '?' => Some(escaped),
                        'x' => self.read_radix_escape(16, 2),
                        'u' => self.read_radix_escape(16, 4),
                        'U' => self.read_radix_escape(16, 8),
                        '0'..='7' => {
                            self.pos -= 1;
                            self.read_radix_escape(8, 3)
                        }
                        'c' => self.bump().map(|c| char::from((c as u32 as u8) & 0x1f)),
                        other => {
                            word.push('\\', true);
                            Some(other)
                        }
                    };
                    match decoded {
                        Some(c) => word.push(c, true),
                        None => self.unresolved("undecodable escape in $'…' quote"),
                    }
                }
                Some(c) => word.push(c, true),
            }
        }
    }

    fn read_radix_escape(&mut self, radix: u32, max_digits: usize) -> Option<char> {
        let mut value: u32 = 0;
        let mut digits = 0;
        while digits < max_digits {
            match self.peek().and_then(|c| c.to_digit(radix)) {
                Some(d) => {
                    value = value * radix + d;
                    digits += 1;
                    self.pos += 1;
                }
                None => break,
            }
        }
        if digits == 0 {
            return None;
        }
        char::from_u32(value)
    }

    /// Reads a backtick command substitution; the current character is the opening backtick.
    fn read_backtick(&mut self, word: &mut Word, in_double_quotes: bool) {
        let start = self.pos;
        self.pos += 1;
        let mut content = String::new();
        loop {
            match self.bump() {
                None => {
                    self.unresolved("unterminated backtick substitution");
                    break;
                }
                Some('`') => break,
                Some('\\') => match self.bump() {
                    Some(next @ ('$' | '`' | '\\')) => content.push(next),
                    Some(next) => {
                        content.push('\\');
                        content.push(next);
                    }
                    None => content.push('\\'),
                },
                Some(c) => content.push(c),
            }
        }
        self.analyze_nested(&content, EscapeChar::Backslash);
        let text = self.slice(start, self.pos);
        word.push_dynamic(&text, in_double_quotes);
    }

    /// Parses the command list of `$( … )`, `( … )` or `<( … )`; the opening paren has been
    /// consumed, and the closing one is consumed here.
    fn parse_nested_paren(&mut self) {
        if self.nesting >= MAX_NESTING {
            // Give up on the rest of the line rather than recurse without bound.
            self.unresolved("grouping nested too deeply to analyse");
            self.pos = self.chars.len();
            return;
        }
        self.nesting += 1;
        let closed = self.parse_list(Term::Paren);
        self.nesting -= 1;
        if !closed {
            self.unresolved("unterminated `(`");
        }
    }

    /// Skips a balanced bracketed region (`${ … }`, `(( … ))`, an array value, an extglob),
    /// analysing any command substitutions inside it. The opening bracket(s) have been
    /// consumed; `depth` says how many.
    fn scan_balanced(&mut self, open: char, close: char, mut depth: usize) {
        let mut scratch = Word::default();
        while depth > 0 {
            let Some(c) = self.peek() else {
                self.unresolved(format!("unterminated `{open}`"));
                return;
            };
            match c {
                c if c == close => {
                    depth -= 1;
                    self.pos += 1;
                }
                c if c == open => {
                    depth += 1;
                    self.pos += 1;
                }
                c if c == self.escape => self.pos += 2,
                '\'' => {
                    self.pos += 1;
                    self.read_single_quoted(&mut scratch);
                }
                '"' => {
                    self.pos += 1;
                    self.read_double_quoted(&mut scratch);
                }
                '`' if self.posix => self.read_backtick(&mut scratch, false),
                '$' => self.read_dollar(&mut scratch, false),
                _ => self.pos += 1,
            }
        }
        self.pos = self.pos.min(self.chars.len());
    }

    /// Skips an arithmetic body after `((` / `$((`, analysing its substitutions like
    /// [`Self::scan_balanced`].
    ///
    /// Additionally, and only additively, the body is read as a command list: bash and zsh
    /// evaluate `((rm -rf ~))` as arithmetic, but dash has no `(( … ))` and parses it as
    /// nested subshells, and in fish `( … )` is command substitution — both run `rm`. The
    /// commands found are offered as candidates; what that reading cannot resolve is
    /// discarded, because in the shells that do have arithmetic, `(( $n > 1 ))` names no
    /// command at all.
    fn scan_arithmetic(&mut self) {
        let body_start = self.pos;
        self.scan_balanced('(', ')', 2);
        let body_end = self.pos.saturating_sub(2).max(body_start);
        if self.depth >= MAX_DEPTH {
            return;
        }
        let body = self.slice(body_start, body_end);
        let mut as_commands = ExecutedCommands::default();
        analyze(&body, self.escape_char, self.depth + 1, &mut as_commands);
        self.out.commands.extend(as_commands.commands);
    }

    /// Consumes the bodies of here-documents whose operators preceded the newline just read.
    fn read_heredoc_bodies(&mut self) {
        for heredoc in std::mem::take(&mut self.heredocs) {
            let body_start = self.pos;
            let mut body_end = self.chars.len();
            while self.pos < self.chars.len() {
                let line_start = self.pos;
                let line_end = self.chars[line_start..]
                    .iter()
                    .position(|&c| c == '\n')
                    .map_or(self.chars.len(), |offset| line_start + offset);
                let line = self.slice(line_start, line_end);
                self.pos = (line_end + 1).min(self.chars.len());
                let line = if heredoc.strip_tabs {
                    line.trim_start_matches('\t')
                } else {
                    line.as_str()
                };
                if line == heredoc.delimiter {
                    body_end = line_start;
                    break;
                }
            }
            if heredoc.expands && self.posix && self.depth >= MAX_DEPTH {
                self.unresolved("command nesting is too deep to analyse");
            } else if heredoc.expands && self.posix {
                // An unquoted delimiter means `$( … )` and backticks in the body run.
                let body = self.slice(body_start, body_end);
                let mut body_analyzer =
                    Analyzer::new(&body, self.escape_char, self.depth + 1, &mut *self.out);
                body_analyzer.scan_heredoc_body();
            }
        }
    }

    fn scan_heredoc_body(&mut self) {
        let mut scratch = Word::default();
        while let Some(c) = self.peek() {
            match c {
                '\\' => self.pos += 2,
                '`' => self.read_backtick(&mut scratch, true),
                '$' => self.read_dollar(&mut scratch, true),
                _ => self.pos += 1,
            }
        }
    }

    // ---------------------------------------------------------------------------------------
    // Grammar
    // ---------------------------------------------------------------------------------------

    /// Parses a command list up to `term`. Returns whether the terminator was found (always
    /// true for [`Term::Eof`]).
    fn parse_list(&mut self, term: Term) -> bool {
        let mut cmd = SimpleCommand::default();
        let mut mode = Mode::CommandStart;
        let mut open_cases: usize = 0;
        let mut open_compounds: i64 = 0;
        let mut after_time = false;

        loop {
            let tok = self.next_token();
            let is_newline = matches!(tok, Tok::Newline);
            match tok {
                Tok::Eof => {
                    self.flush(&mut cmd);
                    if term != Term::Eof {
                        return false;
                    }
                    if open_compounds != 0 || open_cases != 0 || mode_is_open(mode) {
                        self.unresolved("unterminated compound command");
                    }
                    return true;
                }
                Tok::Word(word) => match mode {
                    Mode::DoubleBracket => {
                        if word.is_plain("]]") {
                            mode = Mode::Arguments;
                        }
                    }
                    Mode::FunctionName => mode = Mode::CommandStart,
                    Mode::ForHeader { saw_in, words } => {
                        if word.is_plain("do") && !saw_in && words <= 1 {
                            mode = Mode::CommandStart;
                        } else {
                            mode = Mode::ForHeader {
                                saw_in: saw_in || (words == 1 && word.is_plain("in")),
                                words: words + 1,
                            };
                        }
                    }
                    Mode::CaseHeader => {
                        if word.is_plain("in") {
                            mode = Mode::CasePattern;
                        }
                    }
                    Mode::CasePattern => {
                        if word.is_plain("esac") {
                            open_cases = open_cases.saturating_sub(1);
                            open_compounds -= 1;
                            mode = Mode::Arguments;
                        }
                    }
                    Mode::Arguments => cmd.words.push(word),
                    Mode::CommandStart => {
                        if after_time && word.segs.first().is_some_and(|seg| seg.ch == '-') {
                            continue;
                        }
                        after_time = false;
                        let keyword = if cmd.is_empty() {
                            RESERVED_WORDS.iter().find(|kw| word.is_plain(kw))
                        } else {
                            None
                        };
                        if let Some(keyword) = keyword {
                            match *keyword {
                                "if" | "while" | "until" | "{" => open_compounds += 1,
                                "fi" | "done" | "}" => open_compounds -= 1,
                                "esac" => {
                                    if open_cases > 0 {
                                        open_cases -= 1;
                                        open_compounds -= 1;
                                    } else {
                                        self.unresolved("unexpected `esac`");
                                    }
                                }
                                "for" | "select" => {
                                    open_compounds += 1;
                                    mode = Mode::ForHeader {
                                        saw_in: false,
                                        words: 0,
                                    };
                                }
                                "case" => {
                                    open_compounds += 1;
                                    open_cases += 1;
                                    mode = Mode::CaseHeader;
                                }
                                "function" => mode = Mode::FunctionName,
                                "[[" => mode = Mode::DoubleBracket,
                                "time" => after_time = true,
                                // then, else, elif, do, !, coproc
                                _ => {}
                            }
                            if open_compounds < 0 {
                                self.unresolved("unbalanced compound command");
                                open_compounds = 0;
                            }
                            continue;
                        }
                        if word.is_assignment() {
                            cmd.prefix += 1;
                        } else {
                            cmd.words.push(word);
                            mode = Mode::Arguments;
                        }
                    }
                },
                Tok::Newline | Tok::Op(Op::Semi | Op::Amp | Op::AndIf | Op::OrIf | Op::Pipe) => {
                    match mode {
                        Mode::DoubleBracket => continue,
                        Mode::CasePattern => continue,
                        Mode::CaseHeader if is_newline => continue,
                        _ => {}
                    }
                    self.flush(&mut cmd);
                    after_time = false;
                    mode = Mode::CommandStart;
                }
                Tok::Op(Op::CaseBreak) => {
                    self.flush(&mut cmd);
                    if open_cases == 0 {
                        self.unresolved("unexpected `;;`");
                        mode = Mode::CommandStart;
                    } else {
                        mode = Mode::CasePattern;
                    }
                }
                Tok::Op(Op::LParen) => match mode {
                    Mode::DoubleBracket | Mode::CasePattern => {}
                    Mode::ForHeader { saw_in, words } if words == 0 && self.peek() == Some('(') => {
                        self.pos += 1;
                        self.scan_balanced('(', ')', 2);
                        mode = Mode::ForHeader {
                            saw_in,
                            words: words + 1,
                        };
                    }
                    Mode::CommandStart if cmd.is_empty() => {
                        if self.peek() == Some('(') {
                            // Arithmetic command `(( … ))`.
                            self.pos += 1;
                            self.scan_arithmetic();
                        } else {
                            self.parse_nested_paren();
                        }
                    }
                    Mode::Arguments
                        if cmd.prefix == 0
                            && cmd.words.len() == 1
                            && self.peek_nonblank() == Some(')') =>
                    {
                        // Function definition `name() …`: the name is not executed here, the
                        // body is parsed as ordinary commands below.
                        cmd.words.clear();
                        while !matches!(self.bump(), Some(')') | None) {}
                        mode = Mode::CommandStart;
                    }
                    _ => {
                        self.unresolved("unexpected `(`");
                        self.flush(&mut cmd);
                        self.parse_nested_paren();
                        mode = Mode::CommandStart;
                    }
                },
                Tok::Op(Op::RParen) => match mode {
                    Mode::DoubleBracket => {}
                    Mode::CasePattern => mode = Mode::CommandStart,
                    _ => {
                        self.flush(&mut cmd);
                        if term == Term::Paren {
                            if open_compounds != 0 || open_cases != 0 || mode_is_open(mode) {
                                self.unresolved("unterminated compound command");
                            }
                            return true;
                        }
                        self.unresolved("unexpected `)`");
                        mode = Mode::CommandStart;
                    }
                },
                Tok::Op(Op::Redirect { heredoc }) => {
                    if mode == Mode::DoubleBracket {
                        // `<`/`>` compare strings inside `[[ … ]]`.
                        continue;
                    }
                    match self.next_token() {
                        Tok::Word(target) => {
                            if let Some(strip_tabs) = heredoc {
                                self.heredocs.push(Heredoc {
                                    delimiter: target.text(),
                                    strip_tabs,
                                    expands: !target.any_quoted,
                                });
                            }
                        }
                        other => {
                            self.unresolved("redirection without a target");
                            self.pushback = Some(other);
                        }
                    }
                    if mode == Mode::CommandStart {
                        cmd.prefix += 1;
                    }
                }
            }
        }
    }

    /// Records the simple command gathered so far, if it has a command word.
    fn flush(&mut self, cmd: &mut SimpleCommand) {
        let command = std::mem::take(cmd);
        if command.words.is_empty() {
            return;
        }
        let mut words = Vec::new();
        for word in &command.words {
            match brace_expand(&word.segs) {
                Some(expansions) => words.extend(expansions.into_iter().map(|segs| ExpandedWord {
                    text: segs.iter().map(|seg| seg.ch).collect(),
                    dynamic: word.dynamic || word.glob,
                    name_unknown: word.unquoted_expansion
                        || word.glob
                        || dynamic_after_last_slash(&segs),
                })),
                None => {
                    self.unresolved("brace expansion too large to enumerate");
                    words.push(ExpandedWord {
                        text: word.text(),
                        dynamic: true,
                        name_unknown: true,
                    });
                }
            }
        }
        self.emit(&words, 0);
    }

    /// Records `words` as an executed command, then follows any command it runs in turn.
    fn emit(&mut self, words: &[ExpandedWord], hops: usize) {
        let Some(first) = words.first() else {
            return;
        };
        if hops > MAX_DEPTH {
            self.unresolved("command wrappers nested too deeply to analyse");
            return;
        }
        self.out
            .commands
            .push(words.iter().map(|word| word.text.clone()).collect());
        let args = &words[1..];

        if !self.posix
            && first.text.starts_with('$')
            && args.first().is_some_and(|arg| {
                matches!(
                    arg.text.as_str(),
                    "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "??="
                )
            })
        {
            // PowerShell assignment `$x = <pipeline>`: the right-hand side is a command.
            self.emit(&args[1..], hops + 1);
            return;
        }

        if first.name_unknown {
            self.unresolved(format!(
                "command word `{}` is only known at run time",
                first.text
            ));
            return;
        }

        let name = if self.posix {
            program_name(&first.text).to_string()
        } else {
            program_name(&first.text).to_ascii_lowercase()
        };
        match name.as_str() {
            "eval" | "iex" | "invoke-expression" => self.analyze_args_as_command(args),
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "ash" | "yash" | "fish" | "csh"
            | "tcsh" | "pwsh" | "powershell" | "cmd" => {
                if let Some(index) = shell_command_string_index(args) {
                    // POSIX shells take exactly one command string (later words are `$0`,
                    // `$1`, …); PowerShell and cmd run the rest of the line.
                    let (command_words, escape_char) = match name.as_str() {
                        "pwsh" | "powershell" => (&args[index..], EscapeChar::Backtick),
                        "cmd" => (&args[index..], EscapeChar::Backslash),
                        _ => (&args[index..index + 1], EscapeChar::Backslash),
                    };
                    self.analyze_args_as_command_with(command_words, escape_char);
                }
            }
            "find" => self.emit_find_exec(args, hops),
            "alias" => {
                for arg in args {
                    if let Some((_, value)) = arg.text.split_once('=') {
                        if arg.dynamic {
                            self.unresolved("alias with a value only known at run time");
                        } else {
                            self.analyze_nested(value, self.escape_char);
                        }
                    }
                }
            }
            "trap" => {
                if let Some(action) = args.first().filter(|arg| !arg.text.starts_with('-')) {
                    if action.dynamic {
                        self.unresolved("trap action only known at run time");
                    } else {
                        self.analyze_nested(&action.text, self.escape_char);
                    }
                }
            }
            "env" if env_split_string(args).is_some() => {
                if let Some((source, dynamic)) = env_split_string(args) {
                    if dynamic {
                        self.unresolved("command string only known at run time");
                    }
                    self.analyze_nested(&source, self.escape_char);
                }
            }
            "command" if command_only_describes(args) => {}
            _ => {
                let Some(spec) = wrapper_spec(&name) else {
                    return;
                };
                // Command strings given by option (`su -c …`, `flock -c …`, `script -c …`).
                if let Some(index) = command_string_option_index(spec, args) {
                    self.analyze_args_as_command(&args[index..index + 1]);
                }
                match peel_wrapper(spec, args) {
                    Peeled::Inner(index) => {
                        if spec.joins_rest_as_string {
                            self.analyze_args_as_command(&args[index..]);
                        } else {
                            self.emit(&args[index..], hops + 1);
                        }
                    }
                    Peeled::NoCommand => {}
                    Peeled::Unknown => {
                        // An option this table does not know: the inner command word cannot be
                        // located, so every suffix is offered to the policy as a candidate.
                        if args.len() > MAX_WRAPPER_SUFFIXES {
                            self.unresolved("wrapper options could not be parsed");
                        }
                        for index in 0..args.len().min(MAX_WRAPPER_SUFFIXES) {
                            self.out
                                .commands
                                .push(args[index..].iter().map(|w| w.text.clone()).collect());
                        }
                    }
                }
            }
        }
    }

    /// Analyses `args` (joined as `eval` joins them) as a command line.
    fn analyze_args_as_command(&mut self, args: &[ExpandedWord]) {
        self.analyze_args_as_command_with(args, self.escape_char);
    }

    fn analyze_args_as_command_with(&mut self, args: &[ExpandedWord], escape_char: EscapeChar) {
        if args.is_empty() {
            return;
        }
        if args.iter().any(|arg| arg.dynamic) {
            self.unresolved("command string only known at run time");
        }
        let source = args
            .iter()
            .map(|arg| arg.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        self.analyze_nested(&source, escape_char);
    }

    /// `find … -exec cmd … ;` (and `-execdir`, `-ok`, `-okdir`) runs `cmd`.
    fn emit_find_exec(&mut self, args: &[ExpandedWord], hops: usize) {
        let mut index = 0;
        while index < args.len() {
            if matches!(
                args[index].text.as_str(),
                "-exec" | "-execdir" | "-ok" | "-okdir"
            ) {
                let start = index + 1;
                let mut end = start;
                while end < args.len() && !matches!(args[end].text.as_str(), ";" | "+") {
                    end += 1;
                }
                self.emit(&args[start..end], hops + 1);
                index = end;
            }
            index += 1;
        }
    }
}

/// Whether a later `/`-free part of a command word — the program name — contains an
/// expansion. `"$HOME"/bin/tool` names `tool`; `"$DIR"` and `tool-"$V"` name nothing static.
fn dynamic_after_last_slash(segs: &[Seg]) -> bool {
    let name_start = segs
        .iter()
        .rposition(|seg| seg.ch == '/' && !seg.dynamic)
        .map_or(0, |index| index + 1);
    segs[name_start..].iter().any(|seg| seg.dynamic)
}

/// `command -v rm` / `command -V rm` only describe `rm`; they do not run it.
fn command_only_describes(args: &[ExpandedWord]) -> bool {
    args.iter()
        .take_while(|arg| arg.text.starts_with('-') && arg.text != "--")
        .any(|arg| arg.text.contains(['v', 'V']))
}

const RESERVED_WORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "do", "done", "while", "until", "for", "select", "case",
    "esac", "{", "}", "!", "function", "[[", "time", "coproc",
];

fn mode_is_open(mode: Mode) -> bool {
    matches!(
        mode,
        Mode::ForHeader { .. }
            | Mode::CaseHeader
            | Mode::CasePattern
            | Mode::DoubleBracket
            | Mode::FunctionName
    )
}

/// For `sh`-family shells: the index of the command string when `-c` (or PowerShell's
/// `-Command`, cmd's `/c`) is given.
fn shell_command_string_index(args: &[ExpandedWord]) -> Option<usize> {
    let mut saw_command_flag = false;
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].text.as_str();
        let lower = arg.to_ascii_lowercase();
        if matches!(lower.as_str(), "/c" | "/k" | "-command" | "--command") {
            return (index + 1 < args.len()).then_some(index + 1);
        }
        if arg == "--" || arg == "-" {
            index += 1;
            break;
        }
        if matches!(arg, "-o" | "+o" | "-O" | "+O" | "--rcfile" | "--init-file") {
            index += 2;
            continue;
        }
        if arg.len() > 1 && (arg.starts_with('-') || arg.starts_with('+')) {
            if !arg.starts_with("--") && arg[1..].contains('c') {
                saw_command_flag = true;
            }
            index += 1;
            continue;
        }
        break;
    }
    (saw_command_flag && index < args.len()).then_some(index)
}

/// `env -S 'cmd args'` / `env --split-string=…`: the command line `env` splits and runs,
/// and whether any of it is only known at run time.
fn env_split_string(args: &[ExpandedWord]) -> Option<(String, bool)> {
    for (index, arg) in args.iter().enumerate() {
        let text = arg.text.as_str();
        if text == "--" || !text.starts_with('-') {
            return None;
        }
        let attached = if text == "--split-string" || text == "-S" {
            Some("")
        } else if let Some(value) = text.strip_prefix("--split-string=") {
            Some(value)
        } else if !text.starts_with("--") {
            text.split_once('S').map(|(_, value)| value)
        } else {
            None
        };
        if let Some(attached) = attached {
            let rest = &args[index + 1..];
            let source = std::iter::once(attached)
                .chain(rest.iter().map(|word| word.text.as_str()))
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            let dynamic = arg.dynamic || rest.iter().any(|word| word.dynamic);
            return Some((source, dynamic));
        }
    }
    None
}

struct WrapperSpec {
    /// Short options that take a value (from the next word when not attached).
    short_with_arg: &'static str,
    /// Short options that take no value.
    short_flags: &'static str,
    long_with_arg: &'static [&'static str],
    long_flags: &'static [&'static str],
    /// Positional arguments between the options and the command (`timeout DURATION cmd`).
    positionals: usize,
    /// Leading `NAME=value` words are assignments, not the command (`env`).
    skip_assignments: bool,
    /// `-10` style numeric options are accepted (`nice`).
    numeric_options: bool,
    /// A short option whose value is itself a command string (`su -c`), and its long form.
    command_string_short: Option<char>,
    command_string_long: &'static [&'static str],
    /// The remaining words are joined and run by a shell (`watch`).
    joins_rest_as_string: bool,
}

const fn wrapper(short_with_arg: &'static str, short_flags: &'static str) -> WrapperSpec {
    WrapperSpec {
        short_with_arg,
        short_flags,
        long_with_arg: &[],
        long_flags: &[],
        positionals: 0,
        skip_assignments: false,
        numeric_options: false,
        command_string_short: None,
        command_string_long: &[],
        joins_rest_as_string: false,
    }
}

static SUDO: WrapperSpec = WrapperSpec {
    long_with_arg: &[
        "--chdir",
        "--close-from",
        "--group",
        "--host",
        "--prompt",
        "--chroot",
        "--command-timeout",
        "--other-user",
        "--user",
        "--role",
        "--type",
    ],
    long_flags: &[
        "--askpass",
        "--background",
        "--bell",
        "--preserve-env",
        "--edit",
        "--set-home",
        "--login",
        "--remove-timestamp",
        "--reset-timestamp",
        "--list",
        "--non-interactive",
        "--preserve-groups",
        "--stdin",
        "--shell",
        "--version",
        "--validate",
        "--help",
    ],
    ..wrapper("CDghpRrTtUu", "AbBEeHiKklnPSsVv")
};

static ENV: WrapperSpec = WrapperSpec {
    long_with_arg: &["--unset", "--chdir"],
    long_flags: &[
        "--ignore-environment",
        "--null",
        "--debug",
        "--default-signal",
        "--ignore-signal",
        "--block-signal",
        "--list-signal-handling",
    ],
    skip_assignments: true,
    ..wrapper("uCP", "i0v")
};

static NICE: WrapperSpec = WrapperSpec {
    long_with_arg: &["--adjustment"],
    numeric_options: true,
    ..wrapper("n", "")
};

static TIME: WrapperSpec = WrapperSpec {
    long_with_arg: &["--format", "--output"],
    long_flags: &["--portability", "--verbose", "--append", "--quiet"],
    ..wrapper("fo", "pvaq")
};

static TIMEOUT: WrapperSpec = WrapperSpec {
    long_with_arg: &["--signal", "--kill-after"],
    long_flags: &["--foreground", "--preserve-status", "--verbose"],
    positionals: 1,
    ..wrapper("sk", "fpv")
};

static STDBUF: WrapperSpec = WrapperSpec {
    long_with_arg: &["--input", "--output", "--error"],
    ..wrapper("ioe", "")
};

static SETSID: WrapperSpec = WrapperSpec {
    long_flags: &["--ctty", "--fork", "--wait"],
    ..wrapper("", "cfw")
};

static IONICE: WrapperSpec = WrapperSpec {
    long_with_arg: &["--class", "--classdata", "--pid", "--pgid", "--uid"],
    long_flags: &["--ignore"],
    ..wrapper("cnpPu", "t")
};

static TASKSET: WrapperSpec = WrapperSpec {
    long_flags: &["--all-tasks", "--cpu-list", "--pid"],
    positionals: 1,
    ..wrapper("", "acp")
};

static CHRT: WrapperSpec = WrapperSpec {
    positionals: 1,
    ..wrapper("TPD", "abdefiormpvR")
};

static XARGS: WrapperSpec = WrapperSpec {
    long_with_arg: &[
        "--arg-file",
        "--delimiter",
        "--max-lines",
        "--max-args",
        "--max-procs",
        "--max-chars",
        "--process-slot-var",
    ],
    long_flags: &[
        "--null",
        "--no-run-if-empty",
        "--interactive",
        "--verbose",
        "--exit",
        "--open-tty",
        "--show-limits",
        "--eof",
        "--replace",
    ],
    ..wrapper("adEILnPs", "0eilprtx")
};

static FLOCK: WrapperSpec = WrapperSpec {
    long_with_arg: &["--timeout", "--wait", "--conflict-exit-code"],
    long_flags: &[
        "--shared",
        "--exclusive",
        "--unlock",
        "--nonblock",
        "--nb",
        "--close",
        "--no-fork",
        "--verbose",
    ],
    positionals: 1,
    command_string_short: Some('c'),
    command_string_long: &["--command"],
    ..wrapper("wEc", "sxunoFv")
};

static SU: WrapperSpec = WrapperSpec {
    long_with_arg: &[
        "--group",
        "--supp-group",
        "--shell",
        "--whitelist-environment",
    ],
    long_flags: &["--login", "--preserve-environment", "--pty", "--fast"],
    command_string_short: Some('c'),
    command_string_long: &["--command", "--session-command"],
    // `su [user]`: the positional is a user name; a command comes only from `-c`.
    positionals: usize::MAX,
    ..wrapper("gGswc", "lmpfP")
};

static SCRIPT: WrapperSpec = WrapperSpec {
    command_string_short: Some('c'),
    command_string_long: &["--command"],
    // `script [file]` records a shell; the command it runs comes only from `-c`.
    positionals: usize::MAX,
    ..wrapper("cEOIoBTm", "aefqk")
};

static WATCH: WrapperSpec = WrapperSpec {
    long_with_arg: &["--interval"],
    long_flags: &[
        "--beep",
        "--color",
        "--no-color",
        "--differences",
        "--errexit",
        "--chgexit",
        "--precise",
        "--no-title",
        "--no-wrap",
        "--exec",
    ],
    joins_rest_as_string: true,
    ..wrapper("nq", "bcCdegptwx")
};

static CHROOT: WrapperSpec = WrapperSpec {
    long_with_arg: &["--userspec", "--groups"],
    long_flags: &["--skip-chdir"],
    positionals: 1,
    ..wrapper("", "")
};

static STRACE: WrapperSpec = wrapper("abeEIoOpPsSuUX", "cCdDfFhikqrtTvVwxyYZ");

static PLAIN: WrapperSpec = wrapper("", "");
static DOAS: WrapperSpec = wrapper("uC", "Lns");
static COMMAND: WrapperSpec = wrapper("", "pvV");
static EXEC: WrapperSpec = wrapper("a", "cl");
static CAFFEINATE: WrapperSpec = wrapper("tw", "dimsu");
static PROXYCHAINS: WrapperSpec = wrapper("f", "q");
static PKEXEC: WrapperSpec = WrapperSpec {
    long_with_arg: &["--user"],
    long_flags: &["--disable-internal-agent", "--keep-cwd"],
    ..wrapper("", "")
};

fn wrapper_spec(name: &str) -> Option<&'static WrapperSpec> {
    Some(match name {
        "sudo" => &SUDO,
        "doas" => &DOAS,
        "env" => &ENV,
        "nice" => &NICE,
        "command" => &COMMAND,
        "exec" => &EXEC,
        "time" => &TIME,
        "timeout" => &TIMEOUT,
        "stdbuf" => &STDBUF,
        "setsid" => &SETSID,
        "ionice" => &IONICE,
        "taskset" => &TASKSET,
        "chrt" => &CHRT,
        "xargs" => &XARGS,
        "caffeinate" => &CAFFEINATE,
        "proxychains" | "proxychains4" => &PROXYCHAINS,
        "strace" => &STRACE,
        "flock" => &FLOCK,
        "su" | "runuser" => &SU,
        "script" => &SCRIPT,
        "watch" => &WATCH,
        "chroot" => &CHROOT,
        "pkexec" => &PKEXEC,
        "nohup" | "builtin" | "noglob" | "nocorrect" | "unbuffer" | "catchsegv" | "torsocks"
        | "busybox" | "coproc" => &PLAIN,
        _ => return None,
    })
}

enum Peeled {
    /// The inner command starts at this index.
    Inner(usize),
    /// The wrapper runs no command (`sudo -l`, bare `env`).
    NoCommand,
    /// An option outside the table: the inner command cannot be located.
    Unknown,
}

fn peel_wrapper(spec: &WrapperSpec, args: &[ExpandedWord]) -> Peeled {
    let mut index = 0;
    while index < args.len() {
        let arg = args[index].text.as_str();
        if arg == "--" {
            index += 1;
            break;
        }
        if spec.skip_assignments && is_assignment_text(arg) {
            index += 1;
            continue;
        }
        if spec.skip_assignments && arg == "-" {
            index += 1;
            continue;
        }
        if let Some(long) = arg.strip_prefix("--") {
            if long.is_empty() {
                break;
            }
            let (name, has_value) = match arg.split_once('=') {
                Some((name, _)) => (name, true),
                None => (arg, false),
            };
            if spec.long_with_arg.contains(&name) || spec.command_string_long.contains(&name) {
                index += if has_value { 1 } else { 2 };
                continue;
            }
            if spec.long_flags.contains(&name) {
                index += 1;
                continue;
            }
            return Peeled::Unknown;
        }
        if let Some(cluster) = arg.strip_prefix('-').filter(|cluster| !cluster.is_empty()) {
            if spec.numeric_options && cluster.chars().all(|c| c.is_ascii_digit()) {
                index += 1;
                continue;
            }
            let mut takes_next = false;
            for (offset, c) in cluster.char_indices() {
                if spec.short_with_arg.contains(c) {
                    takes_next = offset + c.len_utf8() == cluster.len();
                    break;
                }
                if !spec.short_flags.contains(c) {
                    return Peeled::Unknown;
                }
            }
            index += if takes_next { 2 } else { 1 };
            continue;
        }
        break;
    }
    let Some(index) = index.checked_add(spec.positionals) else {
        return Peeled::NoCommand;
    };
    if index < args.len() {
        Peeled::Inner(index)
    } else {
        Peeled::NoCommand
    }
}

/// The index of the value of a command-string option (`su -c 'cmd'`), searched across all
/// arguments because such tools permute their options.
fn command_string_option_index(spec: &WrapperSpec, args: &[ExpandedWord]) -> Option<usize> {
    for (index, arg) in args.iter().enumerate() {
        let text = arg.text.as_str();
        if text == "--" {
            return None;
        }
        if spec.command_string_long.contains(&text) {
            return (index + 1 < args.len()).then_some(index + 1);
        }
        let short_option = spec.command_string_short.zip(
            text.strip_prefix('-')
                .filter(|cluster| !cluster.starts_with('-')),
        );
        if short_option.is_some_and(|(short, cluster)| cluster.ends_with(short)) {
            return (index + 1 < args.len()).then_some(index + 1);
        }
    }
    None
}

fn is_assignment_text(text: &str) -> bool {
    let Some((name, _)) = text.split_once('=') else {
        return false;
    };
    let name = name.strip_suffix('+').unwrap_or(name);
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

// -------------------------------------------------------------------------------------------
// Brace expansion
// -------------------------------------------------------------------------------------------

/// Expands unquoted `{a,b}` lists and `{x..y[..step]}` sequences the way bash does, returning
/// `None` if the result would exceed [`MAX_BRACE_EXPANSION`] words.
fn brace_expand(segs: &[Seg]) -> Option<Vec<Vec<Seg>>> {
    let groups = segs
        .iter()
        .filter(|seg| !seg.quoted && seg.ch == '{')
        .count();
    if groups > MAX_BRACE_GROUPS {
        return None;
    }
    let mut results = Vec::new();
    expand_into(segs.to_vec(), &mut results)?;
    Some(results)
}

fn expand_into(segs: Vec<Seg>, results: &mut Vec<Vec<Seg>>) -> Option<()> {
    let is = |seg: &Seg, ch: char| !seg.quoted && seg.ch == ch;
    for open in 0..segs.len() {
        if !is(&segs[open], '{') {
            continue;
        }
        // Find the matching close brace and the top-level commas.
        let mut depth = 0usize;
        let mut commas = Vec::new();
        let mut close = None;
        for (index, seg) in segs.iter().enumerate().skip(open) {
            if is(seg, '{') {
                depth += 1;
            } else if is(seg, '}') {
                depth -= 1;
                if depth == 0 {
                    close = Some(index);
                    break;
                }
            } else if is(seg, ',') && depth == 1 {
                commas.push(index);
            }
        }
        let Some(close) = close else {
            continue;
        };
        let alternatives: Vec<Vec<Seg>> = if !commas.is_empty() {
            let mut bounds = vec![open];
            bounds.extend(commas);
            bounds.push(close);
            bounds
                .windows(2)
                .map(|pair| segs[pair[0] + 1..pair[1]].to_vec())
                .collect()
        } else {
            if segs[open + 1..close].iter().any(|seg| seg.quoted) {
                continue;
            }
            let inner: String = segs[open + 1..close].iter().map(|seg| seg.ch).collect();
            match sequence(&inner) {
                Sequence::Items(items) => items
                    .into_iter()
                    .map(|item| item.chars().map(|ch| Seg::literal(ch, true)).collect())
                    .collect(),
                Sequence::TooLarge => return None,
                Sequence::Literal => continue,
            }
        };
        for alternative in alternatives {
            let mut combined = segs[..open].to_vec();
            combined.extend(alternative);
            combined.extend_from_slice(&segs[close + 1..]);
            expand_into(combined, results)?;
            if results.len() > MAX_BRACE_EXPANSION {
                return None;
            }
        }
        return Some(());
    }
    results.push(segs);
    (results.len() <= MAX_BRACE_EXPANSION).then_some(())
}

enum Sequence {
    Literal,
    TooLarge,
    Items(Vec<String>),
}

/// A brace sequence expression: `1..5`, `a..e`, `10..1..3`.
fn sequence(inner: &str) -> Sequence {
    let parts: Vec<&str> = inner.split("..").collect();
    if !(2..=3).contains(&parts.len()) {
        return Sequence::Literal;
    }
    let step: i128 = match parts.get(2).map(|step| step.parse::<i64>()) {
        Some(Ok(step)) => i128::from(step).abs().max(1),
        Some(Err(_)) => return Sequence::Literal,
        None => 1,
    };
    if let (Ok(start), Ok(end)) = (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
        let (start, end) = (i128::from(start), i128::from(end));
        let count = (start - end).abs() / step + 1;
        if count > MAX_BRACE_EXPANSION as i128 {
            return Sequence::TooLarge;
        }
        let direction = if start <= end { 1 } else { -1 };
        return Sequence::Items(
            (0..count)
                .map(|index| (start + direction * index * step).to_string())
                .collect(),
        );
    }
    let single = |s: &str| {
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) if c.is_ascii_alphabetic() => Some(i128::from(c as u8)),
            _ => None,
        }
    };
    let (Some(start), Some(end)) = (single(parts[0]), single(parts[1])) else {
        return Sequence::Literal;
    };
    let count = (start - end).abs() / step + 1;
    let direction = if start <= end { 1 } else { -1 };
    Sequence::Items(
        (0..count)
            .filter_map(|index| u8::try_from(start + direction * index * step).ok())
            .map(|byte| char::from(byte).to_string())
            .collect(),
    )
}

#[cfg(test)]
#[path = "command_words_test.rs"]
mod tests;
