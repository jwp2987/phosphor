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

/// How every PowerShell wrapper starts. `pwsh.ps1` (the `Clear-History` call and the
/// `AddToHistoryHandler`) matches this exact prefix, anchored, so a user's own command that
/// merely mentions the marker is never dropped from their history (#942). MATCHED PAIR.
pub(crate) const PWSH_WRAPPER_HISTORY_PREFIX: &str =
    ". { $__phosphor_ap = @{}; foreach ($__phosphor_ap_v in @(";

const MARKER_ACTIVE: &str = "__phosphor_ap_active";
const MARKER_RC: &str = "__phosphor_ap_rc";

fn var_list() -> String {
    PAGER_VARS.join(" ")
}

/// bash/zsh: remember, for each variable, its value and whether it was set at all.
/// bash/zsh: while an agent command runs, `exit` and `logout` are functions that return, so a
/// form the detection missed cannot end the user's shell. The user's own definitions (functions;
/// an alias is untouched) are saved as text and put back byte-identical by [`posix_restore`] and
/// by the precmd backstop, so the user's own `exit`, `logout` and Ctrl-D work again afterwards,
/// also when the command failed or was interrupted. Subshells (`( ... )`, `$( ... )`, generators)
/// inherit the functions, so inside one (`BASH_SUBSHELL` / `ZSH_SUBSHELL` above zero) the guard
/// exits for real: it only has to protect the user's own shell. `builtin exit` / `command exit`
/// bypass the guard and are caught by the detection instead.
const POSIX_GUARD_SAVE: &str = r#"__phosphor_ap_gx=$(typeset -f exit 2>/dev/null); __phosphor_ap_gl=$(typeset -f logout 2>/dev/null); function exit { __phosphor_ap_s=$?; if [ "${BASH_SUBSHELL:-${ZSH_SUBSHELL:-0}}" != 0 ]; then builtin exit "${1:-$__phosphor_ap_s}"; fi; return "${1:-0}"; }; function logout { __phosphor_ap_s=$?; if [ "${BASH_SUBSHELL:-${ZSH_SUBSHELL:-0}}" != 0 ]; then builtin exit "$__phosphor_ap_s"; fi; return 0; }"#;

/// Undoes [`POSIX_GUARD_SAVE`]. Keep identical to the loop in the bootstrap backstops.
pub(crate) const POSIX_GUARD_RESTORE: &str = r#"unset -f exit logout; if [ -n "$__phosphor_ap_gx" ]; then eval "$__phosphor_ap_gx"; fi; if [ -n "$__phosphor_ap_gl" ]; then eval "$__phosphor_ap_gl"; fi; unset __phosphor_ap_gx __phosphor_ap_gl"#;

