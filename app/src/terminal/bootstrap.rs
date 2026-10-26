use std::borrow::Cow;

use itertools::Itertools;
use lazy_static::lazy_static;
use memo_map::MemoMap;
use rand::Rng as _;
use warp_core::SessionId;
use warpui::{AppContext, AssetProvider, SingletonEntity};

use crate::{
    env_vars::EnvVar,
    terminal::{session_settings::SessionSettings, shell::ShellType},
};

#[cfg(feature = "local_fs")]
use super::{
    model::session::{BootstrapSessionType, SessionInfo},
    warpify::settings::{PIPENV_SUBSHELL_COMMAND_REGEX, POETRY_SUBSHELL_COMMAND_REGEX},
};

lazy_static! {
    /// A memoized cache of the fully-interpolated bootstrap script for each
    /// shell.  We store the full version here as an optimization so that we
    /// don't have to regenerate it every time we spawn a shell.
    static ref BOOTSTRAP_CACHE: MemoMap<ShellType, Vec<u8>> = Default::default();
}

/// This can sometimes appear in the beginning of files. If it gets written into the PTY, it causes
/// errors
const BYTE_ORDER_MARK: &str = "\u{FEFF}";

/// Whether this session is a subshell running inside a container
/// (`docker exec`/`docker run`, `podman ...`).
///
/// Ported from the pin (`42effe840:app/src/terminal/bootstrap.rs:31-40`).
/// A container subshell shares the host's PTY but NOT the host's filesystem, so
/// anything the host writes to its own temp directory is unreadable from inside
/// the container.
///
/// The pin has a second consumer for this predicate:
/// `writeable_pty/pty_controller.rs`'s `write_bootstrap_script_to_shell` chunks
/// the bootstrap into 4KB writes with 50ms gaps for container subshells,
/// because the double-PTY proxy in `docker/podman exec -it` drops data on
/// large writes. That consumer is now ported too.
#[cfg(feature = "local_fs")]
pub fn is_container_subshell(session_info: &SessionInfo) -> bool {
    session_info.subshell_info.as_ref().is_some_and(|info| {
        let first_token = info
            .spawning_command
            .split_ascii_whitespace()
            .next()
            .unwrap_or("");
        first_token == "docker" || first_token == "podman"
    })
}

