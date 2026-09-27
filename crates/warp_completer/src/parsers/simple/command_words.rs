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
//! Beyond the shell grammar, it follows programs whose *arguments or environment* are
//! commands: `git -c core.pager=…`/`alias.x=!…`/`rebase --exec`, `GIT_EXTERNAL_DIFF=…` and the
//! other command-valued variables, same-line `alias` definitions (expanded where the alias is
//! used), and `xargs`/`parallel`, whose appended input can itself become a command.
//!
//! # What an unknown program means
//!
//! A program the analysis has no model of is treated as running *itself* with its arguments
//! as data: that is what the denylist, which matches program names, can reason about. Programs
//! that run code *given as an argument or on stdin* break that assumption, so they are either
//! followed (above) or reported unresolved: inline interpreter code (`python -c`, `perl -e`,
//! `node -e`, `ruby -e`, …), `awk` programs that call `system`/pipes, GNU `sed`'s `e`, a shell
//! reading commands from stdin (`bash`, `sh -s`), `LD_PRELOAD`/`BASH_ENV`-style code loading,
//! and `git -c` keys that load further config. What stays out of scope — code in *files*
//! (`bash script.sh`, `make`, `npm run`), remote or container execution (`ssh`, `docker run`),
//! and aliases or functions defined before this command line — is listed on
//! `denylist_match_candidates` in `app/src/ai/blocklist/permissions.rs`.
//!
//! # Shells other than bash
//!
//! The shell is not known here, so where shells disagree the analysis takes the reading that
//! reports more. Comments are analysed as commands too, because zsh without
//! `interactive_comments` runs `echo x # ; rm -rf ~`; `((…))` is also read as commands, for
//! dash and fish; zsh's `=rm` and `;|` are understood; fish's `and`/`or`/`not` are peeled.
//!
//! PowerShell (`EscapeChar::Backtick`) has a different grammar, and reading it with this one
//! can *hide* commands (script blocks `{ … }` as arguments, `@( … )`, method calls, here-
//! strings). So only a simple subset is analysed — pipelines and lists of commands with
//! literal or variable arguments, `$( … )`, `( … )`, `&`/`.` invocation, `$x = <command>` —
//! and any line containing a construct outside it is reported unresolved. Aliases
//! (`ri`, `del`, `erase`, `rd`, `rmdir` for `Remove-Item`, …) and case-insensitive names are
//! added as spellings.

use std::collections::HashSet;

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

/// Longer command lines are still analysed, but reported unresolved: the work is bounded by
/// the caps above, and a policy should not vouch for a line nobody can review.
const MAX_SOURCE_CHARS: usize = 64 * 1024;

/// The most commands recorded; beyond it the result is reported unresolved.
const MAX_COMMANDS: usize = 4096;

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

    /// Whether the line was analysed as PowerShell.
    powershell: bool,

    /// Aliases defined on this line (`alias r=rm`), name to value.
    aliases: Vec<(String, String)>,

    /// Aliases currently being expanded, so `alias ls='ls -la'` does not recurse.
    expanding_aliases: Vec<String>,
}

impl ExecutedCommands {
    /// Whether every command word the line executes was determined statically.
    pub fn is_fully_resolved(&self) -> bool {
        self.unresolved.is_empty()
    }

    /// Every spelling of every executed command that a command-text policy should match:
    /// each command's words joined by single spaces, plus the same with the command word
    /// replaced by each equivalent name the operating system or shell may resolve it to —
    /// the bare program name of a path (`/bin/rm`, `./rm`, `C:\bin\rm.exe`) or of zsh's
    /// `=rm`, the name without a Windows executable extension, the lower-cased name (macOS
    /// and Windows file systems are case-insensitive, so `RM` runs `rm`), `source` for `.`,
    /// and, for PowerShell, every alias of the same cmdlet.
    pub fn policy_spellings(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut spellings: Vec<String> = Vec::new();
        for words in &self.commands {
            let Some((first, rest)) = words.split_first() else {
                continue;
            };
            for name in command_word_spellings(first, self.powershell) {
                let spelling = std::iter::once(name.as_str())
                    .chain(rest.iter().map(String::as_str))
                    .collect::<Vec<_>>()
                    .join(" ");
                if !spelling.is_empty() && seen.insert(spelling.clone()) {
                    spellings.push(spelling);
                }
            }
        }
        spellings
    }
}

/// Every name `word` may run as, `word` itself first.
fn command_word_spellings(word: &str, powershell: bool) -> Vec<String> {
    let mut names = vec![word.to_string()];
    let mut add = |name: String| {
        if !name.is_empty() && !names.contains(&name) {
            names.push(name);
        }
    };
    let bare = program_name(word);
    add(bare.to_string());
    let stripped = strip_executable_extension(bare);
    add(stripped.to_string());
    add(word.to_lowercase());
    let canonical = stripped.to_lowercase();
    add(canonical.clone());
    if canonical == "." {
        add("source".to_string());
    }
    let group = if powershell {
        POWERSHELL_ALIASES
            .iter()
            .find(|group| group.contains(&canonical.as_str()))
    } else {
        None
    };
    for alias in group.into_iter().flat_map(|group| group.iter()) {
        add((*alias).to_string());
    }
    names
}