fn posix_save() -> String {
    format!(
        r#"{MARKER_ACTIVE}=1; {POSIX_GUARD_SAVE}; for __phosphor_ap_v in {list}; do eval "__phosphor_ap_o_$__phosphor_ap_v=\${{$__phosphor_ap_v-}}; __phosphor_ap_s_$__phosphor_ap_v=\${{$__phosphor_ap_v+1}}"; done"#,
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
        r#"for __phosphor_ap_v in {list}; do eval "if [ -n \"\${{__phosphor_ap_s_$__phosphor_ap_v-}}\" ]; then export $__phosphor_ap_v=\"\$__phosphor_ap_o_$__phosphor_ap_v\"; else unset $__phosphor_ap_v; fi; unset __phosphor_ap_s_$__phosphor_ap_v __phosphor_ap_o_$__phosphor_ap_v"; done; {POSIX_GUARD_RESTORE}; unset __phosphor_ap_v {MARKER_ACTIVE}"#,
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
        // The user's `$LASTEXITCODE` is saved and reset, so a stale native exit code cannot be
        // reported for a failing cmdlet; `Warp-Precmd` computes the code, then puts it back.
        "$__phosphor_ap = @{{}}; foreach ($__phosphor_ap_v in @({})) {{ $__phosphor_ap[$__phosphor_ap_v] = [Environment]::GetEnvironmentVariable($__phosphor_ap_v) }}; $global:__phosphor_ap_lecset = [bool](Microsoft.PowerShell.Management\\Test-Path variable:global:LASTEXITCODE); $global:__phosphor_ap_lec = if ($global:__phosphor_ap_lecset) {{ $global:LASTEXITCODE }} else {{ $null }}; $global:LASTEXITCODE = 0",
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
const PWSH_CAPTURE: &str = "$__phosphor_ap_ok = $?; $__phosphor_ap_ec = if (Microsoft.PowerShell.Management\\Test-Path variable:global:LASTEXITCODE) { $global:LASTEXITCODE } else { 0 }";

/// The statements after the wrapper's block: put the captured status back. They must run at the
/// top level of the typed line, not inside the `. { ... }` block: `$?` after a dot-sourced block
/// is always true, whatever failed inside. At the top level a failing lookup does set `$?` to
/// false, and the path is chosen so the lookup fails exactly when the command did: the
/// capture variable `__phosphor_ap_ok` exists (and is true) on success, `__phosphor_ap_failed`
/// never exists (`-ErrorAction Ignore` records nothing in the user's `$Error`; `Out-Null` keeps the success case quiet and leaves `$?` to the lookup). The capture variables are removed by `Warp-Precmd`.
const PWSH_RERAISE: &str = "$global:LASTEXITCODE = if (Microsoft.PowerShell.Management\\Test-Path variable:__phosphor_ap_ec) { $__phosphor_ap_ec } else { 0 }; Microsoft.PowerShell.Management\\Get-Item -LiteralPath ('Variable:\\__phosphor_ap_' + ('failed', 'ok')[[int][bool]$__phosphor_ap_ok]) -ErrorAction Ignore | Microsoft.PowerShell.Core\\Out-Null";

/// `$env:` is process-wide, so the restore must be explicit; `finally` also runs on Ctrl-C.
fn pwsh_restore() -> &'static str {
    "foreach ($__phosphor_ap_v in @($__phosphor_ap.Keys)) { [Environment]::SetEnvironmentVariable($__phosphor_ap_v, $__phosphor_ap[$__phosphor_ap_v]) }; Microsoft.PowerShell.Utility\\Remove-Variable __phosphor_ap, __phosphor_ap_v -ErrorAction SilentlyContinue"
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
            let reraise = format!(r#"eval "unset {MARKER_RC}; (builtin exit ${MARKER_RC})""#);
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
            } else {
                // While the command runs, `exit` is a function that returns (fish lets a
                // function shadow the builtin); the user's own function, if any, is copied
                // aside and put back by name, and `fish.sh` has a prompt-event backstop for an
                // interrupted command. The command's exact `$status` is re-raised last.
                let guard = "set -g __phosphor_ap_guard 1; functions -q exit; and functions -c exit __phosphor_ap_gx; function exit; set -q argv[1]; and return $argv[1]; return 0; end";
                let unguard = "set -l __phosphor_ap_st $status; functions -e exit; functions -q __phosphor_ap_gx; and functions -c __phosphor_ap_gx exit; functions -q __phosphor_ap_gx; and functions -e __phosphor_ap_gx; set -e __phosphor_ap_guard; command sh -c \"exit $__phosphor_ap_st\"";
                if own_line {
                    format!(" begin; {prelude}; {guard}\n{command}\n{unguard}; end")
                } else {
                    format!(" begin; {prelude}; {guard}; {command}; {unguard}; end")
                }
            }
        }
        ShellType::PowerShell => {
            // `.` runs the block in the current scope (variables persist, like the venv
            // activation the agent may run). A command that would end the host (`exit`,
            // `[Environment]::Exit`, ...) runs in a child pwsh instead (#941).
            let save = pwsh_save();
            let over = pwsh_override();
            let mut restore = pwsh_restore().to_owned();
            let capture = PWSH_CAPTURE;
            let reraise = PWSH_RERAISE;
            let child;
            let command = if subshell {
                child = pwsh_child_call(command);
                restore
                    .push_str("; [Environment]::SetEnvironmentVariable('__PHOSPHOR_AP_CWD',$null)");
                child.as_str()
            } else {
                command
            };
            let own_line = own_line && !subshell;
            if own_line {
                format!(
                    ". {{ {save}; {over}\ntry {{\n{command}\n{capture}\n}} finally {{ {restore} }}\n}}\n{reraise}"
                )
            } else {
                format!(
                    ". {{ {save}; {over}; try {{ {command}; {capture} }} finally {{ {restore} }} }}; {reraise}"
                )
            }
        }
    }
}