/// Returns `true` if Zap should use an RC-file based bootstrap (e.g. dump the bootstrap script to
/// a temp file and `source` it) for a newly spawned session with the given `shell_type`, and
/// associated `session_type` and `subshell_initialization_info`.
///
/// This returns `true` for local Fish/Pwsh shells and local subshells spawned via `poetry shell`.
///
/// We use RC-file based bootstrap for local Fish shells because there is a long-standing bug which
/// causes an explosion of formatting output when a command is longer than a screen height. (See
/// https://github.com/fish-shell/fish-shell/issues/7296 for more) This multiplication of output
/// makes our bootstrap take a long time, as we need to process all of that output (even though
/// most of it is irrelevant). To avoid the impact on bootstrap time, we write the script to a
/// temporary file and then source that file (which avoids writing the long script to the shell
/// itself).
///
/// We use RC-file based bootstrap for PowerShell because chars written to the PTY get randomly
/// ignored. See PLAT-757 in Linear.
///
/// We use RC-file based bootstrap for `poetry shell` subshells because the underlying library used
/// to spawn a subshell by `poetry shell` uses blocking PTY reads and writes, which results in a
/// deadlock when attempting to write the whole bootstrap script to the PTY; RC file-based
/// bootstrap is the only known way to bootstrap such subshells successfully.
///
/// We use RC-file based bootstrap for MSYS2 because it has slow PTY throughput.
#[cfg(feature = "local_fs")]
pub fn should_use_rc_file_bootstrap_method(
    shell_type: ShellType,
    session_info: &SessionInfo,
) -> bool {
    use super::ShellLaunchData;

    // Container subshells cannot access host temp files, so the RC-file
    // method is never viable for them: the path handed to `source` names a
    // file on the host, which does not exist inside the container. Matches the
    // pin (`42effe840:...:72-76`). Reachable through the `shell_type == Fish`
    // and `shell_type == PowerShell` disjuncts below (e.g.
    // `docker run -it <image> fish`) and, on Windows, through the
    // `subshell_initialization_info.is_some() && cfg!(windows)` zsh disjunct --
    // NOT for a plain `docker run -it <image> bash`, which never selected the
    // RC-file method anyway.
    if is_container_subshell(session_info) {
        return false;
    }

    let session_type = &session_info.session_type;
    match session_type {
        BootstrapSessionType::Local => {
            let subshell_initialization_info = session_info.subshell_info.as_ref();
            let is_poetry_subshell = subshell_initialization_info
                .as_ref()
                .map(|info| POETRY_SUBSHELL_COMMAND_REGEX.is_match(info.spawning_command.as_str()))
                .unwrap_or(false);
            let is_pipenv_subshell = subshell_initialization_info
                .as_ref()
                .map(|info| PIPENV_SUBSHELL_COMMAND_REGEX.is_match(info.spawning_command.as_str()))
                .unwrap_or(false);
            let is_msys2 = session_info
                .launch_data
                .as_ref()
                .is_some_and(|data| matches!(data, ShellLaunchData::MSYS2 { .. }));
            shell_type == ShellType::Fish
                || shell_type == ShellType::PowerShell
                || is_poetry_subshell
                || ((is_pipenv_subshell
                    || (subshell_initialization_info.is_some() && cfg!(windows)))
                    && shell_type == ShellType::Zsh)
                || is_msys2
        }
        BootstrapSessionType::WarpifiedRemote => false,
    }
}

/// Returns the bootstrap script that should be used when initializing a shell
/// of the given type.
///
/// This supports a very basic form of interpolation:
///
/// ```shell
/// #include bundled/bootstrap/zsh_body.sh
/// ```
///
/// The directive above instructs this function to replace that line with the
/// contents of the file in our asset cache with the path `bundled/bootstrap/zsh_body.sh`.
///
/// At the moment, this interpolation is only performed for the top-level file,
/// and is not performed recursively, but it would be useful to add such support
/// in the future.
pub fn script_for_shell(shell_type: ShellType, assets: &dyn AssetProvider) -> Cow<'static, [u8]> {
    let file = match shell_type {
        ShellType::Bash => "bash.sh",
        ShellType::Zsh => "zsh.sh",
        ShellType::Fish => "fish.sh",
        ShellType::PowerShell => "pwsh.ps1",
    };

    BOOTSTRAP_CACHE
        .get_or_insert(&shell_type, || {
            let file_path = format!("bundled/bootstrap/{file}");
            let bootstrap = assets
                .get(&file_path)
                .unwrap_or_else(|_| panic!("failed to retrieve {file_path} from assets"));

            // Interpret the file as UTF-8.  We do this in an unchecked way
            // for performance, expecting that any issues here will be caught by
            // unit tests.
            let bootstrap = unsafe { String::from_utf8_unchecked(bootstrap.to_vec()) };

            let additional_files = memo_map::MemoMap::new();

            // Parse through the file, looking for any lines which start with
            // "#include", and replacing that line with the contents of the file
            // located at the path specified.
            //
            // We trim most leading and all trailing whitespace from lines, and
            // drop all empty lines and lines that only contain a comment.  We
            // keep a single leading space on each line, if one exists, to
            // avoid interfering with histignorespace behavior.
            //
            // This minimizes the number of bytes we send over the pty during the
            // bootstrap process.
            fn trim_and_borrow_line(mut line: &str) -> Cow<'_, str> {
                let len = line.len();
                let trimmed_len = line.trim_start().len();
                if trimmed_len < len {
                    let trimmed_chars = len - trimmed_len;
                    line = &line[trimmed_chars - 1..];
                }
                Cow::Borrowed(line.trim_end())
            }
            let mut script = bootstrap
                .trim_start_matches(BYTE_ORDER_MARK)
                .split('\n')
                .map(trim_and_borrow_line)
                .flat_map(|line| {
                    if let Some(path) = line.strip_prefix("#include ") {
                        additional_files
                            .get_or_insert(path, || {
                                let data = assets.get(path).unwrap_or_else(|_| {
                                    panic!("failed to retrieve {path} from assets")
                                });
                                let data_string =
                                    unsafe { String::from_utf8_unchecked(data.to_vec()) };
                                data_string.replace(
                                    "@@USING_CON_PTY_BOOLEAN@@",
                                    &(cfg!(windows).to_string()),
                                )
                            })
                            .split('\n')
                            .map(trim_and_borrow_line)
                            .collect_vec()
                    } else {
                        vec![line]
                    }
                })
                // Filter out empty lines and comments, to minimize the amount
                // of data we send over the pty during the bootstrap process.
                .filter(|line| {
                    let line = line.trim_start();
                    !(line.is_empty()
                        || line.starts_with('#')
                        || shell_type == ShellType::PowerShell
                            && line
                                .starts_with("[Diagnostics.CodeAnalysis.SuppressMessageAttribute"))
                })
                .join("\n");

            // Make sure there's a newline at the end of the bootstrap script,
            // otherwise we'll never submit the final line to the shell.
            script.push('\n');
            script.into_bytes()
        })
        .into()
}

