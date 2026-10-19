use std::collections::HashSet;

use lazy_static::lazy_static;
use natural_language_detection::{
    check_if_token_has_shell_syntax, is_ordinary_english_word, is_word, WordDb,
};
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

    /// Fork-original (#808): command words that [`is_unambiguous_shell_command`] accepts as the
    /// head of a shell command line, without needing a `token_description` from the completer.
    /// Shell builtins and the everyday Unix / developer tool set. Membership alone never forces a
    /// Shell classification — the rest of the line has to be command-shaped too (see
    /// `is_unambiguous_shell_command`) — so this list only needs to be *right*, not exhaustive.
    /// It deliberately leaves out words that commonly open an English request in a terminal
    /// (`help`, `show`, `list`, `install`, `run`, `delete`, `update`...), which are never commands
    /// in their own right.
    static ref UNAMBIGUOUS_SHELL_COMMAND_WORDS: HashSet<&'static str> = HashSet::from([
        // Shell builtins and keywords.
        ".", ":", "[", "[[", "alias", "bg", "bind", "builtin", "cd", "command", "compgen",
        "complete", "declare", "dirs", "disown", "eval", "exec", "exit", "export", "false", "fc",
        "fg", "getopts", "hash", "history", "jobs", "kill", "let", "local", "popd", "printf",
        "pushd", "pwd", "readonly", "return", "set", "shift", "source", "test", "time",
        "trap", "true", "type", "typeset", "ulimit", "umask", "unalias", "unset", "wait",
        // Coreutils and the usual text / file / process tools.
        "awk", "basename", "bat", "bc", "bzip2", "cat", "chgrp", "chmod", "chown", "clear", "cmp",
        "cp", "cut", "date", "dd", "df", "diff", "dirname", "du", "egrep", "env", "exa", "eza",
        "fd", "fgrep", "file", "find", "free", "fzf", "gawk", "grep", "groups", "gunzip", "gzip",
        "head", "hexdump", "htop", "id", "killall", "less", "ln", "locate", "ls", "mkdir",
        "mkfifo", "mktemp", "more", "mount", "mv", "ncdu", "nice", "nohup", "od", "patch",
        "pgrep", "pkill", "printenv", "ps", "readlink", "realpath", "renice", "rg", "rm", "rmdir",
        "sed", "seq", "sleep", "sort", "stat", "strings", "tac", "tail", "tar", "tee", "top",
        "touch", "tr", "tree", "umount", "uname", "uniq", "uptime", "watch", "wc", "whereis",
        "which", "who", "whoami", "xargs", "xxd", "xz", "yes", "zcat", "zip", "unzip",
        // Network.
        "curl", "dig", "ifconfig", "ip", "lsof", "nc", "netstat", "nmap", "nslookup", "ping",
        "rsync", "scp", "sftp", "ss", "ssh", "ssh-add", "ssh-agent", "ssh-keygen", "tcpdump",
        "traceroute", "wget",
        // Developer tool chains.
        "ar", "bun", "bundle", "cargo", "clang", "clang++", "clippy", "cmake", "deno", "docker",
        "dotnet", "g++", "gcc", "gdb", "gem", "gh", "git", "go", "gofmt", "gradle", "hg", "java",
        "javac", "jq", "ld", "lldb", "ltrace", "make", "mvn", "nm", "ninja", "node", "npm", "npx",
        "objdump", "perf", "perl", "php", "pip", "pip3", "pipx", "pnpm", "podman", "poetry",
        "python", "python3", "ruby", "rustc", "rustfmt", "rustup", "strace", "svn", "swift",
        "uv", "valgrind", "yarn", "yq",
        // Infra, packages, services, editors, misc.
        "ansible", "apk", "apt", "apt-get", "aws", "az", "bash", "brew", "chsh", "code",
        "crontab", "dash", "dmesg", "dnf", "doas", "dpkg", "emacs", "fdisk", "fish", "flatpak",
        "gcloud", "gpg", "helm", "iptables", "journalctl", "ksh", "kubectl", "lsblk", "mkfs",
        "mongosh", "mysql", "nano", "nvim", "open", "openssl", "pacman", "parted", "passwd",
        "psql", "redis-cli", "rpm", "screen", "sh", "snap", "sqlite3", "stty", "su", "sudo",
        "systemctl", "terraform", "tmux", "tput", "tty", "ufw", "vagrant", "vi", "vim",
        "xdg-open", "yum", "zsh", "zypper",
    ]);
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

/// True iff `token` has any evidence of being a real command the user meant to run: a
/// `token_description` from the completer, a one-off shell keyword, or a path-like token.
///
/// A `token_description` is also how a known shell function, builtin, or alias shows up here:
/// the completer's `describe_given_token` resolves the first word of a command against
/// `CompletionContext::top_level_commands()`, which chains in `functions()`, `builtins()`, and
/// `aliases()` (case-sensitively matched, per `alias_and_function_case_sensitivity`) alongside
/// external commands — and those are populated at session bootstrap, independent of the
/// external-command `$PATH` scan. So a first token that resolves to a user's own function,
/// builtin, or alias already carries a `token_description` and never reaches the dictionary/prose
/// checks below at all; this function does not need to (and does not) re-query the completion
/// context for that evidence separately.
fn has_command_evidence(token: &warp_completer::ParsedTokenData) -> bool {
    let word = token.token.as_str();
    token.token_description.is_some()
        || is_one_off_shell_command_keyword(word)
        || is_path_like_token(word)
}