/// How [`pwsh_scan`] treats the inside of quoted text.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PwshScan {
    /// Literals become `''` and comments vanish, so what is left is code. A double-quoted
    /// string that can run code (`$(`) or names `$PID` is kept: erring toward "might end the
    /// shell" is free, a miss is not.
    Code,
    /// Literals are kept but their separators and spaces are neutralised, so a quoted command
    /// name (`& 'pwsh'`) stays a word of its own and `'a; b'` cannot split a statement.
    Words,
}

/// A conservative, PowerShell-aware pass over `command`: removes comments, here-strings and
/// quoted literals (or neutralises them, see [`PwshScan`]). `None` when a literal or block
/// comment is unterminated, which callers treat as "might end the host".
fn pwsh_scan(command: &str, mode: PwshScan) -> Option<String> {
    let chars: Vec<char> = command.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    let neutral = |c: char| {
        if c.is_whitespace() || matches!(c, ';' | '|' | '&') {
            '\u{1}'
        } else {
            c
        }
    };
    let runs_code = |body: &str| {
        let l = body.to_ascii_lowercase();
        l.contains("$(") || l.contains("$pid") || l.contains("${pid")
    };
    while i < chars.len() {
        let c = chars[i];
        let prev = out.chars().last();
        match c {
            '`' => {
                // Escape: the next character is not syntax.
                i += 2;
            }
            '<' if chars.get(i + 1) == Some(&'#') => {
                let rest: String = chars[i + 2..].iter().collect();
                let end = rest.find("#>")?;
                i += 2 + rest[..end].chars().count() + 2;
            }
            '#' if prev
                .is_none_or(|p| p.is_whitespace() || matches!(p, ';' | '(' | '{' | '|' | '&')) =>
            {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '@' if matches!(chars.get(i + 1), Some('\'' | '"'))
                && matches!(chars.get(i + 2), Some('\n' | '\r')) =>
            {
                let q = chars[i + 1];
                let rest: String = chars[i + 2..].iter().collect();
                let end_marker = format!("\n{q}@");
                let end = rest.find(&end_marker)?;
                let body = &rest[..end];
                match mode {
                    PwshScan::Code if q == '\'' || !runs_code(body) => {
                        out.push(q);
                        out.push(q);
                    }
                    PwshScan::Code => {
                        out.push(q);
                        out.push_str(body);
                        out.push(q);
                    }
                    PwshScan::Words => {
                        out.push(q);
                        out.extend(body.chars().map(neutral));
                        out.push(q);
                    }
                }
                i += 2 + body.chars().count() + end_marker.chars().count();
            }
            '\'' | '"' => {
                let q = c;
                let mut j = i + 1;
                let mut body = String::new();
                loop {
                    let d = *chars.get(j)?;
                    if d == q {
                        if chars.get(j + 1) == Some(&q) {
                            body.push(d);
                            body.push(d);
                            j += 2;
                            continue;
                        }
                        break;
                    }
                    if d == '`' && q == '"' {
                        body.push(d);
                        if let Some(&e) = chars.get(j + 1) {
                            body.push(e);
                        }
                        j += 2;
                        continue;
                    }
                    body.push(d);
                    j += 1;
                }
                match mode {
                    PwshScan::Code if q == '\'' || !runs_code(&body) => {
                        out.push(q);
                        out.push(q);
                    }
                    PwshScan::Code => {
                        out.push(q);
                        out.push_str(&body);
                        out.push(q);
                    }
                    PwshScan::Words => {
                        out.push(q);
                        out.extend(body.chars().map(neutral));
                        out.push(q);
                    }
                }
                i = j + 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    Some(out)
}

/// Whether the word starting at byte `at` of `text` is in command position: first on its line
/// or after `;`, `{`, `}`, `(`, `|` or `&`.
fn pwsh_command_position(text: &str, at: usize) -> bool {
    match text[..at].trim_end_matches([' ', '\t']).chars().last() {
        None => true,
        Some(c) => matches!(c, '\n' | '\r' | ';' | '{' | '}' | '(' | '|' | '&'),
    }
}

/// Dot-sourced files whose content is known not to end the shell: dot-sourcing them in the
/// user's own scope is the point (a virtualenv activation, the profile).
fn pwsh_safe_dot_source(raw_target: &str) -> bool {
    let base = raw_target
        .trim_matches(['\'', '"'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        base.as_str(),
        "activate.ps1" | "activate" | "$profile" | "profile.ps1"
    )
}

/// Whether a PowerShell command MIGHT end or replace the host (`exit`, `[Environment]::Exit`,
/// `$host.SetShouldExit`, `Stop-Process`/`kill` of anything, `$PID` anywhere, dynamic
/// invocation, a dot-sourced script) outside literals and comments. Command-position matching
/// cannot be complete for PowerShell (`try { } catch { exit 1 }`, `% { exit }`,
/// `switch { default { exit } }`, `iex $c`, ...), so this over-approximates: a false positive
/// only costs that one command's `cd` / `$env:` persistence, a miss kills the user's shell.
fn pwsh_may_end_host(command: &str) -> bool {
    pwsh_may_end_host_in(command, true)
}

/// The raw targets of `. file` dot-sourcing statements (quotes removed), `{` blocks excluded.
fn pwsh_dot_source_targets(command: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let bytes: Vec<(usize, char)> = command.char_indices().collect();
    for (k, &(at, c)) in bytes.iter().enumerate() {
        let next = bytes.get(k + 1).map(|n| n.1);
        if c == '.'
            && next.is_some_and(|n| n == ' ' || n == '\t')
            && pwsh_command_position(command, at)
        {
            let rest = command[at + 1..].trim_start_matches([' ', '\t']);
            if rest.starts_with('{') {
                continue;
            }
            let target: String = match rest.chars().next() {
                Some(q @ ('\'' | '"')) => rest[1..].chars().take_while(|&d| d != q).collect(),
                _ => rest
                    .chars()
                    .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | '|' | ')'))
                    .collect(),
            };
            targets.push(target);
        }
    }
    targets
}

/// A dot-sourced file that can be read here and whose text holds nothing that might end the host
/// (no `exit`, `$PID`, dynamic invocation, nested dot-sourcing, ...) runs in the user's pwsh,
/// which has no `exit` guard; anything else, readable or not, goes to the child.
fn pwsh_sourced_file_is_benign(target: &str) -> bool {
    read_sourced_file(target).is_some_and(|text| !pwsh_may_end_host_in(&text, false))
}

fn pwsh_may_end_host_in(command: &str, follow_sources: bool) -> bool {
    let Some(code) = pwsh_scan(command, PwshScan::Code) else {
        return true;
    };
    let lower = code.to_ascii_lowercase();
    let collapsed: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    const NEEDLES: &[&str] = &[
        "::exit(",
        ".exit(",
        ".kill(",
        "failfast",
        "setshouldexit",
        "$pid",
        "${pid}",
        "[scriptblock]::create",
        ".invoke(",
        "invokescript",
        "invokecommand",
    ];
    if NEEDLES.iter().any(|n| collapsed.contains(n)) {
        return true;
    }
    // Words, with their byte offsets.
    let is_delim = |c: char| {
        c.is_whitespace()
            || matches!(
                c,
                ';' | '|' | '&' | '(' | ')' | '{' | '}' | ',' | '"' | '\''
            )
    };
    let mut start = None;
    let mut words: Vec<(usize, &str)> = Vec::new();
    for (idx, c) in lower.char_indices() {
        match (start, is_delim(c)) {
            (None, false) => start = Some(idx),
            (Some(st), true) => {
                words.push((st, &lower[st..idx]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(st) = start {
        words.push((st, &lower[st..]));
    }
    for (at, word) in &words {
        let cmd_pos = pwsh_command_position(&lower, *at);
        match *word {
            "exit" | "kill" | "spps" | "stop-process" | "iex" | "invoke-expression"
            | "invoke-command" | "icm"
                if cmd_pos =>
            {
                return true;
            }
            _ => {}
        }
    }
    // `& $cmd`, `& (...)`: a command chosen at runtime. And `. file`: dot-sourcing runs the
    // file in the user's scope, where its `exit` ends the host.
    let mut dot_sources = 0usize;
    let raw_chars: Vec<(usize, char)> = lower.char_indices().collect();
    for (k, &(at, c)) in raw_chars.iter().enumerate() {
        let prev = k.checked_sub(1).map(|p| raw_chars[p].1);
        let next = raw_chars.get(k + 1).map(|n| n.1);
        if c == '&' && prev != Some('&') && next != Some('&') {
            let after = lower[at + 1..]
                .trim_start_matches([' ', '\t'])
                .chars()
                .next();
            if matches!(after, Some('$' | '(')) {
                return true;
            }
        }
        if c == '.'
            && next.is_some_and(|n| n == ' ' || n == '\t')
            && pwsh_command_position(&lower, at)
        {
            let target: String = lower[at + 1..]
                .trim_start_matches([' ', '\t'])
                .chars()
                .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | '|' | ')'))
                .collect();
            if !target.starts_with('{') && !pwsh_safe_dot_source(&target) {
                dot_sources += 1;
            }
        }
    }
    if dot_sources > 0 {
        let targets = pwsh_dot_source_targets(command);
        return !(follow_sources
            && targets.len() >= dot_sources
            && targets
                .iter()
                .all(|t| pwsh_safe_dot_source(t) || pwsh_sourced_file_is_benign(t)));
    }
    false
}

/// The statement that runs `command` in a child of the running PowerShell host, with the
/// environment inherited (the pager overrides are process environment) and the working
/// directory carried over (`Set-Location` does not move the process directory). The command
/// travels as `-EncodedCommand` (base64 of UTF-16LE), so no quoting can break it.
fn pwsh_child_call(command: &str) -> String {
    use base64::Engine as _;
    let bytes: Vec<u8> = format!(
        "Microsoft.PowerShell.Management\\Set-Location -LiteralPath $env:__PHOSPHOR_AP_CWD; {command}"
    )
    .encode_utf16()
    .flat_map(u16::to_le_bytes)
    .collect();
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!(
        "$env:__PHOSPHOR_AP_CWD = (Microsoft.PowerShell.Management\\Get-Location -PSProvider FileSystem).ProviderPath; \
         & ([System.Diagnostics.Process]::GetCurrentProcess().MainModule.FileName) \
         -NoProfile -NoLogo -NonInteractive -EncodedCommand {encoded}"
    )
}

/// Whether `command` must keep the subshell wrapper because, at its top level, it ends the
/// shell or changes how it treats errors, which a group in the user's real shell would leak
/// into (or kill) their session. fish has no subshell form, and PowerShell only has the
/// scope difference, so only `exit` matters there.
pub(super) fn needs_subshell(shell_type: ShellType, command: &str) -> bool {
    if shell_type == ShellType::PowerShell {
        return pwsh_may_end_host(command);
    }
    simple_commands(command).iter().any(|words| {
        let Some((name, args)) = command_name_and_args(words) else {
            return false;
        };
        match shell_type {
            ShellType::Fish => fish_command_needs_subshell(name, args),
            _ => posix_command_needs_subshell(name, args),
        }
    })
}

thread_local! {
    /// The session's working directory (local sessions only) while a command is being wrapped,
    /// so a sourced file given by a relative path can be read and scanned.
    static SOURCE_CWD: std::cell::RefCell<Option<std::path::PathBuf>> = const { std::cell::RefCell::new(None) };
}

/// [`wrap_command`], able to read a sourced / dot-sourced file relative to `cwd`.
pub(super) fn wrap_command_in(
    shell_type: Option<ShellType>,
    command: &str,
    cwd: Option<&str>,
) -> String {
    SOURCE_CWD.with(|c| *c.borrow_mut() = cwd.map(std::path::PathBuf::from));
    let wrapped = wrap_command(shell_type, command);
    SOURCE_CWD.with(|c| *c.borrow_mut() = None);
    wrapped
}

/// The first 256 KiB of the file `target` names (`~` expanded, relative to the session's working
/// directory), if it can be read here. `None` for a remote session, a missing or unreadable file.
fn read_sourced_file(target: &str) -> Option<String> {
    use std::io::Read as _;
    let target = target.trim_matches(['\'', '"']);
    let path = if let Some(rest) = target.strip_prefix("~/") {
        std::path::PathBuf::from(std::env::var_os("HOME")?).join(rest)
    } else {
        let p = std::path::PathBuf::from(target);
        if p.is_absolute() {
            p
        } else {
            SOURCE_CWD.with(|c| c.borrow().clone())?.join(p)
        }
    };
    let mut text = String::new();
    std::fs::File::open(path)
        .ok()?
        .take(256 * 1024)
        .read_to_string(&mut text)
        .ok()?;
    Some(text)
}

/// Whether a sourced file visibly holds a hazard that the in-shell `exit` guard cannot stop:
/// `builtin exit` / `command exit`, `exec`, or a `kill` of the shell's own pid. A file that cannot
/// be read (remote session, dynamic path) is run in the shell: the guard covers a plain `exit`.
fn sourced_file_is_hazard(args: &[String], fish: bool) -> bool {
    let Some(target) = args.iter().find(|a| !a.starts_with('-')) else {
        return false;
    };
    let Some(text) = read_sourced_file(target) else {
        return false;
    };
    let mentions_self = ["$$", "$PPID", "$BASHPID", "$fish_pid", "%self"]
        .iter()
        .any(|r| text.contains(r));
    let has_word = |w: &str| {
        text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|t| t == w)
    };
    let _ = fish;
    text.contains("builtin exit")
        || text.contains("command exit")
        || has_word("exec")
        || (has_word("kill") && mentions_self)
}

/// Whether text handed to `eval` is only known at runtime (a variable, a command substitution).
fn is_dynamic_text(text: &str) -> bool {
    text.contains('$') || text.contains('`')
}

fn fish_command_needs_subshell(name: &str, args: &[String]) -> bool {
    // The shell's own pid / job, in a child fish those name the child.
    if args
        .iter()
        .any(|a| a.contains("$fish_pid") || a.contains("%self"))
    {
        return true;
    }
    match name {
        "exit" | "exec" | "return" | "logout" => true,
        // `kill $fish_pid`, `kill %self`: fish has no subshell, so it runs in a child fish
        // whose own pid and `%self` are the child's.
        "kill" => true,
        "source" | "." => sourced_file_is_hazard(args, true),
        // `eval` runs its arguments in the current shell.
        "eval" => {
            let text = args.join(" ");
            !text.is_empty() && (is_dynamic_text(&text) || needs_subshell(ShellType::Fish, &text))
        }
        _ => false,
    }
}

/// Whether `command` would kill the user's own shell in a way no subshell can contain: `$$` and
/// `$PPID` name the user's shell (or its parent) even inside `( ... )`, and `kill 0` / `kill -1`
/// signal the whole process group / every process. Refused with a tool error rather than run.
/// bash / zsh only: in fish the command runs in a child fish whose own pid it names.
pub(super) fn kills_own_shell(shell_type: Option<ShellType>, command: &str) -> Option<String> {
    if matches!(shell_type, Some(ShellType::Fish | ShellType::PowerShell)) {
        return None;
    }
    const SELF_REFS: &[&str] = &["$$", "$PPID", "${PPID}", "${$}", "$BASHPID", "${BASHPID}"];
    let mentions_self = |text: &str| SELF_REFS.iter().any(|r| text.contains(r));
    simple_commands(command).iter().find_map(|words| {
        let (name, args) = command_name_and_args(words)?;
        let base = name.rsplit('/').next().unwrap_or(name);
        match base {
            "kill" => {
                let bad = args
                    .iter()
                    .any(|a| mentions_self(a) || a == "0" || a == "-1");
                bad.then(|| "kill".to_owned())
            }
            // A trap handler is run later but names the same `$$`.
            "trap" | "eval" => {
                let text = args.join(" ");
                (text.contains("kill") && mentions_self(&text)).then(|| "kill".to_owned())
            }
            _ => None,
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
        return pwsh_interactive_shell(command);
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

/// PowerShell has no `exec`, but `pwsh`, `& pwsh -NoProfile` or `cmd` with nothing to run is the
/// same trap: a nested interactive shell that never ends on its own and has no Phosphor hooks.
/// Only a shell that STARTS a pipeline counts: `curl ... | sh` and `echo hi | bash` read their
/// script from the pipe and end.
fn pwsh_interactive_shell(command: &str) -> Option<String> {
    const SHELLS: &[&str] = &["pwsh", "powershell", "cmd", "bash", "zsh", "fish", "sh"];
    const INTERACTIVE_ARGS: &[&str] = &[
        "-nologo",
        "-noprofile",
        "-noexit",
        "-interactive",
        "-i",
        "-l",
        "-login",
        "-nop",
        "-nol",
    ];
    let text = pwsh_scan(command, PwshScan::Words)?;
    // Split into statements, remembering whether a single `|` fed each one.
    let chars: Vec<char> = text.chars().collect();
    let mut segments: Vec<(bool, String)> = vec![(false, String::new())];
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            '|' if next == Some('|') => {
                segments.push((false, String::new()));
                i += 2;
            }
            '&' if next == Some('&') => {
                segments.push((false, String::new()));
                i += 2;
            }
            '|' => {
                segments.push((true, String::new()));
                i += 1;
            }
            ';' | '\n' | '\r' => {
                segments.push((false, String::new()));
                i += 1;
            }
            c => {
                segments.last_mut().unwrap().1.push(c);
                i += 1;
            }
        }
    }
    segments.iter().find_map(|(piped, segment)| {
        if *piped {
            return None;
        }
        let mut words = segment.split_whitespace().peekable();
        while matches!(words.peek(), Some(&"." | &"&")) {
            words.next();
        }
        let name = words.next()?.trim_matches(['\'', '"']);
        let base = name.rsplit(['/', '\\']).next().unwrap_or(name);
        let base = base.to_ascii_lowercase();
        let base = base.strip_suffix(".exe").unwrap_or(&base);
        (SHELLS.contains(&base)
            && words.all(|a| INTERACTIVE_ARGS.contains(&a.to_ascii_lowercase().as_str())))
        .then(|| base.to_owned())
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
        "exit" | "exec" | "trap" | "ulimit" | "return" | "logout" => true,
        // A sourced file runs in the shell itself (its `exit` returns, see `POSIX_GUARD_SAVE`),
        // so its exports persist. Only a file that can be SEEN to hold a hazard the guard cannot
        // stop (`builtin exit`, `kill $$`, `exec`) goes to the subshell.
        "source" | "." => sourced_file_is_hazard(args, false),
        // `eval` runs its arguments in the current shell, so `eval exit` or `eval "set -e"`
        // is `exit` / `set -e`. Text only known at runtime (`eval "$v"`) might be anything.
        "eval" => {
            let text = args.join(" ");
            !text.is_empty() && (is_dynamic_text(&text) || needs_subshell(ShellType::Bash, &text))
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