/// PowerShell's built-in aliases for cmdlets a denylist is likely to name, grouped by cmdlet
/// (lower-cased). Also includes the Unix names PowerShell maps onto them.
const POWERSHELL_ALIASES: &[&[&str]] = &[
    &["remove-item", "rm", "del", "erase", "rd", "ri", "rmdir"],
    &["invoke-expression", "iex"],
    &["start-process", "start", "saps"],
    &["invoke-webrequest", "iwr", "curl", "wget"],
    &["invoke-restmethod", "irm"],
    &["invoke-command", "icm"],
    &["copy-item", "cp", "copy", "cpi"],
    &["move-item", "mv", "move", "mi"],
    &["set-content", "sc"],
    &["stop-process", "kill", "spps"],
    &["invoke-item", "ii"],
    &["remove-itemproperty", "rp"],
];

/// Enumerates every command `source` would execute. See the module docs.
pub fn executed_commands(source: &str, escape_char: EscapeChar) -> ExecutedCommands {
    let mut out = ExecutedCommands {
        powershell: matches!(escape_char, EscapeChar::Backtick),
        ..Default::default()
    };
    if source.chars().count() > MAX_SOURCE_CHARS {
        out.unresolved
            .push("command line too long to vouch for".to_string());
    }
    if matches!(escape_char, EscapeChar::Backslash) && source.trim_start().starts_with('^') {
        // bash/zsh quick substitution (`^old^new`) re-runs an edited history entry.
        out.unresolved
            .push("history quick substitution (`^old^new`)".to_string());
    }
    analyze(source, escape_char, 0, &mut out);
    if out.commands.len() > MAX_COMMANDS {
        out.commands.truncate(MAX_COMMANDS);
        out.unresolved
            .push("too many commands to vouch for".to_string());
    }
    out
}

/// The first PowerShell construct in `source` that the POSIX-shaped grammar here cannot
/// analyse without risking *hiding* a command. Deliberately textual and case-insensitive:
/// a match inside a string literal only costs a confirmation.
fn unanalysed_powershell_construct(source: &str) -> Option<&'static str> {
    const CONSTRUCTS: &[&str] = &[
        // script blocks and hashtables, which run (or hold) code as arguments
        "{",
        "}",
        // .NET type accessors and static calls: [IO.File]::Delete, [scriptblock]::Create
        "[",
        "::",
        // array subexpressions and here-strings
        "@(",
        "@'",
        "@\"",
        // block comments
        "<#",
        // dynamic code and aliasing
        "scriptblock",
        ".invoke",
        "invokescript",
        "newscriptblock",
        "frombase64",
        "set-alias",
        "new-alias",
        "sal ",
        "nal ",
        "add-type",
        "new-object",
        "invoke-command",
        "icm ",
        "start-job",
        "start-threadjob",
        "-stop-parsing",
        "--%",
    ];
    let lower = source.to_lowercase();
    CONSTRUCTS
        .iter()
        .find(|construct| lower.contains(**construct))
        .copied()
}

/// The program a command word names: the last path component (either separator), without
/// zsh's `=cmd` prefix.
fn program_name(word: &str) -> &str {
    let base = word.rsplit(['/', '\\']).next().unwrap_or(word);
    base.strip_prefix('=').unwrap_or(base)
}

/// `name` without a Windows executable extension (`rm.exe` -> `rm`).
fn strip_executable_extension(name: &str) -> &str {
    for extension in [".exe", ".com", ".bat", ".cmd", ".ps1"] {
        if name.len() > extension.len()
            && name.is_char_boundary(name.len() - extension.len())
            && name[name.len() - extension.len()..].eq_ignore_ascii_case(extension)
        {
            return &name[..name.len() - extension.len()];
        }
    }
    name
}