/// Generates a cryptographically random session ID for use as both a session
/// identifier and an integrity token for DCS hook validation.
///
/// See #419 / #532: this ID is baked into the shell's init script (via
/// [`SESSION_ID_PLACEHOLDER`]) before the shell can write anything back, and
/// registered with the owning [`crate::terminal::model::TerminalModel`] at the
/// same time, so a later DCS hook can be checked against it.
pub fn generate_session_id() -> SessionId {
    let mut rng = rand::thread_rng();
    loop {
        let session_id = rng.r#gen::<u64>();
        if session_id != 0 {
            return SessionId::from(session_id);
        }
    }
}

/// Placeholder in init shell scripts that gets replaced with the client-generated session ID.
pub(crate) const SESSION_ID_PLACEHOLDER: &str = "@@WARP_SESSION_ID@@";

/// Returns the init shell script for the given `shell_type` (e.g. the script that emits the
/// InitShell DCS hook).
///
/// The returned script is one line and, for shells that need it, has escaped single-quotes for the
/// purposes of being passed as a single-quoted argument to 'eval'.
pub fn init_shell_script_for_shell(
    shell_type: ShellType,
    assets: &dyn AssetProvider,
    session_id: SessionId,
) -> String {
    let script = match shell_type {
        ShellType::Zsh => load_and_escape_script("bundled/bootstrap/zsh_init_shell.sh", assets),
        ShellType::Bash => load_and_escape_script("bundled/bootstrap/bash_init_shell.sh", assets),
        ShellType::Fish => {
            load_and_escape_fish_script("bundled/bootstrap/fish_init_shell.sh", assets)
        }
        ShellType::PowerShell => load_script("bundled/bootstrap/pwsh_init_shell.ps1", assets),
    };
    script.replace(SESSION_ID_PLACEHOLDER, &session_id.as_u64().to_string())
}

/// Longest line (bytes, excluding the terminator) the subshell bootstrap writes to a pty.
///
/// The bootstrap is typed into a shell whose tty may still be canonical (cooked): the new shell
/// has not started its line editor yet. A canonical line is limited to `MAX_CANON` -- 1024 on
/// macOS, 4096 on Linux -- and a longer one is silently truncated, which leaves the shell
/// at a continuation prompt with the tail of the script lost (#962). Every line stays well under
/// the smaller limit.
pub(crate) const MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES: usize = 900;