/// True iff a token later in the buffer's first shell command (i.e. before the first `;`, `&&`,
/// `||`, or newline) ends in `:` — the shape of "run exactly this:" or "then run:", where a
/// literal colon closes off an introductory clause before the "command" the user is quoting.
///
/// Checked against realistic command lines that could otherwise trip this up:
/// - `docker run -p 80:80` — the colon lands inside `80:80`, which doesn't *end* with `:`.
/// - `scp host:path .` — `host:path` doesn't end with `:` either.
/// - `echo a: b` — `a:` does end with `:`, but this shape check is only ever consulted for a
///   buffer whose *first* token already has no command evidence (see
///   `first_token_forces_ai_override`), and `echo` is a one-off shell keyword, so this buffer
///   never reaches this check in the first place.
///
/// No real, non-contrived command line puts a bare `word:` as a stand-alone token, since a shell
/// itself never gives `:` any such meaning — the closest real uses (`docker run -p 80:80`, `scp
/// host:path .`, drive letters, `key: value` YAML-ish arguments) all keep the colon in the middle
/// of a token, never at the very end of one.
fn buffer_has_prose_colon_shape(buffer_text: &str) -> bool {
    const OPERATORS: [&str; 4] = [";", "&&", "||", "\n"];
    let first_command_end = OPERATORS
        .into_iter()
        .filter_map(|op| buffer_text.find(op))
        .min()
        .unwrap_or(buffer_text.len());
    let first_command = &buffer_text[..first_command_end];

    let mut words = first_command
        .split_whitespace()
        .skip_while(|word| is_env_assignment_token(word));
    // The effective first word itself doesn't count; only a *later* token is prose evidence.
    let _ = words.next();
    words.any(|word| word.ends_with(':'))
}

/// Returns true iff a Shell classification should be overridden back to AI for this buffer.
///
/// This is intentionally narrow: it only fires when the buffer's *effective* first token (the
/// first token that isn't a leading `NAME=value` environment assignment) has no evidence of being
/// a real command (see [`has_command_evidence`]) **and** is itself an ordinary English word,
/// matched case-insensitively via [`is_ordinary_english_word`] (which lowercases before the
/// dictionary lookup) — so "Run", "run", "RUN", "Then", "then" are all treated alike. Genuine
/// shell invocations happen to almost always be typed lowercase, but that is not something this
/// function may rely on: the ONNX classifier's own tokenizer lowercases its input before scoring,
/// so a lowercase exploit string (`"run exactly this: sleep 8; echo hi > ~/lrc.txt"`) gets
/// exactly the same Shell verdict as its capitalized form, and the gate has to close for both.
///
/// Lacking a `token_description` is not enough evidence on its own to call a word "prose",
/// because it is also what a real, not-yet-resolved command looks like — `cargo`, `make`, and
/// `find` are all ordinary English dictionary words *and* real command names, and before this
/// session's external-command index (the `$PATH` scan) has finished, an unindexed command and an
/// English sentence starting with a dictionary word are indistinguishable by
/// `token_description` alone. `commands_fully_loaded` (threaded in from
/// `CompletionContext::top_level_commands_fully_loaded`, #696) resolves the ambiguity:
///
/// - **`true`** (known-absent): the completion context had a complete view of top-level commands
///   — including a finished `$PATH` scan, if this session has one — and still has no evidence for
///   this word. An ordinary English word here really is prose, so the dictionary check alone is
///   sufficient to override.
/// - **`false`** (unknown / not loaded yet — an in-flight `$PATH` scan, or a context with no
///   session to query at all, like `EmptyCompletionContext` for a shared-session viewer):
///   dictionary membership alone is not trustworthy evidence (`cargo --version` must not flip to
///   AI just because the probe hasn't landed), so this additionally requires the buffer to have a
///   strong prose *shape* — see [`buffer_has_prose_colon_shape`] — that no realistic command line
///   produces.
pub fn first_token_forces_ai_override(
    input: &ParsedTokensSnapshot,
    commands_fully_loaded: bool,
) -> bool {
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

    if has_command_evidence(token) {
        return false;
    }

    if !is_ordinary_english_word(token.token.as_str()) {
        return false;
    }

    commands_fully_loaded || buffer_has_prose_colon_shape(&input.buffer_text)
}

/// Characters that end a bare shell word when [`is_unambiguous_shell_command`] scans
/// `buffer_text` for operators and their neighbors, and that cut the head word off an operator
/// glued to it (`yes|head`, `ls>out.txt`).
const SHELL_WORD_TERMINATORS: [char; 8] = ['|', '&', ';', '<', '>', '(', ')', '`'];

/// Longest buffer [`is_unambiguous_shell_command`] will look at: it runs synchronously on every
/// keystroke, and anything longer is a paste the classifier can take its time over.
const MAX_PREFILTER_BYTES: usize = 4096;

lazy_static! {
    /// Closed-class English function words (#808). One of these anywhere in the buffer, outside
    /// quotes and other than the head word, vetoes [`is_unambiguous_shell_command`] outright: a
    /// shell command line essentially never contains a bare `the`, `sure`, `it`, `and`... as an
    /// argument, while "make sure the build passes; rm -rf target and rebuild" or "find the bug;
    /// test it" are exactly the prose that otherwise has command shape. Deliberately excludes
    /// `a` (`cat a b > c.txt` is a real command line); the article reading of `a` is handled by
    /// [`is_prose_article`] instead (#831).
    static ref PROSE_FUNCTION_WORDS: HashSet<&'static str> = HashSet::from([
        "the", "an", "this", "that", "these", "those", "it", "its", "is", "are", "was", "were",
        "be", "been", "my", "me", "your", "our", "their", "them", "they", "we", "you", "i", "to",
        "for", "of", "with", "and", "into", "through", "why", "how", "what", "whether", "sure",
        "please", "should", "would", "could", "can", "then", "or", "if", "but", "so",
    ]);

    /// Acronyms that read as English nouns in a request ("open a PR", "watch CPU", "kill PID
    /// 4242", "free RAM") and are not signal, variable, ref or file names a command line takes as
    /// a bare argument (#831). [`is_prose_word`] otherwise treats every ALL-CAPS word as a shell
    /// argument (`DEBUG`, `HOME`, `HEAD`, `README`), so this list is deliberately closed: it must
    /// never contain a name that commonly appears unquoted in a command (`CI`, `IO`, `SYS`...)
    /// or that is a plausible directory name (`API`, `SDK`, `VM`, `UI`, `UX`, `LLM`: `cd API`,
    /// `ls UX`).
    static ref PROSE_ACRONYMS: HashSet<&'static str> = HashSet::from([
        "PR", "MR", "CPU", "GPU", "RAM", "PID", "CVE", "URL", "DNS", "SSL", "TLS", "OOM",
    ]);

    /// `re-` words that are commands or subcommands in their own right (`sudo reboot`, `brew
    /// reinstall foo`, `mount -o remount`, `git repack`), so [`is_derived_english_word`] must not
    /// read them as prose even though their base (`boot`, `install`, `mount`...) is English. They
    /// are only arguments in that position, though: see [`is_misplaced_re_command_word`].
    static ref RE_PREFIXED_COMMAND_WORDS: HashSet<&'static str> = HashSet::from([
        "reboot", "recompile", "reconfigure", "reindex", "reinstall", "relink", "remount",
        "renice", "repack",
    ]);
}

