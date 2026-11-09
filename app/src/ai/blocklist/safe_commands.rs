//! The safe read-only command allowlist (OFF by default; `AISettings::safe_command_allowlist_*`).
//!
//! A user-configurable list of read-only commands the agent may run without asking: `ls`, `cat`,
//! `grep`, `git status`, `df` and so on. It sits beside (not instead of) the regex allowlist and
//! denylist of the execution profile: `BlocklistAIPermissions::can_autoexecute_command` consults it
//! only after the denylist and the command-word analysis, and only when the profile would
//! otherwise ask.
//!
//! The matcher is deliberately narrow, because a wrong "yes" runs a command with no human in the
//! loop and a wrong "no" costs one Enter:
//!
//! * The whole line is first screened as TEXT: any character that lets a shell do something other
//!   than run one program with literal words (`| & ; < > ( ) { } ` $ \ * ? [ ] ! ~ # " '`, a
//!   newline, a control character, anything non-ASCII) rejects it. So no pipes, redirects,
//!   `;` / `&&` / `||`, subshells, substitutions, globs, expansions, quoting tricks or comments.
//! * The first word is the program, spelled exactly (no `/`, so no path lookalike and no
//!   `./ls`), and must equal an allowlist entry's first word. An entry may carry sub-words
//!   (`git status`): those must follow immediately, so `git -c core.pager=x status` never matches
//!   `git status`.
//! * Every other word must be a plain word or a flag; per-program rules then refuse the flags
//!   that write, run something or never end (`sort -o`, `tail -f`, `rg --pre`, `date -s`,
//!   `git diff --output`, ...). A program the rules do not know gets only the generic screen.
//!
//! What this cannot promise: a program that runs configured helpers (git's `core.fsmonitor`,
//! `diff.external`, textconv) can run what a repository's config names. The default list keeps to
//! the commands where that is the only way in, and the setting documents it.

/// The entries the list starts with when the setting is first turned on. Each is `program` or
/// `program subcommand`.
pub const DEFAULT_SAFE_COMMANDS: &[&str] = &[
    "ls",
    "cat",
    "head",
    "tail",
    "wc",
    "grep",
    "rg",
    "pwd",
    "whoami",
    "id",
    "uname",
    "df",
    "du",
    "stat",
    "file",
    "which",
    "basename",
    "dirname",
    "realpath",
    "tree",
    "uptime",
    "free",
    "git status",
    "git log",
    "git diff",
    "git show",
    "git rev-parse",
    "git ls-files",
];

/// The default list as owned strings, for the setting's default value.
pub fn default_safe_commands() -> Vec<String> {
    DEFAULT_SAFE_COMMANDS
        .iter()
        .map(|s| (*s).to_owned())
        .collect()
}

/// Whether `entry` is a well-formed list entry: one to three words of `[A-Za-z0-9._+-]`, the first
/// not starting with `-` or `.` and not containing `/`. The settings UI refuses anything else, and
/// the matcher ignores it.
pub fn is_valid_entry(entry: &str) -> bool {
    let words: Vec<&str> = entry.split_whitespace().collect();
    !words.is_empty()
        && words.len() <= 3
        && words.iter().all(|w| is_plain_word(w))
        && !words[0].starts_with(['-', '.'])
}

fn is_plain_word(word: &str) -> bool {
    !word.is_empty()
        && word
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'+' | b'-'))
}

/// Characters that make a line more than "one program, literal words". Anything outside printable
/// ASCII is refused too (checked separately).
const SHELL_ACTIVE: &[char] = &[
    '|', '&', ';', '<', '>', '(', ')', '{', '}', '`', '$', '\\', '*', '?', '[', ']', '!', '~', '#',
    '"', '\'', '=', '%', '^', '@', ',', ':',
];