/// Budget for the quoted payload of one chunk line (after single-quote escaping). The fixed text
/// around it (the assignment) is well under 100 bytes.
const SUBSHELL_CHUNK_PAYLOAD_BYTES: usize = 700;

/// Returns the command to be used to emit the InitShell hook for a new subshell session, as a
/// single string whose lines are separated by CR.
///
/// See [`init_subshell_command_lines`], which the app uses so it can pace the writes.
pub fn init_subshell_command(
    shell_type: Option<ShellType>,
    vars: &[EnvVar],
    session_id: SessionId,
    ctx: &AppContext,
) -> String {
    init_subshell_command_lines(shell_type, vars, session_id, ctx).join("\r")
}

/// Returns the lines to be written (each followed by Enter) to emit the InitShell hook for a new
/// subshell session.
///
/// If `shell_type` is `Some()`, returns shell type-specific lines (e.g. valid for bash, fish,
/// zsh, or pwsh). Otherwise, returns a shell type-agnostic command that emits the right
/// `InitShell` hook based on the shell it is evaluated in.
///
/// For bash, zsh and fish the script is longer than a canonical tty line may be (#962), so it is
/// accumulated in a shell variable over several short lines and evaluated by the last one. Every
/// line is below [`MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES`], starts with a space (so histignorespace
/// and fish skip it) and none of them ever deletes a history entry (#967).
pub fn init_subshell_command_lines(
    shell_type: Option<ShellType>,
    vars: &[EnvVar],
    session_id: SessionId,
    ctx: &AppContext,
) -> Vec<String> {
    match shell_type {
        Some(ShellType::PowerShell) => {
            let subshell_script = init_subshell_script_for_shell(
                ShellType::PowerShell,
                &crate::ASSETS,
                vars,
                session_id,
                ctx,
            );
            // The POSIX `[ -z $WARP_BOOTSTRAPPED ] && eval '...'` guard below is not valid
            // PowerShell syntax (#800) -- `$global:WARP_BOOTSTRAPPED` (set by `pwsh.ps1`,
            // see its `$global:WARP_BOOTSTRAPPED = 1`) is PowerShell's equivalent guard
            // variable, and the subshell script is raw PowerShell source rather than a
            // single-quoted `eval` argument, so no escaping is needed here.
            // PSReadLine appends every accepted line to the history file before it runs, so the
            // big line below would be saved. A short first line installs an AddToHistoryHandler
            // (chained to any the user had) that refuses the bootstrap line and the line that
            // later sources the temp bootstrap file; nothing rewrites the user's history file.
            vec![
                POWERSHELL_HISTORY_HANDLER_LINE.to_owned(),
                format!(" if (-not $global:WARP_BOOTSTRAPPED) {{ {subshell_script} }}"),
            ]
        }
        Some(shell_type @ (ShellType::Bash | ShellType::Zsh | ShellType::Fish)) => {
            let raw_script =
                init_subshell_raw_script(shell_type, &crate::ASSETS, vars, session_id, ctx);
            chunked_eval_lines(shell_type, &raw_script)
        }
        None => vec![
            UNKNOWN_SHELL_HISTORY_PRELUDE.to_owned(),
            init_subshell_script_for_unknown_shell(&crate::ASSETS, session_id),
        ],
    }
}

