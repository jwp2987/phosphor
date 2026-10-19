//! How an agent-requested, `wait_until_completion` command is typed into the user's REAL
//! shell with pagers disabled for the command only (#855).
//!
//! History: this used to be a subshell, `(export PAGER=cat ...; cmd)`. That keeps the real
//! exit code (unlike `(cmd) | cat`) but the subshell also swallowed every `cd`, `export` and
//! venv activation the agent ran, while the block list showed them as having run in the
//! user's session. The command now runs in a group in the current shell: the pager variables
//! are saved, overridden, and restored afterwards, and the command's own status is handed on.
//!
//! The command runs as the left side of `{ cmd; } && rc=0 || rc=$?`, a context in which
//! errexit (`set -e`, zsh `ERR_EXIT` / `ERR_RETURN`) is suppressed: a failing agent command
//! must not exit the user's shell or skip the restore. The status is re-raised as `$?` only
//! when errexit is off (re-raising would exit the shell); otherwise `__phosphor_ap_rc` is left
//! set and the precmd hook in the bootstrap scripts prefers it over `$?`.
//!
//! Known limitation: zsh rejects a bare `}` argument (`echo }`) as a parse error, in or out of
//! a group; that is zsh's own behaviour, not something the wrapper introduces.
//!
//! A command that, at its top level, would change or end the shell itself in a way a group
//! cannot contain (`exit`, `exec`, `set -e`, `trap`, ...) keeps the subshell form; see
//! [`needs_subshell`]. The exception is `exec <shell>`: in the subshell it would leave a second,
//! unhooked interactive shell holding the terminal, so it is refused before any wrapping (see
//! [`exec_into_shell`]).
//!
//! Every wrapped form carries [`HISTORY_MARKER`] (or, for fish, a leading space) so the shell's
//! history filter keeps the wrapper out of the history file (#904).
//!
//! **Invariant (unchanged since e9cbc20b0): the closing token must be alone on its own
//! line** whenever the command is multi-line, ends in a comment, or ends in `&` / `;`, or a
//! heredoc terminator / the comment swallows it.

use crate::terminal::shell::ShellType;

/// Pager variables saved and restored around an agent command. Keep in sync with the
/// backstop in `bash_body.sh` / `zsh_body.sh` (they restore from the same markers).
pub(super) const PAGER_VARS: &[&str] = &[
    "PAGER",
    "GIT_PAGER",
    "MANPAGER",
    "SYSTEMD_PAGER",
    "PSQL_PAGER",
    "AWS_PAGER",
    "GH_PAGER",
    "BAT_PAGER",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_KEY_0",
    "GIT_CONFIG_VALUE_0",
];

/// `(variable, value)` overrides applied for the duration of the command. Most pagers
/// accept `cat`; `AWS_PAGER` and `BAT_PAGER` document the empty string as "no pager"
/// (`cat` also works for bat, but an empty `AWS_PAGER` is the only form aws-cli v2 treats as
/// off), the git entries are the second line of defence for `core.pager` from git >= 2.31.
const OVERRIDES: &[(&str, &str)] = &[
    ("PAGER", "cat"),
    ("GIT_PAGER", "cat"),
    ("MANPAGER", "cat"),
    ("SYSTEMD_PAGER", "cat"),
    ("PSQL_PAGER", "cat"),
    ("AWS_PAGER", ""),
    ("GH_PAGER", "cat"),
    ("BAT_PAGER", ""),
    ("GIT_CONFIG_COUNT", "1"),
    ("GIT_CONFIG_KEY_0", "core.pager"),
    ("GIT_CONFIG_VALUE_0", "cat"),
];

/// Names of the temporaries the bash/zsh group uses. Unlikely to collide with user code.
/// Substring present in every bash/zsh wrapper form. The bootstrap scripts' history filters
/// (`HISTIGNORE` in `bash_body.sh`, `_warp_zshaddhistory` in `zsh_body.sh`) match on it so the
/// ~900-character wrapper never lands in the user's history file (#904). MATCHED PAIR with
/// those two scripts. fish has no pattern filter; it skips lines with a leading space, which
/// [`wrap_command`] adds.
pub(crate) const HISTORY_MARKER: &str = "__phosphor_ap_";

const MARKER_ACTIVE: &str = "__phosphor_ap_active";
const MARKER_RC: &str = "__phosphor_ap_rc";

fn var_list() -> String {
    PAGER_VARS.join(" ")
}