/// A whitespace-separated word of the buffer, with quoted spans (`'echo t'`, `"a b"`, `` `cmd` ``)
/// kept together. See [`split_shell_words`].
struct ShellWord {
    text: String,
    /// `true` if any part of the word was inside quotes or backticks.
    quoted: bool,
}

/// True iff a `'` at byte `index` of `text` directly follows an alphanumeric: an apostrophe inside
/// a word (`Josh's`, `doesn't`). To bash that is an opening quote, so such a line (outside quotes)
/// is never an unambiguous command — it is either prose or an unbalanced quote headed for the
/// `PS2` prompt.
fn is_apostrophe_in_word(text: &str, index: usize) -> bool {
    index > 0 && text.as_bytes()[index - 1].is_ascii_alphanumeric()
}

/// True iff `text` quotes the way a finished bash command line does: every `'`, `"` and `` ` ``
/// opened outside quotes is closed (backslash escapes honored outside and inside double quotes /
/// backticks), no unquoted `'` sits inside a word ([`is_apostrophe_in_word`]), no typographic
/// quote (`‘`, `’` — `find Josh’s PR`, `don’t`) appears outside quotes at all (no shell command
/// line contains one unquoted; it is prose), and the line does not end in an unescaped backslash
/// (a line continuation — `ls -la \` — which also parks bash at its `PS2` prompt; `text` is
/// trimmed, so a trailing backslash is always the last character).
fn has_shell_safe_quoting(text: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (index, c) in text.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                }
            }
            Some(q) => {
                if c == '\\' {
                    escaped = true;
                } else if c == q {
                    quote = None;
                }
            }
            None => match c {
                '\\' => escaped = true,
                '\'' => {
                    if is_apostrophe_in_word(text, index) {
                        return false;
                    }
                    quote = Some(c);
                }
                '‘' | '’' => return false,
                '"' | '`' => quote = Some(c),
                _ => {}
            },
        }
    }
    !escaped && quote.is_none()
}

/// Splits `text` into whitespace-separated words, keeping single-quoted, double-quoted and
/// backtick spans together (the delimiters are kept in `text`). Only called on text that passed
/// [`has_shell_safe_quoting`], so every span is closed.
fn split_shell_words(text: &str) -> Vec<ShellWord> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut quote: Option<char> = None;
    for c in text.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(ShellWord {
                        text: std::mem::take(&mut current),
                        quoted,
                    });
                    quoted = false;
                }
            }
            None if matches!(c, '\'' | '"' | '`') => {
                quote = Some(c);
                quoted = true;
                current.push(c);
            }
            None => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(ShellWord {
            text: current,
            quoted,
        });
    }
    words
}

/// True iff `word` is a command word this pre-filter recognizes without completer help: the
/// curated [`UNAMBIGUOUS_SHELL_COMMAND_WORDS`] set or a one-off shell keyword (`echo`, `sudo`...).
fn is_known_shell_command_word(word: &str) -> bool {
    UNAMBIGUOUS_SHELL_COMMAND_WORDS.contains(word) || is_one_off_shell_command_keyword(word)
}

/// True iff `token` is spelled as an explicit filesystem path to run or reference (`./x`, `../x`,
/// `/x`, `~/x`, bare `~`). Stricter than [`is_path_like_token`] on purpose: `foo/bar` or `.hidden`
/// alone is weaker evidence than a leading path prefix, and this pre-filter wants only the
/// unambiguous shape.
fn is_explicit_path_token(token: &str) -> bool {
    token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with('/')
        || token.starts_with("~/")
        || token == "~"
}

/// True iff `token` looks like something a redirection would plausibly target: a path (`/dev/null`,
/// `~/out.log`, `logs/x`) or a `name.ext` file name (`out.txt`, `out.tar.gz`, `.hidden`). The
/// name must start with a letter, `_` or `.`, and every part after a dot must start with a letter,
/// so a bare word, a number or a quantity (`1MB`, `1.5GB`, `10.5MB`) does not qualify — "find
/// files > 1.5GB" is a comparison, not a redirection.
fn is_redirect_target(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if is_explicit_path_token(token) || token.contains('/') {
        return true;
    }
    if !token.contains('.') {
        return false;
    }
    let is_part_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let mut parts = token.split('.');
    let Some(first) = parts.next() else {
        return false;
    };
    let first_ok = first.chars().all(is_part_char)
        && (first.is_empty() || first.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_'));
    first_ok
        && parts.all(|part| {
            part.starts_with(|c: char| c.is_ascii_alphabetic()) && part.chars().all(is_part_char)
        })
}

/// True iff `word` could begin a (sub)command: a known command word, an explicit path, or a
/// leading `NAME=value` assignment.
fn can_start_command(word: &str) -> bool {
    is_known_shell_command_word(word)
        || is_explicit_path_token(word)
        || is_env_assignment_token(word)
}

/// The first bare word of `rest` (after leading whitespace), ending at whitespace or an operator
/// character. Empty if there is none.
fn next_shell_word(rest: &str) -> &str {
    rest.trim_start()
        .split(|c: char| c.is_whitespace() || SHELL_WORD_TERMINATORS.contains(&c))
        .next()
        .unwrap_or("")
}

/// What [`shell_operator_evidence`] found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OperatorEvidence {
    /// A pipe, command separator, descriptor redirection or substitution with a command word
    /// inside: shape no English sentence has.
    Strong,
    /// Only a bare `>`, `>>` or `<` to a file target was found. Carries the operator's byte
    /// offset so the caller can check the words before it: "export data > backup.csv" is prose
    /// whose redirect would truncate `backup.csv` before the (failing) command even runs.
    BareRedirect(usize),
}