/// First line of the PowerShell subshell bootstrap (#967): installs an `AddToHistoryHandler`,
/// chained to the user's own, that refuses the guarded init line and the ` . <tmp>.ps1` line the
/// app types afterwards to source the bootstrap file (`.tmp` plus six characters, then `.ps1`:
/// the shape `tempfile` gives the file). The handler cannot cover the line that installs it, so
/// that line ends in a comment word PSReadLine's default handler keeps out of the history *file*
/// (`apikey`: lines matching its sensitive pattern are saved to memory only on PSReadLine 2.2 and
/// later). A user-replaced handler or an older PSReadLine still saves this one short line; nothing
/// ever rewrites or deletes history.
const POWERSHELL_HISTORY_HANDLER_LINE: &str = r#" if (-not $global:WARP_BOOTSTRAPPED) { $global:_warpPrevHistoryHandler = (PSReadLine\Get-PSReadLineOption).AddToHistoryHandler; PSReadLine\Set-PSReadLineOption -AddToHistoryHandler { param([string]$l) $t = $l.TrimStart(); if ($t.StartsWith('if (-not $global:WARP_BOOTSTRAPPED) {', [System.StringComparison]::Ordinal) -or $t -match '^\.\s+[^\r\n]*[\\/]\.tmp[A-Za-z0-9]{6}\.ps1\s*$') { return $false }; if ($global:_warpPrevHistoryHandler) { return (& $global:_warpPrevHistoryHandler $l) }; return $true } } # apikey"#;