/// bash/zsh: remember, for each variable, its value and whether it was set at all.
fn posix_save() -> String {
    format!(
        r#"{MARKER_ACTIVE}=1; for __phosphor_ap_v in {list}; do eval "__phosphor_ap_o_$__phosphor_ap_v=\${{$__phosphor_ap_v-}}; __phosphor_ap_s_$__phosphor_ap_v=\${{$__phosphor_ap_v+1}}"; done"#,
        list = var_list()
    )
}

fn posix_export() -> String {
    let assignments: Vec<String> = OVERRIDES
        .iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect();
    format!("export {}", assignments.join(" "))
}

/// bash/zsh: put every saved variable back (unsetting the ones that were unset) and drop
/// the markers. Identical text is used by the precmd backstop in the bootstrap scripts.
pub(crate) fn posix_restore() -> String {
    format!(
        r#"for __phosphor_ap_v in {list}; do eval "if [ -n \"\${{__phosphor_ap_s_$__phosphor_ap_v-}}\" ]; then export $__phosphor_ap_v=\"\$__phosphor_ap_o_$__phosphor_ap_v\"; else unset $__phosphor_ap_v; fi; unset __phosphor_ap_s_$__phosphor_ap_v __phosphor_ap_o_$__phosphor_ap_v"; done; unset __phosphor_ap_v {MARKER_ACTIVE}"#,
        list = var_list()
    )
}

/// Runs `action` only when no errexit-like option is on in the user's shell.
fn errexit_guard(shell_type: ShellType, action: &str) -> String {
    let is_on = match shell_type {
        ShellType::Zsh => "case $- in *e*) true;; *) [[ -o errreturn ]];; esac".to_owned(),
        _ => "case $- in *e*) true;; *) false;; esac".to_owned(),
    };
    format!("if {is_on}; then :; else {action}; fi")
}

/// The old behaviour: unset then export inside a subshell. Used for commands that cannot
/// safely run in a group.
fn posix_subshell_prelude() -> String {
    format!("unset PAGER GIT_PAGER MANPAGER; {}", posix_export())
}

/// A no-op simple command in front of the subshell. It carries [`HISTORY_MARKER`] (the group
/// form has it in its variable names, a bare `( ... )` has none) and, for bash, it is a simple
/// command of the user's own shell: bash's DEBUG trap, which reports `Preexec`, fires for
/// simple commands but not for a `( ... )` list, so a bare subshell never got a `Preexec`.
fn posix_subshell_lead() -> String {
    format!(": {HISTORY_MARKER}subshell; ")
}

fn fish_prelude() -> String {
    // `set -lx` is block-scoped: it shadows any global and vanishes at `end`, even if the
    // block is interrupted. `$status` after `end` is the last command's.
    OVERRIDES
        .iter()
        .map(|(name, value)| format!("set -lx {name} '{value}'"))
        .collect::<Vec<_>>()
        .join("; ")
}

fn pwsh_save() -> String {
    let names: Vec<String> = PAGER_VARS.iter().map(|n| format!("'{n}'")).collect();
    format!(
        "$__phosphor_ap = @{{}}; foreach ($__phosphor_ap_v in @({})) {{ $__phosphor_ap[$__phosphor_ap_v] = [Environment]::GetEnvironmentVariable($__phosphor_ap_v) }}",
        names.join(",")
    )
}