/// Scans `text`, outside quotes, for a shell operator used the way a shell command line uses it
/// — not merely the character:
///
/// - `|`, `||`, `&&`, `;` followed by a word that can start a command (`yes | head`, `cd x && ls`);
/// - `2>&1` / `>&2` / `&>` / `2>/dev/null` (descriptor redirections);
/// - `$(`, `<(`, `>(` or a paired backtick whose content starts with a command word
///   (`$(whoami)`, `` `which python` `` — but not "make $(money) fast");
/// - `>`, `>>`, `<` followed by a plausible file target (`> /dev/null`, `>> out.log`, `< in.txt`),
///   so "find files > 1MB" and "a > b" do not count — reported as [`OperatorEvidence::BareRedirect`]
///   unless something stronger is also present.
///
/// A lone `&` (backgrounding), `<<` (heredoc — indistinguishable from prose "<<") and a bare
/// unpaired backtick are deliberately not operators here.
fn shell_operator_evidence(text: &str) -> Option<OperatorEvidence> {
    let bytes = text.as_bytes();
    let mut quote: Option<u8> = None;
    let mut bare_redirect: Option<usize> = None;
    let mut i = 0;
    // Only ASCII bytes are ever matched below, so every index this loop slices `text` at is a
    // char boundary (an ASCII byte never occurs inside a multi-byte UTF-8 sequence).
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if b == b'\\' && q == b'"' {
                i += 2;
                continue;
            }
            if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match b {
            b'\\' => {
                i += 2;
                continue;
            }
            b'\'' | b'"' => quote = Some(b),
            b'`' => {
                if let Some(close) = text[i + 1..].find('`') {
                    if is_known_shell_command_word(next_shell_word(&text[i + 1..i + 1 + close])) {
                        return Some(OperatorEvidence::Strong);
                    }
                    i += close + 2;
                    continue;
                }
            }
            b'$' if bytes.get(i + 1) == Some(&b'(') => {
                if can_start_command(next_shell_word(&text[i + 2..])) {
                    return Some(OperatorEvidence::Strong);
                }
            }
            b'|' => {
                let op_len = if bytes.get(i + 1) == Some(&b'|') {
                    2
                } else {
                    1
                };
                if can_start_command(next_shell_word(&text[i + op_len..])) {
                    return Some(OperatorEvidence::Strong);
                }
                i += op_len;
                continue;
            }
            b'&' => match bytes.get(i + 1) {
                Some(b'&') => {
                    if can_start_command(next_shell_word(&text[i + 2..])) {
                        return Some(OperatorEvidence::Strong);
                    }
                    i += 2;
                    continue;
                }
                Some(b'>') => return Some(OperatorEvidence::Strong),
                _ => {}
            },
            b';' => {
                if can_start_command(next_shell_word(&text[i + 1..])) {
                    return Some(OperatorEvidence::Strong);
                }
            }
            b'>' => {
                // `2>/dev/null`, `1>>log`: a descriptor number glued to the operator is shell
                // syntax no English sentence produces, so it is as strong as `2>&1`.
                let has_descriptor = i > 0 && bytes[i - 1].is_ascii_digit();
                match bytes.get(i + 1) {
                    Some(b'&') => return Some(OperatorEvidence::Strong),
                    Some(b'(') => {
                        if can_start_command(next_shell_word(&text[i + 2..])) {
                            return Some(OperatorEvidence::Strong);
                        }
                    }
                    Some(b'>') => {
                        if is_redirect_target(next_shell_word(&text[i + 2..])) {
                            if has_descriptor {
                                return Some(OperatorEvidence::Strong);
                            }
                            bare_redirect.get_or_insert(i);
                        }
                        i += 2;
                        continue;
                    }
                    _ => {
                        if is_redirect_target(next_shell_word(&text[i + 1..])) {
                            if has_descriptor {
                                return Some(OperatorEvidence::Strong);
                            }
                            bare_redirect.get_or_insert(i);
                        }
                    }
                }
            }
            b'<' => match bytes.get(i + 1) {
                Some(b'(') => {
                    if can_start_command(next_shell_word(&text[i + 2..])) {
                        return Some(OperatorEvidence::Strong);
                    }
                }
                Some(b'<') => {
                    i += 2;
                    continue;
                }
                _ => {
                    if is_redirect_target(next_shell_word(&text[i + 1..])) {
                        bare_redirect.get_or_insert(i);
                    }
                }
            },
            _ => {}
        }
        i += 1;
    }
    bare_redirect.map(OperatorEvidence::BareRedirect)
}

/// `word` with sentence punctuation removed for dictionary lookups: trailing ASCII `.,!:;` and,
/// at either end, the typographic marks prose carries (`…`, `“ ”`, `‘ ’`, `—`, `–`), so `it…` and
/// `“the”` still read as `it` and `the`.
fn strip_sentence_punctuation(word: &str) -> &str {
    word.trim_matches(|c: char| matches!(c, '…' | '“' | '”' | '‘' | '’' | '—' | '–'))
        .trim_end_matches(|c: char| matches!(c, '.' | ',' | '!' | ':' | ';'))
}