/// Single-quote escapes `text` for embedding in a single-quoted word of `shell_type`.
fn escape_for_single_quotes(shell_type: ShellType, text: &str) -> String {
    match shell_type {
        // fish processes `\\` and `\'` inside single quotes (#898), so a literal backslash is
        // doubled first.
        ShellType::Fish => text.replace('\\', r"\\").replace('\'', r#"'"'"'"#),
        _ => text.replace('\'', r#"'"'"'"#),
    }
}

/// Printed by the last bootstrap line when the accumulated script is not the length that was sent
/// (so it is not evaluated). Nothing else tells the user why the pane did not warpify; the app
/// also stops holding input after a while (#960). It is one `printf` with the count as an argument.
const SKIPPED_NOTICE: &str = "builtin printf '\\nPhosphor: subshell setup skipped, the script arrived incomplete (%s characters). Type exit to leave this shell.\\n' \"${#_wbs}\"";

/// Splits `raw_script` into lines that rebuild it in `$_wbs` and then evaluate it.
fn chunked_eval_lines(shell_type: ShellType, raw_script: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in raw_script.chars() {
        let escaped = escape_for_single_quotes(shell_type, ch.encode_utf8(&mut [0; 4]));
        if current.len() + escaped.len() > SUBSHELL_CHUNK_PAYLOAD_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        current.push_str(&escaped);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    // Nothing here ever deletes a history entry. Every line starts with a space; bash and zsh
    // only honour that when HISTCONTROL has ignorespace / hist_ignore_space is set, so a short
    // first line turns that on (saving the prior state) and the last line puts it back. The first
    // line carries no bootstrap text, so it is the only thing a shell without the option on can
    // record. fish skips space-led lines on its own.
    let mut lines = Vec::with_capacity(chunks.len() + 2);
    match shell_type {
        ShellType::Bash => {
            lines.push(
                // `__hc0` may already hold the state from before the probe's prelude.
                " __hc0=${__hc0-${HISTCONTROL-}}; HISTCONTROL=ignorespace:${HISTCONTROL-}".to_owned(),
            )
        }
        ShellType::Zsh => lines.push(
            " [ -n \"${__whi-}\" ] || { __whi=0; [[ -o hist_ignore_space ]] && __whi=1; }; setopt hist_ignore_space"
                .to_owned(),
        ),
        _ => {}
    }
    for (index, chunk) in chunks.iter().enumerate() {
        // The first chunk assigns (so a stale `$_wbs` is never prepended); the rest append.
        let assign = match (shell_type, index) {
            (ShellType::Fish, 0) => "set -g _wbs ".to_owned(),
            (ShellType::Fish, _) => "set -g _wbs \"$_wbs\"".to_owned(),
            (_, 0) => "_wbs=".to_owned(),
            (_, _) => "_wbs=$_wbs".to_owned(),
        };
        lines.push(format!(" {assign}'{chunk}'"));
    }
    // The script is only evaluated when it arrived whole: its length (in characters or bytes,
    // whichever the shell's locale counts) must match.
    let chars = raw_script.chars().count();
    let bytes = raw_script.len();
    lines.push(match shell_type {
        // `${WARP_BOOTSTRAPPED-}`, not a bare `$WARP_BOOTSTRAPPED`: this is typed into a shell
        // whose rcfiles already ran, and under their `set -u` / `setopt nounset` the bare
        // expansion aborts the line, so the subshell is never Warpified (#839).
        ShellType::Bash => format!(
            " HISTCONTROL=$__hc0; unset __hc0; if [ -z \"${{WARP_BOOTSTRAPPED-}}\" ]; then if [ \"${{#_wbs}}\" -eq {chars} ] || [ \"${{#_wbs}}\" -eq {bytes} ]; then eval \"$_wbs\"; else {SKIPPED_NOTICE}; fi; fi; unset _wbs"
        ),
        ShellType::Zsh => format!(
            " if [ \"$__whi\" = 1 ]; then WARP_HIST_IGNORE_SPACE_USER=1; else unsetopt hist_ignore_space; fi; unset __whi; if [ -z \"${{WARP_BOOTSTRAPPED-}}\" ]; then if [ \"${{#_wbs}}\" -eq {chars} ] || [ \"${{#_wbs}}\" -eq {bytes} ]; then eval \"$_wbs\"; else {SKIPPED_NOTICE}; fi; fi; unset _wbs WARP_HIST_IGNORE_SPACE_USER"
        ),
        // fish cannot parse `${...}` and has no nounset, so it keeps the bare expansion.
        _ => format!(
            " [ -z $WARP_BOOTSTRAPPED ] && contains -- (string length -- $_wbs) {chars} {bytes} && eval $_wbs; set -e _wbs"
        ),
    });
    lines
}

/// Returns the init subshell script for the given `shell_type` (e.g. the script that emits the
/// subshell version of the InitShell DCS hook).
///
/// The returned script is one line. For bash/zsh/fish it has escaped single-quotes for the
/// purposes of being passed as a single-quoted argument to 'eval'; for PowerShell, which its
/// caller writes out as raw source rather than wrapping in `eval`, it is unescaped (see
/// `init_subshell_command`).
fn init_subshell_script_for_shell(
    shell_type: ShellType,
    assets: &dyn AssetProvider,
    env_vars: &[EnvVar],
    session_id: SessionId,
    ctx: &AppContext,
) -> String {
    let honor_ps1 = *SessionSettings::as_ref(ctx).honor_ps1;
    let honor_ps1_env_var_value = if honor_ps1 { "1" } else { "0" };

    let env_vars_init = env_vars
        .iter()
        .map(|var| var.get_initialization_string(shell_type))
        .collect_vec()
        .join(" ");

    // Prepend environment variable settings to the script. PowerShell needs its own `$env:`
    // assignment syntax here -- the POSIX `export NAME=value;` form the other shells use is
    // not valid PowerShell and would abort the subshell bootstrap before the InitShell hook
    // below it ever runs (#800).
    let env_setup_script = match shell_type {
        ShellType::PowerShell => {
            format!("$env:WARP_HONOR_PS1 = '{honor_ps1_env_var_value}'; {env_vars_init}")
        }
        ShellType::Bash | ShellType::Zsh | ShellType::Fish => {
            format!("export WARP_HONOR_PS1={honor_ps1_env_var_value}; {env_vars_init}")
        }
    };

    // Load and escape the shell-specific init script
    let shell_init_script = match shell_type {
        ShellType::Zsh => load_and_escape_script("bundled/bootstrap/zsh_init_subshell.sh", assets),
        ShellType::Bash => {
            load_and_escape_script("bundled/bootstrap/bash_init_subshell.sh", assets)
        }
        ShellType::Fish => {
            load_and_escape_fish_script("bundled/bootstrap/fish_init_subshell.sh", assets)
        }
        ShellType::PowerShell => load_script("bundled/bootstrap/pwsh_init_subshell.ps1", assets),
    };
    let shell_init_script =
        shell_init_script.replace(SESSION_ID_PLACEHOLDER, &session_id.as_u64().to_string());

    // Combine the environment setup script with the shell-specific init script
    format!("{env_setup_script} {shell_init_script}")
}

/// Like [`init_subshell_script_for_shell`], but returns the script as plain shell source: no
/// quote escaping and not wrapped for `eval` (the caller chunks and quotes it).
fn init_subshell_raw_script(
    shell_type: ShellType,
    assets: &dyn AssetProvider,
    env_vars: &[EnvVar],
    session_id: SessionId,
    ctx: &AppContext,
) -> String {
    let honor_ps1 = *SessionSettings::as_ref(ctx).honor_ps1;
    let honor_ps1_env_var_value = if honor_ps1 { "1" } else { "0" };
    let env_vars_init = env_vars
        .iter()
        .map(|var| var.get_initialization_string(shell_type))
        .collect_vec()
        .join(" ");
    let file = match shell_type {
        ShellType::Zsh => "bundled/bootstrap/zsh_init_subshell.sh",
        ShellType::Bash => "bundled/bootstrap/bash_init_subshell.sh",
        ShellType::Fish => "bundled/bootstrap/fish_init_subshell.sh",
        ShellType::PowerShell => "bundled/bootstrap/pwsh_init_subshell.ps1",
    };
    let script = load_script(file, assets)
        .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string()))
        .replace(SESSION_ID_PLACEHOLDER, &session_id.as_u64().to_string());
    format!("export WARP_HONOR_PS1={honor_ps1_env_var_value}; {env_vars_init} {script}")
}

/// Returns the init subshell script for an unknown shell which detects the shell type.
///
/// The returned script is one line and has escaped single-quotes for the purposes of being passed
/// as a single-quoted argument to 'eval'.
///
/// The caller must have registered `session_id` with the owning `TerminalModel` first: it is
/// baked into the emitted `InitSubshell` hook so the hook can be validated against an ID this
/// client minted.
fn init_subshell_script_for_unknown_shell(
    assets: &dyn AssetProvider,
    session_id: SessionId,
) -> String {
    // Load and escape the shell-specific init script
    // The leading space keeps the probe out of fish's history and out of bash/zsh when
    // HISTCONTROL has ignorespace / hist_ignore_space is set. It is one shell-agnostic line, so
    // it cannot first turn that on; it never deletes a history entry (an entry deleted by
    // number could be the user's own).
    let script = load_and_escape_script("bundled/bootstrap/unknown_init_subshell.sh", assets)
        .replace("HOOK_NAME", "InitSubshell")
        .replace(SESSION_ID_PLACEHOLDER, &session_id.as_u64().to_string());
    format!(" {script}; {UNKNOWN_SHELL_HISTORY_RESTORE}")
}

/// First line of the shell-agnostic probe (#967): a short line that turns history-ignores-space on
/// in bash (`HISTCONTROL`) and zsh (`hist_ignore_space`) so the long probe line after it is not
/// recorded. A shell records a line as it reads it, before running it, so this one line can still
/// land in history when the option was off; nothing is ever deleted, and the line carries no
/// bootstrap text. The prior state goes in `__hc0` / `__whi`, which the known-shell stage reuses
/// and [`UNKNOWN_SHELL_HISTORY_RESTORE`] puts back. Every `${}` sits inside a single-quoted `eval`
/// that only bash (`shopt` exists) or zsh (`setopt` exists) reaches, so fish and POSIX sh parse
/// the line without running any of it, and a bash with `set -u` does not trip on an unset name.
const UNKNOWN_SHELL_HISTORY_PRELUDE: &str = " command -v shopt >/dev/null 2>&1 && eval '__hc0=${HISTCONTROL-}; HISTCONTROL=ignorespace:${HISTCONTROL-}'; command -v setopt >/dev/null 2>&1 && eval '__whi=$(setopt | command grep -c histignorespace); setopt hist_ignore_space'";
/// Tail of the probe line: puts back what [`UNKNOWN_SHELL_HISTORY_PRELUDE`] changed. The
/// known-shell stage that follows a successful probe turns the options on again for its own lines.
const UNKNOWN_SHELL_HISTORY_RESTORE: &str = "command -v shopt >/dev/null 2>&1 && eval 'HISTCONTROL=${__hc0-}; unset __hc0'; command -v setopt >/dev/null 2>&1 && eval '[ \"${__whi-}\" = 1 ] || unsetopt hist_ignore_space; unset __whi'";
/// Returns the raw init shell script for the given `shell_type`, without
/// single-quote escaping. Suitable for passing as an environment variable
/// where the caller controls the eval context (e.g. Docker sandbox init).
///
/// Gated on `unix` because the sole caller today is the Unix Docker
/// sandbox spawn path (`local_tty::unix::prepare_docker_sandbox`); on
/// Windows/wasm the function is dead code.
#[cfg(unix)]
pub fn raw_init_shell_script_for_shell(
    shell_type: ShellType,
    assets: &dyn AssetProvider,
    session_id: SessionId,
) -> String {
    let file = match shell_type {
        ShellType::Bash => "bundled/bootstrap/bash_init_shell.sh",
        ShellType::Zsh => "bundled/bootstrap/zsh_init_shell.sh",
        ShellType::Fish => "bundled/bootstrap/fish_init_shell.sh",
        ShellType::PowerShell => "bundled/bootstrap/pwsh_init_shell.ps1",
    };
    load_script(file, assets)
        .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string()))
        .replace(SESSION_ID_PLACEHOLDER, &session_id.as_u64().to_string())
}