fn analyze(source: &str, escape_char: EscapeChar, depth: usize, out: &mut ExecutedCommands) {
    if depth > MAX_DEPTH {
        out.unresolved
            .push("command nesting is too deep to analyse".to_string());
        return;
    }
    let construct = if matches!(escape_char, EscapeChar::Backtick) {
        unanalysed_powershell_construct(source)
    } else {
        None
    };
    if let Some(construct) = construct {
        let reason = format!("PowerShell construct outside the analysed subset: `{construct}`");
        if !out.unresolved.contains(&reason) {
            out.unresolved.push(reason);
        }
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
    /// The leading assignments themselves: some variables hold commands (`GIT_PAGER=…`).
    assignments: Vec<Word>,
    words: Vec<Word>,
    /// PowerShell: invoked with the call operator `&`, so a computed command word runs.
    invoked: bool,
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

    /// Adds the commands `source` would run if it were a command line, without reporting
    /// what that reading cannot resolve. For text that some shells execute and most do not.
    fn analyze_additively(&mut self, source: &str) {
        if self.depth >= MAX_DEPTH || source.trim().is_empty() {
            return;
        }
        let mut as_commands = ExecutedCommands {
            powershell: self.out.powershell,
            ..Default::default()
        };
        analyze(source, self.escape_char, self.depth + 1, &mut as_commands);
        self.out.commands.extend(as_commands.commands);
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
                    let start = self.pos + 1;
                    while !matches!(self.peek(), None | Some('\n')) {
                        self.pos += 1;
                    }
                    // zsh without `interactive_comments` (its default for interactive
                    // shells) runs `echo x # ; rm -rf ~`, so the comment is also read as
                    // commands. Additive only: what that reading cannot resolve is not
                    // reported, since in most shells a comment runs nothing.
                    let comment = self.slice(start, self.pos);
                    self.analyze_additively(&comment);
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
                    // `;&`, and zsh's `;|`
                    Some('&' | '|') => {
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
                    } else if self.posix
                        && matches!(
                            word.segs.last(),
                            Some(Seg {
                                ch: '@' | '!' | '+' | '*' | '?',
                                quoted: false,
                                ..
                            })
                        )
                    {
                        // extglob pattern `@( … )` and friends.
                        self.pos += 1;
                        self.scan_balanced('(', ')', 1);
                        let text = self.slice(start, self.pos);
                        for ch in text.chars() {
                            word.push(ch, true);
                        }
                        word.glob = true;
                    } else {
                        if !self.posix && !word.segs.is_empty() {
                            // `$file.Delete()`, `Foo(…)`: a .NET method or function call, which
                            // can do anything and names no command.
                            self.unresolved("PowerShell method call");
                        }
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
        let body = self.slice(body_start, body_end);
        self.analyze_additively(&body);
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
        let mut after_coproc = false;

        loop {
            let tok = self.next_token();
            let is_newline = matches!(tok, Tok::Newline);
            let is_call_operator = !self.posix && matches!(tok, Tok::Op(Op::Amp));
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
                        if std::mem::take(&mut after_coproc)
                            && !word.is_plain("{")
                            && matches!(self.peek_nonblank(), Some('{' | '('))
                        {
                            // `coproc NAME { …; }`: NAME names the coprocess.
                            continue;
                        }
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
                                "coproc" => after_coproc = true,
                                // then, else, elif, do, !
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
                            cmd.assignments.push(word);
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
                    // PowerShell's `&` is the call operator, not a background separator.
                    cmd.invoked = is_call_operator;
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
                        if cmd.invoked {
                            self.unresolved("PowerShell invocation of a computed command");
                            cmd.invoked = false;
                        }
                        if self.posix && self.peek() == Some('(') {
                            // Arithmetic command `(( … ))`.
                            self.pos += 1;
                            self.scan_arithmetic();
                        } else {
                            self.parse_nested_paren();
                        }
                    }
                    Mode::Arguments
                        if self.posix
                            && cmd.prefix == 0
                            && cmd.words.len() == 1
                            && self.peek_nonblank() == Some(')') =>
                    {
                        // Function definition `name() …`: the name is not executed here, the
                        // body is parsed as ordinary commands below.
                        cmd.words.clear();
                        while !matches!(self.bump(), Some(')') | None) {}
                        mode = Mode::CommandStart;
                    }
                    // PowerShell grouping `( … )` inside an argument list.
                    _ if !self.posix => self.parse_nested_paren(),
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
        for assignment in &command.assignments {
            self.check_assignment(&assignment.text(), assignment.dynamic);
        }
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
        let context = EmitContext {
            invoked: command.invoked,
            ..EmitContext::default()
        };
        self.emit(&words, context);
    }

    fn record(&mut self, words: &[ExpandedWord]) {
        if self.out.commands.len() <= MAX_COMMANDS {
            self.out
                .commands
                .push(words.iter().map(|word| word.text.clone()).collect());
        }
    }

    /// Records `words` as an executed command, then follows any command it runs in turn.
    fn emit(&mut self, words: &[ExpandedWord], context: EmitContext) {
        let Some(first) = words.first() else {
            return;
        };
        if context.hops > MAX_DEPTH {
            self.unresolved("command wrappers nested too deeply to analyse");
            return;
        }
        self.record(words);
        let args = &words[1..];
        let inner = context.inner();

        if !self.posix && first.text.starts_with('$') {
            if args.first().is_some_and(|arg| {
                matches!(
                    arg.text.as_str(),
                    "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "??="
                )
            }) {
                // PowerShell assignment `$x = <pipeline>`: the right-hand side is a command.
                self.emit(&args[1..], inner);
            } else if context.invoked {
                self.unresolved(format!(
                    "PowerShell invocation of a computed command `{}`",
                    first.text
                ));
            }
            // Otherwise an expression (`$x`, `$x -eq 1`), which runs no command by itself.
            return;
        }

        if first.name_unknown {
            self.unresolved(format!(
                "command word `{}` is only known at run time",
                first.text
            ));
            return;
        }
        if !first.text.is_ascii() {
            // Zero-width, bidi and look-alike characters make a name read as one program and
            // run as another (or as nothing); no policy can vouch for it by text.
            self.unresolved(format!(
                "command word `{}` contains non-ASCII characters",
                first.text.escape_default()
            ));
        }

        let name = strip_executable_extension(program_name(&first.text)).to_ascii_lowercase();

        // An alias defined earlier on this line (`alias r=rm; r -rf ~`).
        let alias_value = self
            .out
            .aliases
            .iter()
            .rev()
            .find(|(alias, _)| *alias == first.text)
            .map(|(_, value)| value.clone());
        let alias_value = alias_value.filter(|_| !self.out.expanding_aliases.contains(&first.text));
        if let Some(value) = alias_value {
            let source = std::iter::once(value.as_str())
                .chain(args.iter().map(|arg| arg.text.as_str()))
                .collect::<Vec<_>>()
                .join(" ");
            self.out.expanding_aliases.push(first.text.clone());
            self.analyze_nested(&source, self.escape_char);
            self.out.expanding_aliases.pop();
            // An alias takes precedence over builtins and programs of the same name.
            return;
        }

        match name.as_str() {
            "eval" | "iex" | "invoke-expression" => self.analyze_args_as_command(args),
            "sh" | "bash" | "zsh" | "dash" | "ksh" | "mksh" | "ash" | "yash" | "fish" | "csh"
            | "tcsh" | "pwsh" | "powershell" | "cmd" => self.emit_shell(&name, args),
            "find" => self.emit_find_exec(args, inner),
            "git" => self.emit_git(args, inner),
            "alias" => {
                for arg in args {
                    if let Some((alias, value)) = arg.text.split_once('=') {
                        if arg.dynamic {
                            self.unresolved("alias with a value only known at run time");
                        } else {
                            self.out
                                .aliases
                                .push((alias.to_string(), value.to_string()));
                            self.analyze_nested(value, self.escape_char);
                        }
                    }
                }
            }
            "export" | "declare" | "typeset" | "local" | "readonly" => {
                for arg in args {
                    self.check_assignment(&arg.text, arg.dynamic);
                }
            }
            "hash" if args.iter().any(|arg| arg.text.starts_with("-p")) => {
                self.unresolved("`hash -p` rebinds a command name to another program");
            }
            "fc" | "r" if self.posix => {
                self.unresolved("re-runs a command from history");
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
            "." | "source" if !self.posix => {
                if args.first().is_some_and(|arg| arg.name_unknown) {
                    self.unresolved("PowerShell dot-sourcing of a computed command");
                }
            }
            "start-process" | "start" | "saps" if !self.posix => {
                self.emit_start_process(args, inner);
            }
            "parallel" => {
                // GNU parallel builds each command from its template *and its input*, runs it
                // through a shell, and has an option grammar of its own: the template is
                // offered as candidates, but the commands cannot be vouched for.
                let end = args
                    .iter()
                    .position(|arg| arg.text.starts_with(":::"))
                    .unwrap_or(args.len());
                self.analyze_additively(
                    &args[..end]
                        .iter()
                        .map(|arg| arg.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                self.unresolved("`parallel` builds its commands from its input");
            }
            "sed" | "gsed" if sed_executes(args) => {
                self.unresolved("`sed` script runs commands (`e`)");
            }
            "awk" | "gawk" | "mawk" | "nawk" if awk_executes(args) => {
                self.unresolved("`awk` program runs commands (`system` or a pipe)");
            }
            "vim" | "vi" | "nvim" | "ex" | "view" | "gvim"
                if args.iter().any(|arg| {
                    arg.text.starts_with('+') || matches!(arg.text.as_str(), "-c" | "--cmd" | "-S")
                }) =>
            {
                self.unresolved("editor commands given on the command line");
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
            _ if runs_inline_code(&name, args) => {
                self.unresolved(format!("`{name}` runs code given inline or on its input"));
            }
            _ => {
                let Some(spec) = wrapper_spec(&name) else {
                    return;
                };
                // Command strings given by option (`su -c …`, `flock -c …`, `script -c …`).
                if let Some(index) = command_string_option_index(spec, args) {
                    self.analyze_args_as_command(&args[index..index + 1]);
                }
                if spec.skip_assignments {
                    for arg in args.iter().take_while(|arg| {
                        arg.text.starts_with('-') || is_assignment_text(&arg.text)
                    }) {
                        self.check_assignment(&arg.text, arg.dynamic);
                    }
                }
                match peel_wrapper(spec, args) {
                    Peeled::Inner(index) => {
                        if spec.joins_rest_as_string {
                            self.analyze_args_as_command(&args[index..]);
                        } else if name == "xargs" {
                            // xargs appends its input to the command, and substitutes it for
                            // the `-I` replace string: those words are only known at run time.
                            let replace = xargs_replace_string(&args[..index]);
                            let command = args[index..]
                                .iter()
                                .map(|arg| mark_placeholder(arg, replace.as_deref()))
                                .collect::<Vec<_>>();
                            self.emit(
                                &command,
                                EmitContext {
                                    stdin_args: true,
                                    ..inner
                                },
                            );
                        } else {
                            self.emit(&args[index..], inner);
                        }
                    }
                    Peeled::NoCommand => {
                        if context.stdin_args && name != "xargs" {
                            // `… | xargs env`: the appended input *is* the command.
                            self.unresolved(format!(
                                "`{name}` would run a command taken from xargs input"
                            ));
                        } else if context.stdin_args {
                            self.unresolved("nested xargs runs a command taken from its input");
                        }
                    }
                    Peeled::Unknown => {
                        // An option this table does not know: the inner command word cannot be
                        // located, so every suffix is offered to the policy as a candidate.
                        if args.len() > MAX_WRAPPER_SUFFIXES {
                            self.unresolved("wrapper options could not be parsed");
                        }
                        for index in 0..args.len().min(MAX_WRAPPER_SUFFIXES) {
                            self.record(&args[index..]);
                        }
                    }
                }
            }
        }
    }

    /// `sh -c …` and friends: the command string is analysed; a shell that would read its
    /// commands from stdin, or PowerShell's encoded command, cannot be vouched for.
    fn emit_shell(&mut self, name: &str, args: &[ExpandedWord]) {
        let powershell = matches!(name, "pwsh" | "powershell");
        if powershell
            && args.iter().any(|arg| {
                let lower = arg.text.to_ascii_lowercase();
                lower == "-ec" || (lower.len() >= 3 && "-encodedcommand".starts_with(&lower))
            })
        {
            self.unresolved("PowerShell encoded command");
            return;
        }
        if let Some(index) = shell_command_string_index(args) {
            // POSIX shells take exactly one command string (later words are `$0`, `$1`, …);
            // PowerShell and cmd run the rest of the line.
            let (command_words, escape_char) = match name {
                "pwsh" | "powershell" => (&args[index..], EscapeChar::Backtick),
                "cmd" => (&args[index..], EscapeChar::Backslash),
                _ => (&args[index..index + 1], EscapeChar::Backslash),
            };
            self.analyze_args_as_command_with(command_words, escape_char);
        } else if shell_reads_stdin(args) {
            self.unresolved(format!("`{name}` would read its commands from stdin"));
        }
    }

    /// PowerShell `Start-Process <file> [-ArgumentList …]`.
    fn emit_start_process(&mut self, args: &[ExpandedWord], context: EmitContext) {
        let mut target = None;
        let mut index = 0;
        while index < args.len() {
            let lower = args[index].text.to_ascii_lowercase();
            if lower.len() >= 2 && "-filepath".starts_with(&lower) {
                target = args.get(index + 1).map(|_| index + 1);
                break;
            }
            if !lower.starts_with('-') {
                target = Some(index);
                break;
            }
            index += 1;
        }
        let Some(target) = target else {
            self.unresolved("`Start-Process` without a recognisable file");
            return;
        };
        let command = std::iter::once(args[target].clone())
            .chain(
                args.iter()
                    .enumerate()
                    .filter(|(index, _)| *index != target)
                    .map(|(_, arg)| arg.clone()),
            )
            .collect::<Vec<_>>();
        self.emit(
            &command,
            EmitContext {
                invoked: true,
                ..context
            },
        );
    }

    /// `git`: config that names commands (`-c core.pager=…`, `alias.x=!…`, `git config …`),
    /// and subcommands that run commands (`rebase --exec`, `bisect run`, `submodule foreach`,
    /// `filter-branch --*-filter`, `difftool -x`).
    fn emit_git(&mut self, args: &[ExpandedWord], context: EmitContext) {
        let mut index = 0;
        while index < args.len() {
            let text = args[index].text.as_str();
            if text == "-c" {
                if let Some(pair) = args.get(index + 1) {
                    self.check_git_config(&pair.text, pair.dynamic);
                }
                index += 2;
                continue;
            }
            if text.starts_with("--config-env") || text.starts_with("--exec-path=") {
                self.unresolved(format!("`git {text}` takes code from elsewhere"));
            }
            if matches!(
                text,
                "-C" | "--git-dir" | "--work-tree" | "--namespace" | "--super-prefix"
            ) {
                index += 2;
                continue;
            }
            if text.starts_with('-') {
                index += 1;
                continue;
            }
            break;
        }
        let Some((subcommand, rest)) = args[index.min(args.len())..].split_first() else {
            return;
        };
        match subcommand.text.as_str() {
            "config" => {
                let operands = rest
                    .iter()
                    .filter(|arg| !arg.text.starts_with('-'))
                    .collect::<Vec<_>>();
                if let [key, value, ..] = operands.as_slice() {
                    self.check_git_config(
                        &format!("{}={}", key.text, value.text),
                        key.dynamic || value.dynamic,
                    );
                }
            }
            "bisect" if rest.first().is_some_and(|arg| arg.text == "run") => {
                self.emit(&rest[1..], context);
            }
            "submodule" => {
                if let Some(position) = rest.iter().position(|arg| arg.text == "foreach") {
                    let command = rest[position + 1..]
                        .iter()
                        .skip_while(|arg| arg.text.starts_with('-'))
                        .cloned()
                        .collect::<Vec<_>>();
                    self.analyze_args_as_command(&command);
                }
            }
            _ => {
                // `rebase --exec`, `difftool -x/--extcmd`, `filter-branch --*-filter`.
                let mut position = 0;
                while position < rest.len() {
                    let text = rest[position].text.as_str();
                    let (option, attached) = match text.split_once('=') {
                        Some((option, value)) => (option, Some(value)),
                        None => (text, None),
                    };
                    let runs_command = matches!(option, "--exec" | "-x" | "--extcmd")
                        || (option.starts_with("--") && option.ends_with("-filter"));
                    if runs_command {
                        let value = match attached {
                            Some(value) => Some((value.to_string(), rest[position].dynamic)),
                            None => rest
                                .get(position + 1)
                                .map(|arg| (arg.text.clone(), arg.dynamic)),
                        };
                        if let Some((value, dynamic)) = value {
                            if dynamic {
                                self.unresolved("git command option only known at run time");
                            }
                            self.analyze_nested(&value, self.escape_char);
                        }
                    }
                    position += 1;
                }
            }
        }
    }

    /// A `key=value` git config pair: analyses values that git runs as commands, and fails
    /// closed on keys that load further config or hooks.
    fn check_git_config(&mut self, pair: &str, dynamic: bool) {
        let Some((key, value)) = pair.split_once('=') else {
            return;
        };
        let key = key.to_ascii_lowercase();
        if key == "include.path" || key.starts_with("includeif.") || key == "core.hookspath" {
            self.unresolved(format!("git config `{key}` loads code from files"));
            return;
        }
        if !is_git_command_key(&key) {
            return;
        }
        let command = if key.starts_with("alias.") || key.contains("credential") {
            // Aliases and credential helpers run a shell command only with a leading `!`.
            match value.strip_prefix('!') {
                Some(command) => command,
                None if key.starts_with("alias.") => return,
                None => value,
            }
        } else {
            value
        };
        if dynamic {
            self.unresolved(format!("git config `{key}` only known at run time"));
        }
        self.analyze_nested(command, self.escape_char);
    }

    /// A `NAME=value` assignment, as a command prefix, standalone, or via `export`/`env`:
    /// some variables hold commands that programs run, and some load code.
    fn check_assignment(&mut self, text: &str, dynamic: bool) {
        let Some((name, value)) = text.split_once('=') else {
            return;
        };
        let name = name.strip_suffix('+').unwrap_or(name);
        if COMMAND_VARIABLES.contains(&name) {
            if dynamic {
                self.unresolved(format!("`{name}` holds a command only known at run time"));
            }
            self.analyze_nested(value, self.escape_char);
        } else if PROMPT_VARIABLES.contains(&name) {
            // Expanded like a double-quoted string at every prompt.
            if self.depth < MAX_DEPTH {
                let mut prompt =
                    Analyzer::new(value, self.escape_char, self.depth + 1, &mut *self.out);
                prompt.scan_heredoc_body();
            }
        } else if CODE_LOADING_VARIABLES.contains(&name)
            || name.starts_with("GIT_CONFIG_KEY_")
            || name.starts_with("GIT_CONFIG_VALUE_")
        {
            self.unresolved(format!("`{name}` makes programs load code"));
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

    /// `find … -exec cmd … ;` (and `-execdir`, `-ok`, `-okdir`) runs `cmd`, with each found
    /// path substituted for `{}`.
    fn emit_find_exec(&mut self, args: &[ExpandedWord], context: EmitContext) {
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
                let command = args[start..end]
                    .iter()
                    .map(|arg| mark_placeholder(arg, None))
                    .collect::<Vec<_>>();
                self.emit(&command, context);
                index = end;
            }
            index += 1;
        }
    }
}

/// How a command was reached, for [`Analyzer::emit`].
#[derive(Clone, Copy, Default)]
struct EmitContext {
    /// Wrapper hops so far.
    hops: usize,
    /// Reached through `xargs`, which appends its input as further words.
    stdin_args: bool,
    /// PowerShell: invoked with `&`, or as a wrapper's inner command.
    invoked: bool,
}

impl EmitContext {
    /// The context of a command run by this one.
    fn inner(self) -> Self {
        Self {
            hops: self.hops + 1,
            invoked: true,
            ..self
        }
    }
}

/// `arg`, marked as only known at run time if it contains the substitution placeholder
/// (`{}`, or xargs' `-I` replace string).
fn mark_placeholder(arg: &ExpandedWord, replace: Option<&str>) -> ExpandedWord {
    let placeholder =
        arg.text.contains("{}") || replace.is_some_and(|replace| arg.text.contains(replace));
    ExpandedWord {
        dynamic: arg.dynamic || placeholder,
        name_unknown: arg.name_unknown || placeholder,
        ..arg.clone()
    }
}

/// xargs' replace string, from `-I R`, `-IR`, `-i[R]` or `--replace[=R]`.
fn xargs_replace_string(options: &[ExpandedWord]) -> Option<String> {
    for (index, option) in options.iter().enumerate() {
        let text = option.text.as_str();
        if text == "-I" {
            return options.get(index + 1).map(|value| value.text.clone());
        }
        if let Some(value) = text
            .strip_prefix("-I")
            .or_else(|| text.strip_prefix("--replace="))
        {
            return Some(value.to_string());
        }
        if text == "-i" || text == "--replace" {
            return Some("{}".to_string());
        }
        if let Some(value) = text.strip_prefix("-i") {
            return Some(value.to_string());
        }
    }
    None
}

/// Variables whose value a program runs as a command.
const COMMAND_VARIABLES: &[&str] = &[
    "GIT_EXTERNAL_DIFF",
    "GIT_PAGER",
    "PAGER",
    "GIT_EDITOR",
    "EDITOR",
    "VISUAL",
    "GIT_SEQUENCE_EDITOR",
    "GIT_SSH_COMMAND",
    "GIT_SSH",
    "GIT_ASKPASS",
    "SSH_ASKPASS",
    "SUDO_ASKPASS",
    "GIT_PROXY_COMMAND",
    "MANPAGER",
    "SYSTEMD_PAGER",
    "BROWSER",
    "LESSOPEN",
    "LESSCLOSE",
    "PROMPT_COMMAND",
    "FCEDIT",
];

/// Prompt variables, expanded (`$( … )` included) every time a prompt is shown.
const PROMPT_VARIABLES: &[&str] = &["PS0", "PS1", "PS2", "PS4", "PROMPT", "RPROMPT"];

/// Variables that make programs load code the command line does not show.
const CODE_LOADING_VARIABLES: &[&str] = &[
    "LD_PRELOAD",
    "LD_AUDIT",
    "DYLD_INSERT_LIBRARIES",
    "BASH_ENV",
    "ENV",
    "ZDOTDIR",
    "PERL5OPT",
    "RUBYOPT",
    "NODE_OPTIONS",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG",
    "GIT_EXEC_PATH",
    "GIT_TEMPLATE_DIR",
];

/// Git config keys whose value git runs as a command (lower-cased).
fn is_git_command_key(key: &str) -> bool {
    const EXACT: &[&str] = &[
        "core.pager",
        "core.editor",
        "core.sshcommand",
        "core.fsmonitor",
        "core.askpass",
        "core.gitproxy",
        "diff.external",
        "sequence.editor",
        "gpg.program",
        "uploadpack.packobjectshook",
        "credential.helper",
        "web.browser",
        "sendemail.sendmailcmd",
    ];
    const SUFFIXES: &[&str] = &[
        ".cmd",
        ".command",
        ".textconv",
        ".driver",
        ".clean",
        ".smudge",
        ".process",
        ".program",
        ".helper",
        ".uploadpack",
        ".receivepack",
        ".sshcommand",
    ];
    EXACT.contains(&key)
        || key.starts_with("alias.")
        || key.starts_with("pager.")
        || SUFFIXES.iter().any(|suffix| key.ends_with(suffix))
}

/// Whether a shell given `args` (and no `-c` string) reads its commands from stdin.
fn shell_reads_stdin(args: &[ExpandedWord]) -> bool {
    let mut index = 0;
    while index < args.len() {
        let text = args[index].text.as_str();
        if matches!(text, "--version" | "--help" | "-v" | "-V" | "/?") {
            return false;
        }
        if matches!(text, "-o" | "+o" | "-O" | "+O" | "--rcfile" | "--init-file") {
            index += 2;
            continue;
        }
        if text.len() > 1 && (text.starts_with('-') || text.starts_with('+')) {
            if !text.starts_with("--") && text[1..].contains('s') {
                return true;
            }
            index += 1;
            continue;
        }
        if text == "-" {
            return true;
        }
        // A script file.
        return false;
    }
    true
}

/// Whether an interpreter runs code given inline (`python -c`, `perl -e`, `node -e`, …) or
/// read from stdin (no script operand).
fn runs_inline_code(name: &str, args: &[ExpandedWord]) -> bool {
    // (inline-code short options, long options, option letters that take a value)
    let (short, long, with_value): (&str, &[&str], &str) =
        if name.starts_with("python") || name.starts_with("pypy") || name == "ipython" {
            ("c", &[], "WXQ")
        } else {
            match name {
                "perl" => ("eE", &[], "IMm"),
                "ruby" => ("e", &[], "IrCEx"),
                "node" | "nodejs" | "bun" => ("ep", &["--eval", "--print"], "r"),
                "deno" => return args.first().is_some_and(|arg| arg.text == "eval"),
                "php" => ("rBRE", &[], "cdzf"),
                "lua" | "luajit" => ("e", &[], "l"),
                "osascript" | "rscript" | "julia" | "groovy" | "expect" => ("ec", &["--eval"], ""),
                "gdb" => {
                    return args.iter().any(|arg| {
                        matches!(arg.text.as_str(), "-ex" | "-x" | "--eval-command" | "-iex")
                    });
                }
                _ => return false,
            }
        };
    let mut index = 0;
    while index < args.len() {
        let text = args[index].text.as_str();
        if matches!(text, "--version" | "--help" | "-V" | "-h") {
            return false;
        }
        if text == "-" {
            return true;
        }
        if text == "--" || !text.starts_with('-') {
            // A script (or module) operand: its code is in a file.
            return false;
        }
        if let Some(option) = text.strip_prefix("--") {
            let option = option.split('=').next().unwrap_or(option);
            if long.iter().any(|long| long[2..] == *option) {
                return true;
            }
            index += 1;
            continue;
        }
        let cluster = &text[1..];
        if name.starts_with("python") && cluster.starts_with('m') {
            // `python -m module`: code from an installed module.
            return false;
        }
        for (offset, c) in cluster.char_indices() {
            if short.contains(c) {
                return true;
            }
            if with_value.contains(c) {
                if offset + c.len_utf8() == cluster.len() {
                    index += 1;
                }
                break;
            }
        }
        index += 1;
    }
    // No script operand: the code comes from stdin (or an interactive session fed by it).
    true
}

/// Whether a `sed` invocation's script uses GNU sed's `e` command or `s///e` flag.
fn sed_executes(args: &[ExpandedWord]) -> bool {
    let mut scripts = Vec::new();
    let mut explicit = false;
    let mut index = 0;
    let mut first_operand = None;
    while index < args.len() {
        let text = args[index].text.as_str();
        if matches!(text, "-e" | "--expression") {
            explicit = true;
            if let Some(script) = args.get(index + 1) {
                scripts.push(script.text.as_str());
            }
            index += 2;
            continue;
        }
        if let Some(script) = text.strip_prefix("--expression=") {
            explicit = true;
            scripts.push(script);
        } else if text.starts_with("-f") || text.starts_with("--file") {
            explicit = true;
        } else if !text.starts_with('-') && first_operand.is_none() {
            first_operand = Some(text);
        }
        index += 1;
    }
    if !explicit {
        scripts.extend(first_operand);
    }
    scripts.iter().any(|script| sed_script_executes(script))
}

fn sed_script_executes(script: &str) -> bool {
    let chars: Vec<char> = script.chars().collect();
    let n = chars.len();
    // Skips `count` delimiter-terminated sections, honouring backslash escapes.
    let skip_delimited = |i: &mut usize, delimiter: char, count: usize| {
        for _ in 0..count {
            while *i < n && chars[*i] != delimiter {
                if chars[*i] == '\\' {
                    *i += 1;
                }
                *i += 1;
            }
            *i += 1;
        }
    };
    let mut i = 0;
    while i < n {
        while i < n && (chars[i].is_whitespace() || matches!(chars[i], ';' | '{' | '}')) {
            i += 1;
        }
        // Addresses: line numbers, `$`, `/regex/`, `\cregexc`, ranges and negation.
        loop {
            match chars.get(i) {
                Some(c) if c.is_ascii_digit() || matches!(c, '$' | ',' | '~' | '+' | '!' | ' ') => {
                    i += 1
                }
                Some('/') => {
                    i += 1;
                    skip_delimited(&mut i, '/', 1);
                    while matches!(chars.get(i), Some('I' | 'M')) {
                        i += 1;
                    }
                }
                Some('\\') => {
                    i += 1;
                    let delimiter = chars.get(i).copied();
                    i += 1;
                    if let Some(delimiter) = delimiter {
                        skip_delimited(&mut i, delimiter, 1);
                    }
                }
                _ => break,
            }
        }
        let Some(&command) = chars.get(i) else {
            break;
        };
        i += 1;
        match command {
            'e' => return true,
            's' | 'y' => {
                let Some(&delimiter) = chars.get(i) else {
                    break;
                };
                i += 1;
                skip_delimited(&mut i, delimiter, 2);
                if command == 's' {
                    while i < n && !matches!(chars[i], ';' | '\n' | '}') {
                        match chars[i] {
                            'e' => return true,
                            // `w file` takes the rest of the line.
                            'w' => {
                                while i < n && chars[i] != '\n' {
                                    i += 1;
                                }
                            }
                            _ => i += 1,
                        }
                    }
                }
            }
            'a' | 'i' | 'c' | 'r' | 'R' | 'w' | 'W' => {
                while i < n && chars[i] != '\n' {
                    i += 1;
                }
            }
            'b' | 't' | 'T' | ':' => {
                while i < n && !matches!(chars[i], ';' | '\n') {
                    i += 1;
                }
            }
            _ => {}
        }
    }
    false
}

/// Whether an `awk` program calls `system()` or runs a command through a pipe.
fn awk_executes(args: &[ExpandedWord]) -> bool {
    let mut index = 0;
    while index < args.len() {
        let text = args[index].text.as_str();
        if matches!(text, "-f" | "--file") {
            // Program in a file: out of scope, like any script.
            return false;
        }
        if matches!(text, "-F" | "-v" | "--assign" | "--field-separator") {
            index += 2;
            continue;
        }
        if matches!(text, "-e" | "--source") {
            return args
                .get(index + 1)
                .is_some_and(|program| awk_program_executes(&program.text));
        }
        if text.starts_with('-') && text != "-" {
            index += 1;
            continue;
        }
        return awk_program_executes(text);
    }
    false
}

fn awk_program_executes(program: &str) -> bool {
    program.contains("system") || program.contains('|')
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
/// zsh `repeat N cmd`.
static REPEAT: WrapperSpec = WrapperSpec {
    positionals: 1,
    ..wrapper("", "")
};
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
        "repeat" => &REPEAT,
        // zsh's `- cmd`, and fish's `and`/`or`/`not`, run the command that follows.
        "-" | "and" | "or" | "not" => &PLAIN,
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