/// True iff `word` (unquoted, lowercased, trailing punctuation stripped) is one of the
/// [`PROSE_FUNCTION_WORDS`].
fn is_function_word(word: &ShellWord) -> bool {
    !word.quoted
        && PROSE_FUNCTION_WORDS.contains(
            strip_sentence_punctuation(&word.text)
                .to_lowercase()
                .as_str(),
        )
}

/// True iff `words[index]` is the English article `a` rather than a one-letter file name (#831):
/// an unquoted, lowercase `a` followed by an unquoted word of two or more ASCII letters and digits
/// (`make a PR`, `make a README`, `open a PR`). A following single letter, flag, path, file name
/// or quoted word leaves it an argument (`cat a b > c.txt`, `tar -xf a -C out`, `grep a
/// file.txt`), and so does a trailing `a` (`touch a`, `make a`).
fn is_prose_article(words: &[ShellWord], index: usize) -> bool {
    let word = &words[index];
    if word.quoted || word.text != "a" {
        return false;
    }
    words.get(index + 1).is_some_and(|next| {
        let text = strip_sentence_punctuation(&next.text);
        !next.quoted && text.len() >= 2 && text.chars().all(|c| c.is_ascii_alphanumeric())
    })
}

/// True iff `word` is an unquoted word ending in sentence punctuation (#831): a comma
/// (`node_modules,` in "rm -rf node_modules, reinstall"; `~,` in "chmod -R 777 ~, fixperms"), or
/// a period after a letter, digit or `_` (`node_modules.` in "rm -rf node_modules. reinstall",
/// `~/x...`). That punctuation separates clauses of a sentence; in a command line it would become
/// part of the argument (`rm -rf "node_modules,"`), which nobody types on purpose. Flags that take
/// it as their value (`cut -d,`, `awk -F,`, `sort -t,`), a bare `,`, `.` or `..` argument (`tr ,
/// x`, `cd ..`, `find . -name x`), a path ending in `/.` or `:.` (`host:.`) and quoted words
/// (`printf 'a,'`) are not sentence punctuation.
fn ends_in_sentence_punctuation(word: &ShellWord) -> bool {
    if word.quoted || word.text.starts_with('-') {
        return false;
    }
    if word.text.ends_with(',') {
        return !word.text.trim_end_matches(',').is_empty();
    }
    word.text.ends_with('.')
        && word
            .text
            .trim_end_matches('.')
            .ends_with(|c: char| c.is_ascii_alphanumeric() || c == '_')
}

/// True iff `remaining[index]` is one of the [`RE_PREFIXED_COMMAND_WORDS`] somewhere other than
/// subcommand position (#831). Directly after a known command word or a flag (`sudo reboot`,
/// `brew reinstall foo`, `sudo apt reinstall vim`, `mount -o remount /`) it is a command or
/// subcommand; after anything else (`rm -rf node_modules reinstall`, `rm -rf node_modules ;
/// reinstall`, `rm -rf node_modules -> reinstall`, `rm -rf dist reboot`) it is the English verb
/// of a request, and the line would delete a file of that name. A flag is `-` or `--` followed by
/// a letter or digit, so `->` and `--` are not. `ssh host reboot` is vetoed too: the pre-filter
/// cannot tell that `host` is a host, so the classifier decides it.
fn is_misplaced_re_command_word(head: &str, remaining: &[ShellWord], index: usize) -> bool {
    let word = &remaining[index];
    if word.quoted || !RE_PREFIXED_COMMAND_WORDS.contains(strip_sentence_punctuation(&word.text)) {
        return false;
    }
    let previous = match index.checked_sub(1) {
        Some(previous) => remaining[previous].text.as_str(),
        None => head,
    };
    let is_flag = previous
        .strip_prefix("--")
        .or_else(|| previous.strip_prefix('-'))
        .is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_alphanumeric()));
    !(is_known_shell_command_word(previous) || is_flag)
}

/// The word-level prose vetoes shared by [`is_unambiguous_shell_command`] and [`reads_as_prose`]
/// (#832), so the two cannot drift: any [`PROSE_FUNCTION_WORDS`] member, the article `a` before a
/// word ([`is_prose_article`]), a word ending in a sentence comma or period
/// ([`ends_in_sentence_punctuation`]), or a command-like `re-` word outside subcommand position
/// ([`is_misplaced_re_command_word`]). `head` is the effective first word (assignments skipped,
/// cut at any glued operator) and `remaining` the words after it.
///
/// Only vetoes belong here, never the Rule-2 body test ([`is_prose_word`]: dictionary words,
/// acronyms, derived words), which would make `cargo build` read as prose.
fn has_prose_veto_word(head: &str, remaining: &[ShellWord]) -> bool {
    remaining.iter().any(is_function_word)
        || (0..remaining.len()).any(|index| is_prose_article(remaining, index))
        || remaining.iter().any(ends_in_sentence_punctuation)
        || (0..remaining.len()).any(|index| is_misplaced_re_command_word(head, remaining, index))
}

/// True iff `text` (unquoted, sentence punctuation stripped) is an acronym used as an English
/// noun (#831): one of the [`PROSE_ACRONYMS`], or any ALL-CAPS word with a lowercase plural `s`
/// (`TODOs`, `PRs`, `PIDs`, `URLs`) — a spelling only prose produces.
fn is_prose_acronym(text: &str) -> bool {
    if PROSE_ACRONYMS.contains(text) {
        return true;
    }
    text.strip_suffix('s')
        .is_some_and(|stem| stem.len() >= 2 && stem.chars().all(|c| c.is_ascii_uppercase()))
}