/// Whether `command` is a read-only command the allowlist `entries` vouches for.
pub fn matches_allowlist(command: &str, entries: &[String]) -> bool {
    let command = command.trim();
    if command.is_empty()
        || command
            .chars()
            .any(|c| !(c == ' ' || c.is_ascii_graphic()) || SHELL_ACTIVE.contains(&c))
    {
        // `=` and `+` are refused in the line-wide screen even though some safe flags use them
        // (`--color=auto`): the cost is one approval, the gain is no `VAR=x prog`, no
        // `--output=file` and no `+` date formats to reason about.
        return false;
    }
    let words: Vec<&str> = command.split(' ').filter(|w| !w.is_empty()).collect();
    let Some(program) = words.first() else {
        return false;
    };
    if !is_plain_word(program) || program.starts_with(['-', '.']) {
        return false;
    }
    entries
        .iter()
        .filter(|entry| is_valid_entry(entry))
        .any(|entry| {
            let entry_words: Vec<&str> = entry.split_whitespace().collect();
            words.len() >= entry_words.len()
                && words[..entry_words.len()] == entry_words[..]
                && args_are_safe(
                    program,
                    &words[entry_words.len()..],
                    &words[1..entry_words.len()],
                )
        })
}

/// Per-program refusals. `args` are the words after the entry's own words; `subcommand` is any
/// words of the entry between the program and `args` (`git`'s `status`).
fn args_are_safe(program: &str, args: &[&str], subcommand: &[&str]) -> bool {
    let flag_in = |args: &[&str], bad: &[&str]| {
        args.iter().any(|a| {
            bad.iter().any(|b| {
                a == b
                    // A bundle of short flags (`-fn`) contains the bad short flag.
                    || (b.len() == 2
                        && b.starts_with('-')
                        && a.starts_with('-')
                        && !a.starts_with("--")
                        && a[1..].contains(&b[1..]))
            })
        })
    };
    // `--` ends flags in every tool; what follows is a name, which is plain by the line screen.
    match program {
        // `-f` / `-F` follow forever: the command never ends and the agent hangs on it.
        "tail" => !flag_in(args, &["-f", "-F", "--follow", "--retry"]),
        "rg" | "ripgrep" => !flag_in(args, &["--pre", "--pre-glob", "--hostname-bin", "-z"]),
        "date" => !flag_in(args, &["-s", "--set"]) && args.iter().all(|a| a.starts_with('-')),
        "sort" => !flag_in(args, &["-o", "--output", "--compress-program"]),
        "file" => !flag_in(args, &["-C", "--compile", "-m", "--magic-file"]),
        "tree" => !flag_in(args, &["-o", "--output"]),
        "hostname" | "uniq" => args.is_empty(),
        "git" => {
            let sub = subcommand.first().copied().unwrap_or("");
            if !matches!(
                sub,
                "status" | "log" | "diff" | "show" | "rev-parse" | "ls-files"
            ) {
                // An entry like plain `git` or `git branch` would let `git branch name` create a
                // branch; only the read-only subcommands are understood.
                return false;
            }
            !args.iter().any(|a| {
                a.starts_with("--output")
                    || a.starts_with("--ext-diff")
                    || a.starts_with("--open-files-in-pager")
                    || a.starts_with("--exec-path")
                    || *a == "-O"
                    || *a == "-o"
                    || a.starts_with("--upload-pack")
            })
        }
        // `find` runs programs and deletes, `env` / `xargs` / `sudo` / `sh` run other commands,
        // `sed -i` / `tee` / `cp` write: nothing here vouches for them even if listed.
        "find" | "env" | "xargs" | "sudo" | "doas" | "sh" | "bash" | "zsh" | "fish" | "eval"
        | "exec" | "command" | "builtin" | "nohup" | "time" | "watch" | "timeout" | "nice"
        | "sed" | "awk" | "perl" | "python" | "python3" | "ruby" | "node" | "tee" | "cp" | "mv"
        | "rm" | "dd" | "ln" | "chmod" | "chown" | "mkdir" | "touch" | "truncate" | "curl"
        | "wget" | "ssh" | "scp" | "less" | "more" | "vi" | "vim" | "nano" | "emacs" | "man" => {
            false
        }
        _ => true,
    }
}

#[cfg(test)]
#[path = "safe_commands_tests.rs"]
mod tests;