/// Returns the script in the file at `file_path` to be passed as a single-quoted argument in the
/// shell (e.g. as a single quoted argument to `eval`).
///
/// The script is transformed in two ways:
///   * Newlines are stripped and replaced with semi-colons
///   * Single quotes are escaped (' is replaced with '"'"')
///   * Lines starting with '#' are removed -- this enables use of comments in scripts. Note,
///   however, that you still cannot use a 'partial line' comment, since this logic only considers
///   whole lines.
fn load_and_escape_script(file_path: &str, assets: &dyn AssetProvider) -> String {
    load_script(file_path, assets)
        .replace('\'', r#"'"'"'"#)
        .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string()))
}

/// Like [`load_and_escape_script`], for a script that fish will receive as a single-quoted
/// string. Unlike POSIX shells, fish processes `\\` and `\'` escapes inside single quotes, so a
/// literal backslash must be doubled first (#898: `'\\'` in the asset reached fish as the
/// unterminated `'\'`, and no fish pane ever started).
fn load_and_escape_fish_script(file_path: &str, assets: &dyn AssetProvider) -> String {
    load_script(file_path, assets)
        .replace('\\', r"\\")
        .replace('\'', r#"'"'"'"#)
        .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string()))
}

fn load_script(file_path: &str, assets: &dyn AssetProvider) -> String {
    let script_bytes = assets
        .get(file_path)
        .unwrap_or_else(|_| panic!("Failed to retrieve {file_path} from assets"));

    std::str::from_utf8(&script_bytes)
        .expect("InitShell script should be utf8 encoded.")
        .trim_start_matches(BYTE_ORDER_MARK)
        .lines()
        .filter(|line| !line.trim_start().starts_with('#') && !line.trim().is_empty())
        .join(";")
}

#[cfg(test)]
#[path = "bootstrap_test.rs"]
mod tests;