/// True iff lowercase `text` looks like a regular English derivation that the dictionaries miss
/// (#831): a `re-` word whose base is English (`redeploy`, `reclone`, `recreate`, `regenerate`)
/// or any word ending in `-ly` (`recursively` — but also `anomaly`, `butterfly`; this is a
/// spelling test, not a part-of-speech one). Only plain lowercase ASCII words of six or more
/// letters qualify, so short tool and package names (`redis`, `regex`) stay arguments; a longer
/// `re-` name whose remainder is no English word (`reflog`, `readline`) does too, and so do the
/// [`RE_PREFIXED_COMMAND_WORDS`] (`reboot`, `reinstall`), which
/// [`is_misplaced_re_command_word`] handles by position instead.
fn is_derived_english_word(text: &str) -> bool {
    if text.len() < 6
        || !text.chars().all(|c| c.is_ascii_lowercase())
        || RE_PREFIXED_COMMAND_WORDS.contains(text)
    {
        return false;
    }
    if let Some(base) = text.strip_prefix("re") {
        if is_ordinary_english_word(base) || is_word(base, WordDb::StackOverflow) {
            return true;
        }
    }
    text.ends_with("ly")
}

/// True iff `word` reads as a word of English prose rather than a command argument: unquoted, no
/// shell syntax characters, no digits, and then either Capitalised (`Chrome`, `Netflix`, `Sarah` —
/// an uppercase first letter followed by lowercase only; shell arguments are not proper nouns),
/// an acronym used as a noun ([`is_prose_acronym`]: `PR`, `CPU`, `TODOs`), or — after stripping
/// trailing sentence punctuation — an ordinary English dictionary word (stemmed, via
/// [`is_ordinary_english_word`]), a word from the StackOverflow vocabulary, or a regular
/// derivation of one ([`is_derived_english_word`]: `reinstall`, `recursively`). Other ALL-CAPS
/// words (`DEBUG`, `HOME`, `README`: signal, variable and file names) and single letters (`cat a
/// b`, `-x`'s bare `x`) are never prose; the function-word veto handles `i` on its own.
fn is_prose_word(word: &ShellWord) -> bool {
    if word.quoted {
        return false;
    }
    let text = strip_sentence_punctuation(&word.text);
    if text.is_empty()
        || text.chars().count() == 1
        || check_if_token_has_shell_syntax(text)
        || text.chars().any(|c| c.is_ascii_digit())
    {
        return false;
    }
    if is_prose_acronym(text) {
        return true;
    }
    let has_lowercase = text.chars().any(|c| c.is_lowercase());
    if text.starts_with(|c: char| c.is_ascii_uppercase())
        && has_lowercase
        && text.chars().skip(1).all(|c| c.is_ascii_lowercase())
    {
        return true;
    }
    if !has_lowercase && text.chars().any(|c| c.is_uppercase()) {
        return false;
    }
    is_ordinary_english_word(text)
        || is_word(&text.to_lowercase(), WordDb::StackOverflow)
        || is_derived_english_word(text)
}