fn pwsh_override() -> String {
    OVERRIDES
        .iter()
        .map(|(name, value)| format!("[Environment]::SetEnvironmentVariable('{name}','{value}')"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Taken inside the `try`, straight after the command: the `finally` block's own statements
/// would otherwise overwrite `$?` and `$LASTEXITCODE` before `Warp-Precmd` reads them. `$?` is
/// taken first, and `$LASTEXITCODE` is read through `Test-Path` because under `Set-StrictMode` a
/// bare read throws when no native command has run yet.
const PWSH_CAPTURE: &str = "$__phosphor_ap_ok = $?; $__phosphor_ap_ec = if (Test-Path variable:global:LASTEXITCODE) { $global:LASTEXITCODE } else { 0 }";

/// The last statements of the wrapper: put the captured status back. `$?` can only be made
/// false by a failing command, so a silent failed lookup stands in for it. The two capture
/// variables are removed by `Warp-Precmd`.
const PWSH_RERAISE: &str = "$global:LASTEXITCODE = if (Test-Path variable:__phosphor_ap_ec) { $__phosphor_ap_ec } else { 0 }; if (-not $__phosphor_ap_ok) { Get-Item -LiteralPath 'Variable:\\__phosphor_ap_failed' -ErrorAction SilentlyContinue }";

/// `$env:` is process-wide, so the restore must be explicit; `finally` also runs on Ctrl-C.
fn pwsh_restore() -> &'static str {
    "foreach ($__phosphor_ap_v in @($__phosphor_ap.Keys)) { [Environment]::SetEnvironmentVariable($__phosphor_ap_v, $__phosphor_ap[$__phosphor_ap_v]) }; Remove-Variable __phosphor_ap, __phosphor_ap_v -ErrorAction SilentlyContinue"
}

/// Whether a single-line form would put the closing token on the command's line in a way
/// the command can swallow or break: a comment, a trailing `&` / `;`, or a newline.
fn needs_own_line_closer(command: &str) -> bool {
    command.contains('\n')
        || command.contains('#')
        || command.ends_with('&')
        || command.ends_with(';')
        || command.ends_with('\\')
}

fn shell_type_is_pwsh(shell_type: ShellType) -> bool {
    shell_type == ShellType::PowerShell
}

/// Wraps `command` for `shell_type`; see the module docs. Unknown shells get the command
/// back byte-exact (pager suppression is then ineffective and the long fallback timeout is
/// the only protection).
pub(super) fn wrap_command(shell_type: Option<ShellType>, command: &str) -> String {
    let Some(shell_type) = shell_type else {
        return command.to_owned();
    };
    // A trailing blank line would only clutter the blocklist display.
    let command = command.trim_end();
    // A trailing line continuation joins the next line to the command, which would swallow
    // the closing token even when it is alone on its line; a blank line absorbs the join.
    let continued;
    let command = if command.ends_with('\\') && !shell_type_is_pwsh(shell_type) {
        continued = format!("{command}\n");
        continued.as_str()
    } else {
        command
    };
    let own_line = needs_own_line_closer(command);
    let subshell = needs_subshell(shell_type, command);

    match shell_type {
        ShellType::Zsh | ShellType::Bash if subshell => {
            let lead = posix_subshell_lead();
            let prelude = posix_subshell_prelude();
            if own_line {
                format!("{lead}({prelude}\n{command}\n)")
            } else {
                format!("{lead}({prelude}; {command})")
            }
        }
        ShellType::Zsh | ShellType::Bash => {
            let head = format!("{}; {}", posix_save(), posix_export());
            // The command runs as the left side of an `&&` / `||` list, where errexit
            // (`set -e`, zsh `ERR_EXIT` / `ERR_RETURN`) is suppressed, so a failing agent
            // command can neither kill the user's shell nor skip the restore. The status is
            // handed over in a variable.
            let status = format!("&& {MARKER_RC}=0 || {MARKER_RC}=$?");
            // With errexit off the status is also re-raised as `$?` (`unset` would clobber it,
            // so an `eval` expands the saved value first), so the user's next `$?` is right.
            // With errexit on, re-raising would exit the shell; the marker is left set for
            // the precmd hook, which prefers it over `$?` and unsets it.
            let reraise = format!(r#"eval "unset {MARKER_RC}; (exit ${MARKER_RC})""#);
            let tail = format!(
                "{restore}; {guard}",
                restore = posix_restore(),
                guard = errexit_guard(shell_type, &reraise)
            );
            if own_line {
                format!("{{ {head}\n{{\n{command}\n}} {status}; {tail}\n}}")
            } else {
                format!("{{ {head}; {{ {command}; }} {status}; {tail}; }}")
            }
        }
        ShellType::Fish => {
            let prelude = fish_prelude();
            // The leading space keeps the line out of fish's history (#904); see
            // `HISTORY_MARKER`.
            if subshell {
                // fish has no `( ... )` subshell, so a command that would end or replace the
                // shell (`exit`, `return`, `exec <cmd>`) runs in a child fish instead; the
                // pager overrides are exported, so the child inherits them, and `$status`
                // after `end` is the child's exit code (#932). fish rejects a command
                // substitution in command position, hence the variable.
                let escaped = command.replace('\\', "\\\\").replace('\'', "\\'");
                format!(
                    " begin; {prelude}; set -l __phosphor_ap_fish (status fish-path); \
                     $__phosphor_ap_fish -c '{escaped}'; end"
                )
            } else if own_line {
                format!(" begin; {prelude}\n{command}\nend")
            } else {
                format!(" begin; {prelude}; {command}; end")
            }
        }
        ShellType::PowerShell => {
            // `.` runs the block in the current scope (variables persist, like the venv
            // activation the agent may run); `&` is the child-scope fallback for `exit`.
            let call = if subshell { "&" } else { "." };
            let save = pwsh_save();
            let over = pwsh_override();
            let restore = pwsh_restore();
            let capture = PWSH_CAPTURE;
            let reraise = PWSH_RERAISE;
            if own_line {
                format!(
                    "{call} {{ {save}; {over}\ntry {{\n{command}\n{capture}\n}} finally {{ {restore} }}\n{reraise}\n}}"
                )
            } else {
                format!(
                    "{call} {{ {save}; {over}; try {{ {command}; {capture} }} finally {{ {restore} }}; {reraise} }}"
                )
            }
        }
    }
}

/// Whether `command` must keep the subshell wrapper because, at its top level, it ends the
/// shell or changes how it treats errors, which a group in the user's real shell would leak
/// into (or kill) their session. fish has no subshell form, and PowerShell only has the
/// scope difference, so only `exit` matters there.
pub(super) fn needs_subshell(shell_type: ShellType, command: &str) -> bool {
    if shell_type == ShellType::Fish {
        return simple_commands(command).iter().any(|words| {
            command_name_and_args(words).is_some_and(|(name, args)| match name {
                "exit" | "exec" | "return" => true,
                // `eval` runs its arguments in the current shell.
                "eval" => {
                    let text = args.join(" ");
                    !text.is_empty() && needs_subshell(ShellType::Fish, &text)
                }
                _ => false,
            })
        });
    }
    simple_commands(command).iter().any(|words| {
        let Some((name, args)) = command_name_and_args(words) else {
            return false;
        };
        match shell_type {
            ShellType::PowerShell => name.eq_ignore_ascii_case("exit"),
            _ => posix_command_needs_subshell(name, args),
        }
    })
}

/// Programs that, once `exec`ed with nothing to run, leave an interactive shell holding the
/// terminal (#899, #900). `su` opens a login shell the same way.
const SHELL_PROGRAMS: &[&str] = &[
    "sh", "bash", "zsh", "fish", "dash", "ksh", "mksh", "ash", "csh", "tcsh", "pwsh", "nu",
    "xonsh", "elvish", "su",
];

/// Words that run another program: `exec nice bash` is `exec bash`.
const EXEC_PASS_THROUGH: &[&str] = &[
    "env", "nice", "sudo", "doas", "nohup", "time", "command", "stdbuf",
];

/// Arguments after a shell that still leave it interactive.
const INTERACTIVE_SHELL_ARGS: &[&str] = &["-l", "-i", "--login", "--interactive", "--", "-"];

/// If `command` would, at its top level, `exec` into an interactive shell, returns that shell's
/// name.
///
/// `exec bash` replaces the process the command runs in. Unwrapped it replaces the user's own
/// hooked shell; in the subshell form it replaces the subshell, which becomes a second
/// interactive shell holding the terminal with none of Phosphor's hooks: the command never
/// finishes, so the agent's tool call never returns, and ^C is ignored at its prompt (#899,
/// #900). There is no way to run that on the agent's behalf that ends, so it is refused up
/// front with an explanation the model can act on.
///
/// Only an `exec` that leaves the shell interactive counts: `exec bash -c 'cmd'`, `exec sh
/// script.sh` and `exec env X=1 sh run.sh` run something and end. A target that is not known
/// until runtime (`exec "$SHELL"`, `exec $(command -v zsh)`, `` exec `which fish` ``) is
/// treated as a shell, since the only reason to `exec` the login shell is to become it.
pub(super) fn exec_into_shell(shell_type: Option<ShellType>, command: &str) -> Option<String> {
    if shell_type == Some(ShellType::PowerShell) {
        return None;
    }
    if exec_of_backtick_substitution(command) {
        return Some("$SHELL".to_owned());
    }
    simple_commands(command).iter().find_map(|words| {
        let (name, args) = command_name_and_args(words)?;
        match name {
            "exec" => exec_target_shell(args),
            // `eval` runs its arguments in the current shell: `eval 'exec bash'`.
            "eval" => {
                let text = args.join(" ");
                (!text.is_empty())
                    .then(|| exec_into_shell(shell_type, &text))
                    .flatten()
            }
            _ => None,
        }
    })
}

/// `` exec `which fish` ``: the scanner ends a command at a backtick, so the target is lost.
fn exec_of_backtick_substitution(command: &str) -> bool {
    let mut prev_exec = false;
    for word in command.split_whitespace() {
        if prev_exec && word.starts_with('`') {
            return true;
        }
        prev_exec = word == "exec";
    }
    false
}

fn exec_target_shell(args: &[String]) -> Option<String> {
    let mut iter = args.iter().peekable();
    while let Some(arg) = iter.next() {
        if arg == "--" {
            continue;
        }
        if arg == "-a" {
            // `-a NAME` sets argv[0]; the program follows.
            iter.next();
            continue;
        }
        if arg.starts_with('-') && arg.len() > 1 {
            continue;
        }
        if is_assignment(arg) {
            continue;
        }
        let base = arg.rsplit('/').next().unwrap_or(arg);
        let base = base.strip_suffix(".exe").unwrap_or(base);
        let rest: Vec<&String> = iter.clone().collect();
        if matches!(base, "sudo" | "doas")
            && rest
                .iter()
                .any(|a| matches!(a.as_str(), "-i" | "-s" | "--login" | "--shell"))
        {
            return Some(base.to_owned());
        }
        if EXEC_PASS_THROUGH.contains(&base) {
            continue;
        }
        let rest_is_interactive = rest
            .iter()
            .all(|a| INTERACTIVE_SHELL_ARGS.contains(&a.as_str()));
        // `exec "$SHELL"`, `exec "${SHELL:-/bin/sh}"`, `$0`, or a `$(...)` substitution (the
        // scanner leaves a lone `$` for it): a shell chosen at runtime.
        let dynamic = arg == "$"
            || arg.starts_with("$(")
            || arg.starts_with("$SHELL")
            || arg.starts_with("${SHELL")
            || matches!(arg.as_str(), "$0" | "${0}");
        if dynamic {
            return rest_is_interactive.then(|| "$SHELL".to_owned());
        }
        return (SHELL_PROGRAMS.contains(&base) && rest_is_interactive).then(|| base.to_owned());
    }
    None
}

/// Words that may precede the real command name in a simple command.
fn is_prefix_word(word: &str) -> bool {
    matches!(
        word,
        "if" | "then"
            | "else"
            | "elif"
            | "do"
            | "while"
            | "until"
            | "!"
            | "{"
            | "}"
            | "time"
            | "builtin"
            | "command"
            | "nohup"
            | "noglob"
            // fish's condition combinators: `false; and exit`.
            | "and"
            | "or"
            | "not"
            | "begin"
    )
}

fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn command_name_and_args(words: &[String]) -> Option<(&str, &[String])> {
    let idx = words
        .iter()
        .position(|w| !is_prefix_word(w) && !is_assignment(w))?;
    Some((words[idx].as_str(), &words[idx + 1..]))
}

/// Lowercased option/flag name with `_` and `-` removed: `ERR_EXIT` -> `errexit`.
fn normalized_option(word: &str) -> String {
    word.chars()
        .filter(|c| *c != '_' && *c != '-')
        .collect::<String>()
        .to_ascii_lowercase()
}

fn is_error_behaviour_option(word: &str) -> bool {
    let n = normalized_option(word);
    n.contains("errexit")
        || n.contains("errreturn")
        || n.contains("nounset")
        || n.contains("pipefail")
        || n.contains("failglob")
        || n == "unset"
}

fn posix_command_needs_subshell(name: &str, args: &[String]) -> bool {
    match name {
        "exit" | "exec" | "trap" | "ulimit" => true,
        // `eval` runs its arguments in the current shell, so `eval exit` or `eval "set -e"`
        // is `exit` / `set -e`.
        "eval" => {
            let text = args.join(" ");
            !text.is_empty() && needs_subshell(ShellType::Bash, &text)
        }
        "set" => args.iter().any(|arg| {
            let is_cluster = (arg.starts_with('-') || arg.starts_with('+'))
                && arg.len() > 1
                && arg[1..].chars().all(|c| c.is_ascii_alphabetic());
            (is_cluster && arg[1..].chars().any(|c| c == 'e' || c == 'u'))
                || (!arg.starts_with('-')
                    && !arg.starts_with('+')
                    && is_error_behaviour_option(arg))
        }),
        "shopt" | "setopt" | "unsetopt" => args.iter().any(|arg| is_error_behaviour_option(arg)),
        "unset" => args.iter().any(|arg| arg == "PATH"),
        _ => false,
    }
}

/// Index just past the `))` that closes an arithmetic expression whose `((` ended at
/// `start`, or the end of input. Nested parentheses are counted.
fn skip_arithmetic(chars: &[char], start: usize) -> usize {
    let mut depth = 2usize;
    let mut i = start;
    while i < chars.len() {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    i
}

/// A cheap, forgiving shell tokenizer: splits `command` into simple commands made of
/// unquoted words. It knows single and double quotes, backslashes, `#` comments and heredoc
/// bodies (which are skipped), and treats `; & | ( ) \n` and backticks as command separators.
/// It is NOT a parser; it only has to be right about "is the word in command position
/// `exit`", erring toward more subshells.
fn simple_commands(command: &str) -> Vec<Vec<String>> {
    struct Scanner {
        out: Vec<Vec<String>>,
        words: Vec<String>,
        cur: String,
        in_word: bool,
    }
    impl Scanner {
        fn end_word(&mut self) {
            if self.in_word {
                self.words.push(std::mem::take(&mut self.cur));
                self.in_word = false;
            }
        }
        fn end_command(&mut self) {
            self.end_word();
            if !self.words.is_empty() {
                self.out.push(std::mem::take(&mut self.words));
            }
        }
        fn push(&mut self, c: char) {
            self.cur.push(c);
            self.in_word = true;
        }
    }

    let chars: Vec<char> = command.chars().collect();
    let mut s = Scanner {
        out: Vec::new(),
        words: Vec::new(),
        cur: String::new(),
        in_word: false,
    };
    // (terminator, tabs stripped) for heredocs opened on the current line.
    let mut pending: Vec<(String, bool)> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' => {
                match chars.get(i + 1) {
                    Some('\n') => {}
                    Some(&next) => s.push(next),
                    None => {}
                }
                i += 2;
            }
            '\'' => {
                s.in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '\'' {
                    s.cur.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            '"' => {
                s.in_word = true;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.cur.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            // ANSI-C quoting: `$'it\'s'`; a backslash escapes the next character.
            '$' if chars.get(i + 1) == Some(&'\'') => {
                s.in_word = true;
                i += 2;
                while i < chars.len() && chars[i] != '\'' {
                    if chars[i] == '\\' && i + 1 < chars.len() {
                        i += 1;
                    }
                    s.cur.push(chars[i]);
                    i += 1;
                }
                i += 1;
            }
            // Arithmetic: `$((1<<2))` inside a word, or `(( x << 2 ))` as a command. The
            // `<<` inside is a shift, never a heredoc, so the whole thing is skipped.
            '$' if chars.get(i + 1) == Some(&'(') && chars.get(i + 2) == Some(&'(') => {
                s.push('$');
                i = skip_arithmetic(&chars, i + 3);
            }
            '(' if chars.get(i + 1) == Some(&'(') && !s.in_word && s.words.is_empty() => {
                s.end_command();
                i = skip_arithmetic(&chars, i + 2);
            }
            '#' if !s.in_word => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '\n' => {
                s.end_command();
                i += 1;
                for (terminator, strip_tabs) in pending.drain(..) {
                    while i < chars.len() {
                        let start = i;
                        while i < chars.len() && chars[i] != '\n' {
                            i += 1;
                        }
                        let line: String = chars[start..i].iter().collect();
                        i = (i + 1).min(chars.len());
                        let line = if strip_tabs {
                            line.trim_start_matches('\t')
                        } else {
                            line.as_str()
                        };
                        if line == terminator {
                            break;
                        }
                    }
                }
            }
            ';' | '&' | '|' | '(' | ')' | '`' => {
                s.end_command();
                i += 1;
            }
            '<' if chars.get(i + 1) == Some(&'<') && chars.get(i + 2) != Some(&'<') => {
                i += 2;
                let strip_tabs = chars.get(i) == Some(&'-');
                if strip_tabs {
                    i += 1;
                }
                while matches!(chars.get(i), Some(' ' | '\t')) {
                    i += 1;
                }
                let mut terminator = String::new();
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !matches!(chars[i], ';' | '&' | '|' | '(' | ')' | '<' | '>')
                {
                    if !matches!(chars[i], '\'' | '"' | '\\') {
                        terminator.push(chars[i]);
                    }
                    i += 1;
                }
                if !terminator.is_empty() {
                    pending.push((terminator, strip_tabs));
                }
                // The redirection itself is not a word of the command.
            }
            ' ' | '\t' | '\r' => {
                s.end_word();
                i += 1;
            }
            other => {
                s.push(other);
                i += 1;
            }
        }
    }
    s.end_command();
    s.out
}

#[cfg(test)]
#[path = "agent_shell_wrap_tests.rs"]
mod tests;