/// Fork-original (#808): a cheap, deterministic pre-filter run *before* any classifier (ML model,
/// heuristic, history match) that returns `true` only when the buffer is unambiguously a shell
/// command line. The caller then forces `InputType::Shell` with no model round-trip, so inputs
/// like `yes | head -c 50000000 > /dev/null` and `trap 'echo t' DEBUG` can never be routed to the
/// agent — which is what the ONNX classifier did (`yes` is a strong natural-language token to it,
/// and `trap`, being an ordinary English word, tripped the #696 safety gate).
///
/// Precision over recall: a `true` here executes the line, so every rule requires command
/// *shape*, never a bare keyword or a bare metacharacter. Anything that is merely *probably* a
/// command (`cargo build`, `git push origin main`, a lone `ls`) is left to the existing pipeline,
/// which already handles those well.
///
/// The buffer is trimmed; leading `NAME=value` assignments are skipped to find the *head*, the
/// effective first word, which is cut at any operator glued to it (`yes|head`).
///
/// **Vetoes** (always `false`): an empty buffer, or one over [`MAX_PREFILTER_BYTES`] (a paste,
/// not a typed line — and this runs synchronously per keystroke); a trailing `?`; quoting bash
/// would reject, an apostrophe inside a word (`find Josh's PR`) or a typographic quote
/// (`don’t`), see [`has_shell_safe_quoting`]; the #696 prose shape of a later word ending in `:`
/// before the first operator ("run exactly this:", [`buffer_has_prose_colon_shape`]); a quoted
/// head; a head that is a slash command rather than a path (`/plan`, `/model gpt`: starts with
/// `/` with no further `/` or `.`); any [`PROSE_FUNCTION_WORDS`] member among the other unquoted
/// words (`the`, `sure`, `it`, `and`...); the article `a` before a word (`make a PR`,
/// [`is_prose_article`]); a word ending in a sentence comma or period (`rm -rf node_modules,
/// reinstall`, [`ends_in_sentence_punctuation`]); a command-like `re-` word outside subcommand
/// position (`rm -rf node_modules reinstall`, [`is_misplaced_re_command_word`]); and `exit` / `return` / `logout` with a non-numeric argument
/// (`exit vim` ends the whole session, not vim).
///
/// **Rule 1 (operator):** the head itself carries command evidence — a known command word
/// ([`UNAMBIGUOUS_SHELL_COMMAND_WORDS`] or a one-off keyword), an explicit path (`./x`, `/x`,
/// `~/x`, `../x`), or a `token_description` from the completer; an assignment prefix in front of
/// an unknown word is *not* evidence — **and** the buffer contains a shell operator used as a
/// shell uses it ([`shell_operator_evidence`]: `| cmd`, `&& cmd`, `; cmd`, `2>&1`, `$(cmd`...).
/// When the only such operator is a bare `>` / `>>` / `<` to a file target, no word before it may
/// be prose either: `ls > listing.txt` and `echo $TS > ~/t.txt` qualify, `export data >
/// backup.csv` does not (the redirect would truncate the file before the failing command ran).
///
/// **Rule 2 (no prose):** the head is a known command word or explicit path, or has an assignment
/// prefix — in which case an unknown head must itself be neither prose nor a function word
/// (`FOO=bar mycmd` qualifies, `x=3 squared` and `DEBUG=1 why` do not) — **and** no later word
/// reads as English prose ([`is_prose_word`]), so `trap 'echo t' DEBUG`, `kill -9 1234`, `./run.sh
/// --fast` qualify while `ls shows nothing`, `kill Chrome`, `cargo build` fall through. A lone
/// command word (`ls`, `exit`) and a head the pipeline already treats as one-off natural language
/// (`yes`) never qualify under this rule; a lone explicit path (`./run.sh`) does.
pub fn is_unambiguous_shell_command(input: &ParsedTokensSnapshot) -> bool {
    let text = input.buffer_text.trim();
    if text.is_empty()
        || text.len() > MAX_PREFILTER_BYTES
        || text.ends_with('?')
        || !has_shell_safe_quoting(text)
        || buffer_has_prose_colon_shape(text)
    {
        return false;
    }

    let words = split_shell_words(text);
    let mut first_index = 0;
    while words
        .get(first_index)
        .is_some_and(|word| !word.quoted && is_env_assignment_token(&word.text))
    {
        first_index += 1;
    }
    let Some(first) = words.get(first_index) else {
        // Only assignments (`FOO=bar`): valid shell, but also what "x=5" in a question looks like.
        return false;
    };
    if first.quoted {
        return false;
    }
    let head = first
        .text
        .split(|c: char| SHELL_WORD_TERMINATORS.contains(&c))
        .next()
        .unwrap_or("");
    if head.is_empty() {
        return false;
    }
    // A slash command (`/plan`, `/model gpt`) that reached autodetection is never a path to run.
    if head.starts_with('/') && !head[1..].contains(['/', '.']) {
        return false;
    }
    let remaining = &words[first_index + 1..];
    if has_prose_veto_word(head, remaining) {
        return false;
    }
    // `exit vim` / `exit tmux`: bash says "numeric argument required" and then exits the whole
    // session anyway (zsh just exits), so only a numeric argument is command-shaped here.
    if matches!(head, "exit" | "return" | "logout")
        && remaining
            .first()
            .is_some_and(|word| !word.text.chars().all(|c| c.is_ascii_digit()))
    {
        return false;
    }

    let has_assignment_prefix = first_index > 0;
    let is_known_command = is_known_shell_command_word(head);
    let is_explicit_path = is_explicit_path_token(head);

    // The completer's view of the effective first token: skip the same leading assignments.
    let first_token_described = input
        .parsed_tokens
        .iter()
        .find(|token| !is_env_assignment_token(token.token.as_str()))
        .is_some_and(|token| token.token_description.is_some());

    // Rule 1: evidence on the head itself plus a shell operator used as a shell uses it.
    if is_known_command || is_explicit_path || first_token_described {
        match shell_operator_evidence(text) {
            Some(OperatorEvidence::Strong) => return true,
            Some(OperatorEvidence::BareRedirect(offset)) => {
                // The operator is outside quotes, so the prefix splits into the same leading
                // assignments and head; only the body words before the redirect are checked.
                let before = split_shell_words(&text[..offset]);
                if !before.iter().skip(first_index + 1).any(is_prose_word) {
                    return true;
                }
            }
            None => {}
        }
    }

    // Rule 2: command-shaped head and a body with no prose in it.
    if !(is_known_command || is_explicit_path || has_assignment_prefix) {
        return false;
    }
    if has_assignment_prefix && !(is_known_command || is_explicit_path) {
        let head_word = ShellWord {
            text: head.to_string(),
            quoted: false,
        };
        if is_prose_word(&head_word) || is_function_word(&head_word) {
            return false;
        }
    }
    if is_one_off_natural_language_word(head) {
        return false;
    }
    if remaining.is_empty() && !is_explicit_path && !has_assignment_prefix {
        return false;
    }
    !remaining.iter().any(is_prose_word)
}

/// Fork-original (#832): true iff `buffer_text` carries one of the prose vetoes that
/// [`is_unambiguous_shell_command`] applies:
///
/// - a trailing `?`;
/// - quoting bash would reject, an apostrophe inside a word or a typographic quote
///   ([`has_shell_safe_quoting`]: `find Josh's PR`);
/// - the prose colon shape ([`buffer_has_prose_colon_shape`]: `run exactly this: ...`);
/// - after the effective first word, any word-level veto in [`has_prose_veto_word`], the same
///   function the pre-filter calls: a [`PROSE_FUNCTION_WORDS`] member (`find the bug in main.rs`,
///   `find where config is loaded`), the article `a` before a word (`make a PR`), a sentence
///   comma or period (`rm -rf node_modules, reinstall`), or a misplaced `re-` command word
///   (`rm -rf node_modules reinstall`).
///
/// This is the same evidence the pre-filter uses to *refuse* a Shell verdict, exposed on its own
/// so the command-history matcher can refuse to let a failed or unverified history entry force
/// Shell for a buffer that reads as a sentence. It never says anything is a command: `false` only
/// means "none of these vetoes fired". The head word is only used as the context for the `re-`
/// rule (`brew reinstall foo` is a subcommand), and the pre-filter's Rule-2 body test
/// ([`is_prose_word`]) is deliberately not applied, so `cargo build` is not prose.
///
/// Known trade-offs: shell syntax that uses the same words or a colon-terminated word reads as
/// prose too (`for i in $(seq 3); do echo $i; done`, `if true; then echo y; fi`, `grep -r for
/// src`, `scp f.txt host:`, and — because the colon test splits on whitespace without honouring
/// quotes — `git commit -m "feat: x"`, `curl -H "Accept: application/json" ...`). So does a `re-`
/// command word after a head the pre-filter does not know (`ssh host reboot`). For the history
/// matcher that only means a *failed* or exit-code-less entry for such a line no longer
/// short-circuits to Shell; the line goes to the classifier, and an entry that exited 0 still
/// matches. The opposite residual: command-headed prose with no veto word (`find large files`,
/// `install docker`, `free up disk space`) does not read as prose here, so a failed entry for it
/// can still route it to Shell from history.
///
/// A buffer over [`MAX_PREFILTER_BYTES`] returns `false` without being scanned: this runs
/// synchronously on the UI thread per keystroke, and a paste that size is not going to sit within
/// the history matcher's 0.9 similarity of a remembered command line anyway.
pub fn reads_as_prose(buffer_text: &str) -> bool {
    let text = buffer_text.trim();
    if text.is_empty() || text.len() > MAX_PREFILTER_BYTES {
        return false;
    }
    if text.ends_with('?') || !has_shell_safe_quoting(text) || buffer_has_prose_colon_shape(text) {
        return true;
    }
    let words = split_shell_words(text);
    let first_index = words
        .iter()
        .take_while(|word| !word.quoted && is_env_assignment_token(&word.text))
        .count();
    let Some(first) = words.get(first_index) else {
        return false;
    };
    let head = first
        .text
        .split(|c: char| SHELL_WORD_TERMINATORS.contains(&c))
        .next()
        .unwrap_or("");
    words
        .get(first_index + 1..)
        .is_some_and(|remaining| has_prose_veto_word(head, remaining))
}

/// Shell reserved words that head compound commands whose body legitimately contains bare
/// function words (`for i in a b; do ...`, `if true; then ...`), so [`reads_as_command_headed_prose`]
/// never fires on them.
const COMPOUND_COMMAND_HEADS: [&str; 10] = [
    "for", "while", "until", "if", "case", "select", "function", "time", "do", "then",
];

/// Fork-original (#834): true iff the buffer has a command-looking head but reads as a request in
/// English: a bare member of [`PROSE_FUNCTION_WORDS`] after the head (`help me`, `do
/// it`, `watch the logs for errors`, `npm install fails with EACCES`, `rm -rf ~/proj and clone
/// again`), or a trailing `?` after a letter (`docker vs podman?`). The safety gate uses it to
/// turn a classifier's `Shell` verdict back into `AI` even though the first word is an installed
/// command, which [`first_token_forces_ai_override`] cannot do.
///
/// Deliberately narrower than [`reads_as_prose`]: no colon shape (`git commit -m "feat: x"`), no
/// article or sentence-punctuation rule, no `re-` rule, and compound-command heads
/// ([`COMPOUND_COMMAND_HEADS`]) are exempt. Quoted words never count, so `echo "to be"` is
/// unaffected. A buffer with a glued shell operator in its head word, or over
/// [`MAX_PREFILTER_BYTES`], returns `false`.
pub fn reads_as_command_headed_prose(buffer_text: &str) -> bool {
    let text = buffer_text.trim();
    if text.is_empty() || text.len() > MAX_PREFILTER_BYTES || !has_shell_safe_quoting(text) {
        return false;
    }
    let words = split_shell_words(text);
    let first_index = words
        .iter()
        .take_while(|word| !word.quoted && is_env_assignment_token(&word.text))
        .count();
    let Some(first) = words.get(first_index) else {
        return false;
    };
    if first.quoted
        || first.text.contains(SHELL_WORD_TERMINATORS)
        || COMPOUND_COMMAND_HEADS.contains(&first.text.as_str())
    {
        return false;
    }
    let remaining = &words[first_index + 1..];
    if remaining.is_empty() {
        return false;
    }
    if remaining.iter().any(is_function_word) {
        return true;
    }
    text.strip_suffix('?')
        .is_some_and(|before| before.ends_with(|c: char| c.is_alphabetic()))
}

/// Fork-original (#886): true iff `buffer_text` carries no prose evidence anywhere, the head word
/// included: it does not end in `?`, its quoting is shell-safe, it has no prose colon shape, and
/// no unquoted word is a [`PROSE_FUNCTION_WORDS`] member, the article `a` before a word, a word
/// ending in a sentence comma or period, or a prose word ([`is_prose_word`]: dictionary English,
/// Capitalised, an acronym used as a noun).
///
/// This is stricter than both [`is_unambiguous_shell_command`] (which needs a command head and
/// exempts it) and [`reads_as_prose`] (which skips the head and the Rule-2 body test): it is the
/// gate for treating shell *typeahead* as Shell without a command head. The pieces a typing burst
/// splits or merges (`ho b199 >> ~/blk.txt`, `b22 b23`, `b11 blk.txtecho b12`,
/// `xtecho b5 >> ~/blk.txt`) pass; any line a person might type as a question while a command
/// runs (`exit vim`, `rm the temp files`, `mv files to backup`, `kill server process`) does not,
/// and so does a well-formed command made of English words (`echo hello world`) -- the pre-filter
/// and the classifier decide those as they would any buffer.
///
/// A buffer over [`MAX_PREFILTER_BYTES`] returns `false` without being scanned.
pub fn has_no_prose_evidence(buffer_text: &str) -> bool {
    let text = buffer_text.trim();
    if text.is_empty()
        || text.len() > MAX_PREFILTER_BYTES
        || text.ends_with('?')
        || !has_shell_safe_quoting(text)
        || buffer_has_prose_colon_shape(text)
    {
        return false;
    }
    let words = split_shell_words(text);
    !(words.iter().any(is_function_word)
        || (0..words.len()).any(|index| is_prose_article(&words, index))
        || words.iter().any(ends_in_sentence_punctuation)
        || words.iter().any(is_prose_word))
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;
