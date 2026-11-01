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
    model::session::{BootstrapSessionType, IsLegacySSHSession, SessionInfo},
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
                // nushell cannot `eval` text, so its bootstrap is always a file it `source`s.
                || shell_type == ShellType::Nu
                || is_poetry_subshell
                || ((is_pipenv_subshell
                    || (subshell_initialization_info.is_some() && cfg!(windows)))
                    && shell_type == ShellType::Zsh)
                || is_msys2
        }
        BootstrapSessionType::WarpifiedRemote => false,
    }
}

/// The line of the bash bootstrap (`bash.sh`) that opens the here-document holding `bash_body.sh`.
const BASH_BOOTSTRAP_HEREDOC_OPEN: &[u8] =
    b" builtin read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :\n";
/// The end of that here-document: the newline of the body's last line, then the terminator line.
const BASH_BOOTSTRAP_HEREDOC_CLOSE: &[u8] = b"\nEOM\n";

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// #1172: rewrites the bash bootstrap so that its large body (`bash_body.sh`, about 45 of the
/// 52 KB) is read by one `head -c <bytes>` instead of being typed, line by line, into the remote
/// shell's line editor as a here-document. The remote spends ~200 ms reading the here-document
/// (1,200 prompts and redraws) and ~120 ms running it; the counted read brings the first to
/// ~15 ms (measured over ssh, 8 runs each: 330 -> 142 ms from InitShell to the Bootstrapped
/// hook). `stty -echo` keeps the tty from echoing the 45 KB back. Returns `None` when the
/// script does not have the expected here-document, so the caller keeps the original.
///
/// Only valid for a shell that is known to have a `head` with `-c`, because a failing `head`
/// would leave the body to be executed as commands: see `SSHValue::raw_bootstrap`.
pub fn bash_counted_read_bootstrap(script: &[u8]) -> Option<Vec<u8>> {
    counted_read_rewrite(script, ShellType::Bash).map(|(rewritten, _)| rewritten)
}

/// The line of the zsh bootstrap (`zsh.sh`) that opens the here-document holding `zsh_body.sh`.
/// Unlike bash, the `eval` of the body and the restore of the options the script changed are on
/// the same line, after the `read`.
const ZSH_BOOTSTRAP_HEREDOC_OPEN: &[u8] = b" read -r -d '' WARP_BOOTSTRAP_VAR << 'EOM' || :; ";

/// #1226: [`bash_counted_read_bootstrap`] for either shell, also returning how many leading bytes
/// of the result end with the counted-read command line (its CR): everything after that is the
/// raw body (and, for bash, the lines that follow it), which the shell does not see until `head`
/// reads it.
///
/// zsh: the command is `stty -echo; WARP_BOOTSTRAP_VAR=$(head -c N); stty echo; <the rest of the
/// original line>`, so the `eval` still runs after the body has been read. The hook sequence and
/// the `Bootstrapped` value are the same as the here-document's (replayed through a pty).
pub(crate) fn counted_read_rewrite(
    script: &[u8],
    shell_type: ShellType,
) -> Option<(Vec<u8>, usize)> {
    let (open_marker, rest_of_line) = match shell_type {
        ShellType::Bash => (BASH_BOOTSTRAP_HEREDOC_OPEN, false),
        ShellType::Zsh => (ZSH_BOOTSTRAP_HEREDOC_OPEN, true),
        _ => return None,
    };
    let open = find_subslice(script, open_marker)?;
    let mut body_start = open + open_marker.len();
    // zsh: what follows the here-document marker on the same line is a command that has to run
    // after the body has been read.
    let after_read: &[u8] = if rest_of_line {
        let line_end = body_start + script[body_start..].iter().position(|b| *b == b'\n')?;
        let rest = &script[body_start..line_end];
        body_start = line_end + 1;
        rest
    } else {
        &[]
    };
    let close = body_start + find_subslice(&script[body_start..], BASH_BOOTSTRAP_HEREDOC_CLOSE)?;
    // The body keeps its final newline, exactly as `read -d ''` read it.
    let body = &script[body_start..=close];
    let mut command = format!(
        " command -p stty -echo; WARP_BOOTSTRAP_VAR=$(LC_ALL=C command -p head -c {}); command -p stty echo",
        body.len()
    )
    .into_bytes();
    if rest_of_line {
        command.extend_from_slice(b"; ");
        command.extend_from_slice(after_read);
    }
    command.push(b'\r');
    let mut rewritten = Vec::with_capacity(script.len() + command.len());
    rewritten.extend_from_slice(&script[..open]);
    rewritten.extend_from_slice(&command);
    let prefix_len = rewritten.len();
    rewritten.extend_from_slice(body);
    rewritten.extend_from_slice(&script[close + BASH_BOOTSTRAP_HEREDOC_CLOSE.len()..]);
    Some((rewritten, prefix_len))
}

/// Whether the counted-read bootstrap is allowed. On by default;
/// `PHOSPHOR_SSH_COUNTED_BOOTSTRAP=0` sends the here-document form again.
fn counted_read_bootstrap_enabled() -> bool {
    counted_read_bootstrap_enabled_from_env(
        std::env::var("PHOSPHOR_SSH_COUNTED_BOOTSTRAP")
            .ok()
            .as_deref(),
    )
}

pub(crate) fn counted_read_bootstrap_enabled_from_env(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

/// The bootstrap to write into the shell: the counted-read form of the bash script
/// ([`bash_counted_read_bootstrap`]) for an ssh session whose remote reported `raw_bootstrap`,
/// otherwise `script` unchanged.
pub fn bootstrap_for_session(
    script: Cow<'static, [u8]>,
    shell_type: ShellType,
    session_info: &SessionInfo,
) -> Cow<'static, [u8]> {
    if shell_type != ShellType::Bash
        || !matches!(
            session_info.is_legacy_ssh_session,
            IsLegacySSHSession::Yes {
                raw_bootstrap: true,
                ..
            }
        )
        || !counted_read_bootstrap_enabled()
    {
        return script;
    }
    match bash_counted_read_bootstrap(&script) {
        Some(rewritten) => Cow::Owned(rewritten),
        None => script,
    }
}

/// Whether a local bash or zsh may be handed the bootstrap as a counted raw read (#1226). On by
/// default; `PHOSPHOR_LOCAL_COUNTED_BOOTSTRAP=0` types the here-document into the shell again.
pub(crate) fn local_counted_bootstrap_enabled_from_env(value: Option<&str>) -> bool {
    !matches!(value.map(str::trim), Some("0" | "false" | "off" | "no"))
}

/// Whether this machine has the two programs the counted read runs through `command -p` (the
/// system default PATH): `stty` and a `head` that takes `-c`. Both are POSIX tools that every
/// Linux and macOS install has; the check is on the files so that a stripped-down system keeps the
/// here-document, since a failing `head` would leave the body to run as commands.
#[cfg(unix)]
fn system_has_stty_and_head() -> bool {
    static HAS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS.get_or_init(|| {
        let found = |name: &str| {
            ["/usr/bin", "/bin"]
                .iter()
                .any(|dir| std::path::Path::new(dir).join(name).is_file())
        };
        found("stty") && found("head")
    })
}

#[cfg(not(unix))]
fn system_has_stty_and_head() -> bool {
    false
}

/// The commands whose subshells run on this host's own filesystem and tty, and that are known to
/// start the shell through a pty of their own (`sudo -s`, `su`, `ksu`, `doas`).
const LOCAL_COUNTED_SUBSHELL_COMMANDS: &[&str] = &["sudo", "su", "ksu", "doas"];

/// Whether `session_info` describes a session whose bootstrap can go in as a counted read (#1226):
/// a local bash or zsh on this machine, or a `sudo`/`su`/`ksu`/`doas` subshell of one, that is not
/// reached through ssh, a container, WSL or MSYS2.
#[cfg(feature = "local_fs")]
pub(crate) fn local_counted_bootstrap_applies(
    shell_type: ShellType,
    session_info: &SessionInfo,
) -> bool {
    use super::ShellLaunchData;

    if !matches!(shell_type, ShellType::Bash | ShellType::Zsh)
        || session_info.session_type != BootstrapSessionType::Local
        || session_info.is_legacy_ssh_session != IsLegacySSHSession::No
        || session_info.tmux_control_mode
        || session_info.wsl_name.is_some()
        || is_container_subshell(session_info)
        || !matches!(
            session_info.launch_data,
            None | Some(ShellLaunchData::Executable { .. })
        )
    {
        return false;
    }
    if let Some(subshell) = session_info.subshell_info.as_ref() {
        let command = super::warpify::subshell_display_name(&subshell.spawning_command)
            .and_then(|name| name.rsplit('/').next());
        if subshell.ssh_connection_info.is_some()
            || !command.is_some_and(|name| LOCAL_COUNTED_SUBSHELL_COMMANDS.contains(&name))
        {
            return false;
        }
    }
    system_has_stty_and_head()
        && local_counted_bootstrap_enabled_from_env(
            std::env::var("PHOSPHOR_LOCAL_COUNTED_BOOTSTRAP")
                .ok()
                .as_deref(),
        )
}

/// The counted-read form of `script` for a local session that can take it
/// ([`local_counted_bootstrap_applies`]), with the length of its leading part that ends with the
/// command line; `None` keeps the here-document.
#[cfg(feature = "local_fs")]
pub fn local_counted_bootstrap(
    script: &[u8],
    shell_type: ShellType,
    session_info: &SessionInfo,
) -> Option<(Vec<u8>, usize)> {
    if !local_counted_bootstrap_applies(shell_type, session_info) {
        return None;
    }
    counted_read_rewrite(script, shell_type)
}

/// The paced writes for a counted-read bootstrap typed into a subshell: what comes before the
/// raw body goes out in the same [`SUBSHELL_BOOTSTRAP_CHUNK_SIZE`] pieces as before, so a shell
/// that has not started reading yet is never handed more than its tty input queue holds (#960),
/// and the body follows as one write. Once `stty -echo` has run, the tty echoes nothing, so a
/// user-space pty relay (`sudo -i`) cannot be left blocked writing into a full inner tty while the
/// inner shell blocks writing an echo the relay is not reading; `head` drains the body at the
/// speed it arrives. (Through a blocking relay: the 52 KB unpaced here-document hangs, the
/// paced one takes 746 ms, this takes 222 ms.)
pub(crate) fn counted_subshell_bootstrap_chunks(script: &[u8], prefix_len: usize) -> Vec<Vec<u8>> {
    let prefix_len = prefix_len.min(script.len());
    let mut chunks = subshell_bootstrap_chunks(&script[..prefix_len]);
    if prefix_len < script.len() {
        chunks.push(script[prefix_len..].to_vec());
    }
    chunks
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
        ShellType::Nu => "nu.nu",
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
        // One statement per line, joined with `;`, passed as the argument of `nu --execute`.
        ShellType::Nu => load_script("bundled/bootstrap/nu_init_shell.nu", assets)
            .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string())),
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

/// Size of one write when the full bootstrap script is typed into a subshell (#960). Well under
/// the 4095 bytes a tty input queue holds, so a user-space pty relay such as `sudo -i` is never
/// asked to push more than the inner shell can accept while it is itself not reading the shell's
/// echo.
pub(crate) const SUBSHELL_BOOTSTRAP_CHUNK_SIZE: usize = 2048;

/// Whether the first bootstrap line is preceded by `^K ^U` to clear whatever the shell's line
/// editor holds. Not for PowerShell (#1047): PSReadLine has no `^K`/`^U` binding in its default
/// Windows edit mode, so it takes them as typed characters and lays out the line with control
/// glyphs whose width its cursor arithmetic does not agree with. On Windows PowerShell 5.1
/// (PSReadLine 2.0) that surfaced as `SetCursorPosition ... Actual value was -2` and a dead line
/// editor, on one launch in two.
pub(crate) fn subshell_bootstrap_clears_line_editor(shell_type: Option<ShellType>) -> bool {
    shell_type != Some(ShellType::PowerShell)
}

/// Pause between consecutive typed bootstrap lines. PSReadLine redraws on every line it reads and
/// the first PowerShell line runs PSReadLine cmdlets (cold on Windows PowerShell 5.1); input that
/// arrives while it is mid-redraw is what its cursor bookkeeping cannot survive (#1047), so
/// PowerShell gets a much longer gap than the tty-queue-driven 30 ms the other shells use.
pub(crate) fn subshell_bootstrap_line_delay(shell_type: Option<ShellType>) -> std::time::Duration {
    if shell_type == Some(ShellType::PowerShell) {
        std::time::Duration::from_millis(300)
    } else {
        std::time::Duration::from_millis(30)
    }
}

/// Splits `script` into the paced writes of [`SUBSHELL_BOOTSTRAP_CHUNK_SIZE`] bytes each.
pub(crate) fn subshell_bootstrap_chunks(script: &[u8]) -> Vec<Vec<u8>> {
    script
        .chunks(SUBSHELL_BOOTSTRAP_CHUNK_SIZE)
        .map(<[u8]>::to_vec)
        .collect()
}

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
    init_subshell_command_lines_with_markers(shell_type, vars, session_id, false, ctx)
}

/// [`init_subshell_command_lines`], optionally with a setup-line marker after every line but the
/// last (bash, zsh, fish, nushell and PowerShell; other shells ignore `marked`). The marker is an OSC the shell
/// prints once it has *executed* that line (`ESC ] 7771 ; <session id> ; <line index> BEL`, see
/// [`setup_line_marker`]), so the app can write line N+1 when line N is known to be consumed
/// instead of after a fixed delay.
pub fn init_subshell_command_lines_with_markers(
    shell_type: Option<ShellType>,
    vars: &[EnvVar],
    session_id: SessionId,
    marked: bool,
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
            let (history_line, policy_line) = if marked {
                powershell_marked_setup_lines(session_id.as_u64())
            } else {
                (
                    POWERSHELL_HISTORY_HANDLER_LINE.to_owned(),
                    POWERSHELL_EXECUTION_POLICY_LINE.to_owned(),
                )
            };
            vec![
                history_line,
                policy_line,
                format!(" if (-not $global:WARP_BOOTSTRAPPED) {{ {subshell_script} }}"),
            ]
        }
        Some(shell_type @ (ShellType::Bash | ShellType::Zsh | ShellType::Fish)) => {
            let raw_script =
                init_subshell_raw_script(shell_type, &crate::ASSETS, vars, session_id, ctx);
            chunked_eval_lines_with_markers(
                shell_type,
                &raw_script,
                marked.then(|| session_id.as_u64()),
            )
        }
        Some(ShellType::Nu) => {
            let raw_script =
                init_subshell_raw_script(ShellType::Nu, &crate::ASSETS, vars, session_id, ctx);
            nu_subshell_lines_with_markers(&raw_script, session_id, marked)
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
const POWERSHELL_HISTORY_HANDLER_LINE: &str = r#" if (-not $global:WARP_BOOTSTRAPPED) { $global:_warpPrevHistoryHandler = (PSReadLine\Get-PSReadLineOption).AddToHistoryHandler; PSReadLine\Set-PSReadLineOption -AddToHistoryHandler { param([string]$l) $t = $l.TrimStart(); if ($t.StartsWith('if (-not $global:WARP_BOOTSTRAPPED) {', [System.StringComparison]::Ordinal) -or $t -match '^\.\s+[^\r\n]*[\\/](\.tmp[A-Za-z0-9]{6}\.ps1|Phosphor[\\/]pwsh\.ps1)["'']?\s*$') { return $false }; if ($global:_warpPrevHistoryHandler) { return (& $global:_warpPrevHistoryHandler $l) }; return $true } } # apikey"#;

/// Second line of the PowerShell subshell bootstrap (#1116): lifts a Restricted execution policy
/// for this process, as `pwsh_init_shell.ps1` does for the app-started shell, so the ` . <tmp>.ps1`
/// line that sources the bootstrap file is not refused on a stock Windows install. `pwsh.ps1`
/// restores the policy from `$global:_warp_PSProcessExecPolicy`. A separate short line because the
/// subshell script is typed as one line and must stay under
/// [`MAX_SUBSHELL_BOOTSTRAP_LINE_BYTES`]; it starts with the guard the history handler filters.
const POWERSHELL_EXECUTION_POLICY_LINE: &str = r#" if (-not $global:WARP_BOOTSTRAPPED) { if ($PSEdition -eq 'Desktop' -or $IsWindows) { $EP = [Microsoft.PowerShell.ExecutionPolicy]; $m = Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope MachinePolicy; $u = Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope UserPolicy; if ($m -eq $EP::Restricted -or $u -eq $EP::Restricted) { Microsoft.PowerShell.Utility\Write-Error 'ExecutionPolicy is Restricted. Unable to Phosphorize this session.' } elseif ((Microsoft.PowerShell.Security\Get-ExecutionPolicy) -eq $EP::Restricted -and $m -eq $EP::Undefined -and $u -eq $EP::Undefined) { $global:_warp_PSProcessExecPolicy = Microsoft.PowerShell.Security\Get-ExecutionPolicy -Scope Process; Microsoft.PowerShell.Security\Set-ExecutionPolicy -Scope Process -ExecutionPolicy RemoteSigned -Force }} }"#;

/// The marker index PowerShell prints, instead of a line's own, at a `PSConsoleHostReadLine` entry
/// that no setup line is waiting on: "PSReadLine is about to read". The app ends the wait behind a
/// Ctrl-C that skipped the setup on it (#1372).
pub const POWERSHELL_READY_MARKER_INDEX: usize = 1_000_000;

/// The text that makes PowerShell print setup-line markers *after* PSReadLine is ready (#1372).
///
/// A marker printed at the end of a line (what bash/zsh/fish do) comes before PSReadLine redraws
/// its prompt, and a line written into that window is what crashed PSReadLine (#1047). So the line
/// only records its index in `$global:_phMk`, and a wrapper around `PSConsoleHostReadLine` -- the
/// function the host calls to read each line, after the prompt is written, the same hook
/// `pwsh.ps1` wraps for preexec -- prints it when the *next* read starts. A read that follows no
/// pending line prints [`POWERSHELL_READY_MARKER_INDEX`] instead (the read after a Ctrl-C). The
/// wrapper is guarded so a second bootstrap does not wrap itself; `pwsh.ps1` clears `$_phMk` at the
/// first command, which turns it into a no-op. Printed with `[Console]::Out`, as `pwsh.ps1` does.
fn powershell_marker_install(token: u64) -> String {
    format!(
        "if (-not $global:_phOldRL) {{ $global:_phOldRL = $function:global:PSConsoleHostReadLine; $function:global:PSConsoleHostReadLine = {{ if ($null -ne $global:_phMk) {{ [Console]::Out.Write(\"$([char]27)]{SETUP_LINE_MARKER_OSC};{token};$($global:_phMk)$([char]7)\"); $global:_phMk = {POWERSHELL_READY_MARKER_INDEX} }}; & $global:_phOldRL }} }}; $global:_phMk = 0"
    )
}

/// The first two PowerShell setup lines with their markers (#1372): the marker installer goes
/// before the history line's trailing comment word, and the second line records that it ran.
fn powershell_marked_setup_lines(token: u64) -> (String, String) {
    const COMMENT: &str = " # apikey";
    let base = POWERSHELL_HISTORY_HANDLER_LINE
        .strip_suffix(COMMENT)
        .unwrap_or(POWERSHELL_HISTORY_HANDLER_LINE);
    (
        format!("{base}; {}{COMMENT}", powershell_marker_install(token)),
        format!("{POWERSHELL_EXECUTION_POLICY_LINE}; $global:_phMk = 1"),
    )
}

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

/// The OSC number of the setup-line marker (private; nothing else uses it).
pub const SETUP_LINE_MARKER_OSC: &str = "7771";

/// What a bash / zsh / fish prints after executing setup line `index` of the session `token`.
/// `printf` is quoted so the text typed into the shell holds no escape byte.
fn setup_line_marker(token: u64, index: usize) -> String {
    format!(" builtin printf '\\e]{SETUP_LINE_MARKER_OSC};{token};{index}\\a'")
}

/// Whether the old timer-paced bootstrap (one fixed delay per typed line, a fixed wait before the
/// pending command / rc-file / auto-warpify bootstrap) is wanted instead of the event-driven one:
/// `PHOSPHOR_BOOTSTRAP_TIMED=1`. A kill switch for the events-not-timers rework (#1354).
pub fn bootstrap_timed() -> bool {
    static TIMED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *TIMED.get_or_init(|| std::env::var("PHOSPHOR_BOOTSTRAP_TIMED").is_ok_and(|v| v == "1"))
}

/// Splits `raw_script` into lines that rebuild it in `$_wbs` and then evaluate it.
fn chunked_eval_lines(shell_type: ShellType, raw_script: &str) -> Vec<String> {
    chunked_eval_lines_with_markers(shell_type, raw_script, None)
}

/// [`chunked_eval_lines`]; with `marker_token` every line but the last ends in a
/// [`setup_line_marker`] (the last one evaluates the script, which ends the typed setup).
fn chunked_eval_lines_with_markers(
    shell_type: ShellType,
    raw_script: &str,
    marker_token: Option<u64>,
) -> Vec<String> {
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
            // Also save and switch off the user's errexit / pipefail / nounset (#1014): the typed
            // lines and the stage they evaluate must not meet them. The last line puts them back, except
            // print_exit_value: it stays off until zsh.sh's tail, because the next typed line (`read` at
            // the end of a heredoc fails by design) would print `zsh: exit 1` under it.
            " [ -n \"${__whi-}\" ] || { __whi=0; [[ -o hist_ignore_space ]] && __whi=1; }; setopt hist_ignore_space; [ -n \"${__wpe-}\" ] || { __wpe=0; [[ -o print_exit_value ]] && __wpe=1; }; unsetopt print_exit_value; [ -n \"${__wee-}\" ] || { __wee=0; [[ -o err_exit ]] && __wee=1; }; unsetopt err_exit err_return; [ -n \"${__wpf-}\" ] || { __wpf=0; [[ -o pipe_fail ]] && __wpf=1; }; unsetopt pipe_fail; [ -n \"${__wnu-}\" ] || { __wnu=0; [[ -o unset ]] || __wnu=1; }; setopt unset"
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
            " HISTCONTROL=${{__hc0-}}; unset __hc0; if [ -z \"${{WARP_BOOTSTRAPPED-}}\" ]; then if [ \"${{#_wbs}}\" -eq {chars} ] || [ \"${{#_wbs}}\" -eq {bytes} ]; then eval \"$_wbs\"; else {SKIPPED_NOTICE}; fi; fi; unset _wbs"
        ),
        ShellType::Zsh => format!(
            " if [ \"${{__whi-}}\" = 1 ]; then WARP_HIST_IGNORE_SPACE_USER=1; else unsetopt hist_ignore_space; fi; unset __whi; if [ -z \"${{WARP_BOOTSTRAPPED-}}\" ]; then if [ \"${{#_wbs}}\" -eq {chars} ] || [ \"${{#_wbs}}\" -eq {bytes} ]; then eval \"$_wbs\"; else {SKIPPED_NOTICE}; fi; fi; if [ \"${{__wpf-}}\" = 1 ]; then setopt pipe_fail; fi; if [ \"${{__wnu-}}\" = 1 ]; then unsetopt unset; fi; if [ \"${{__wee-}}\" = 1 ]; then setopt err_exit; fi; [ \"${{__wpe-}}\" != 1 ] || WARP_RESTORE_PRINT_EXIT_VALUE=1; unset __wpe __wpf __wnu __wee _wbs WARP_HIST_IGNORE_SPACE_USER"
        ),
        // fish cannot parse `${...}` and has no nounset, so it keeps the bare expansion.
        _ => format!(
            " [ -z $WARP_BOOTSTRAPPED ] && contains -- (string length -- $_wbs) {chars} {bytes} && eval $_wbs; set -e _wbs"
        ),
    });
    if let Some(token) = marker_token {
        let last = lines.len().saturating_sub(1);
        for (index, line) in lines.iter_mut().enumerate().take(last) {
            line.push(';');
            line.push_str(&setup_line_marker(token, index));
        }
    }
    lines
}

/// The notice the nushell stage-one file prints when the script arrived with the wrong length.
const NU_SKIPPED_NOTICE: &str = "print $\"(char nl)Phosphor: subshell setup skipped, the script arrived incomplete. Type exit to leave this shell.(char nl)\"";

/// Marks a newline / carriage return inside a nushell chunk, which is a single typed line.
const NU_NEWLINE_TOKEN: &str = "@@NL@@";
const NU_CARRIAGE_RETURN_TOKEN: &str = "@@CR@@";

/// What a nushell prints after executing setup line `index` of the session `token`: the same OSC
/// as [`setup_line_marker`], built from `ansi esc` / `char bel` inside a string interpolation so
/// the typed text holds no escape byte. `print -n` writes no newline. It follows a `;`, so the
/// line stays one statement sequence (#1372).
///
/// The escape is `ansi esc`, NOT `char esc`: nushell 0.116 has no `char esc` ("error finding
/// named character"), which fails the whole line at parse time, so no marker ever came and the
/// setup fell back to timers on every run (#1399). Verified against nu 0.116.1: `ansi esc` and
/// `char bel` print `ESC ] 7771;<token>;<index> BEL`.
fn nu_setup_line_marker(token: u64, index: usize) -> String {
    format!(" print -n $\"(ansi esc)]{SETUP_LINE_MARKER_OSC};{token};{index}(char bel)\"")
}

/// The lines typed into a running `nu` to run the stage-one subshell script `raw_script`.
///
/// Nushell cannot evaluate a string, only `source` a file whose path is known at parse time, so
/// the script is rebuilt in `$env._wbs` over short lines (each a complete statement, as a
/// canonical tty line is limited, #962), written by one line to a file in the user's own cache
/// directory, and sourced by the next (the file must exist when that line is parsed). Every line
/// starts with a space: `history.ignore_space_prefixed` (nushell's default) keeps it out of the
/// history, and the first line switches that option on, saving the prior value in `$env.__wbh`
/// (`nu.nu` puts it back once it has been sourced, so the line that sources it is hidden too).
/// The only line that can ever be recorded is that short first one (#967). Nothing deletes a
/// history entry.
///
/// The script is only run when it arrived whole (its byte length matches) and the shell is not
/// already bootstrapped; otherwise the file holds a notice, or nothing.
///
/// With `marked`, every line but the last ends in a [`nu_setup_line_marker`] (the last removes the
/// temp file and the script variable; nothing after it needs pacing).
///
/// The unknown-shell probe (`shell_type == None`) cannot be marked: its two lines are written
/// before the shell is identified, and a marker needs the shell's own syntax (`builtin printf`
/// vs `print -n` vs PowerShell's `Write-Host`); the probe line is the one that identifies it.
fn nu_subshell_lines_with_markers(
    raw_script: &str,
    session_id: SessionId,
    marked: bool,
) -> Vec<String> {
    let script = raw_script
        .replace('\r', NU_CARRIAGE_RETURN_TOKEN)
        .replace('\n', NU_NEWLINE_TOKEN);
    // A raw string `r#'..'#` ends at the first `'#`; use more `#`s than any run in the script.
    let longest_hash_run = script
        .split(|c| c != '#')
        .map(str::len)
        .max()
        .unwrap_or_default();
    let hashes = "#".repeat(longest_hash_run + 1);

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in script.chars() {
        if current.len() + ch.len_utf8() > SUBSHELL_CHUNK_PAYLOAD_BYTES {
            chunks.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    let file = format!("phosphor-nu-{}.nu", session_id.as_u64());
    let mut lines = Vec::with_capacity(chunks.len() + 4);
    lines.push(
        " $env.__wbh = $env.config.history.ignore_space_prefixed; $env.config.history.ignore_space_prefixed = true"
            .to_owned(),
    );
    for (index, chunk) in chunks.iter().enumerate() {
        let literal = format!("r{hashes}'{chunk}'{hashes}");
        lines.push(if index == 0 {
            format!(" $env._wbs = {literal}")
        } else {
            format!(" $env._wbs = ($env._wbs + {literal})")
        });
    }
    let length = script.len();
    lines.push(format!(
        " const _wbf = ($nu.cache-dir | path join '{file}'); mkdir $nu.cache-dir; (if ($env.WARP_BOOTSTRAPPED? | default '' | is-not-empty) {{ '' }} else if (($env._wbs? | default '' | str length) == {length}) {{ $env._wbs | str replace -a '{NU_NEWLINE_TOKEN}' (char nl) | str replace -a '{NU_CARRIAGE_RETURN_TOKEN}' (char cr) }} else {{ {NU_SKIPPED_NOTICE} }}) | save -f $_wbf"
    ));
    lines.push(" source $_wbf".to_owned());
    lines.push(" rm -f $_wbf; hide-env -i _wbs".to_owned());
    if marked {
        let last = lines.len().saturating_sub(1);
        for (index, line) in lines.iter_mut().enumerate().take(last) {
            line.push(';');
            line.push_str(&nu_setup_line_marker(session_id.as_u64(), index));
        }
    }
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
        ShellType::Nu => {
            format!("$env.WARP_HONOR_PS1 = '{honor_ps1_env_var_value}'; {env_vars_init}")
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
        ShellType::Nu => load_script("bundled/bootstrap/nu_init_subshell.nu", assets)
            .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string())),
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
        ShellType::Nu => "bundled/bootstrap/nu_init_subshell.nu",
    };
    let script = load_script(file, assets)
        .replace("@@USING_CON_PTY_BOOLEAN@@", &(cfg!(windows).to_string()))
        .replace(SESSION_ID_PLACEHOLDER, &session_id.as_u64().to_string());
    if shell_type == ShellType::Nu {
        return format!(
            "$env.WARP_HONOR_PS1 = '{honor_ps1_env_var_value}'; {env_vars_init} {script}"
        );
    }
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
/// in bash (`HISTCONTROL`) and zsh (`hist_ignore_space`) so every later bootstrap line (the long
/// probe line and the whole known-shell stage) is not recorded. A shell records a line as it reads
/// it, before running it, so THIS line is the one line that can still land in history when the
/// option was off. That cannot be avoided from inside the typed stream: nothing the shell runs can
/// change how the line that runs it was recorded, and the only way to take it out afterwards
/// (`history -d`, `fc -p` tricks) risks the user's own entries, which Phosphor never touches. The
/// prior state goes in `__hc0` / `__whi`, which the known-shell stage reuses and puts back (see
/// [`UNKNOWN_SHELL_HISTORY_RESTORE`]). Every `${}` sits inside a single-quoted `eval` that only
/// bash (`shopt` exists) or zsh (`setopt` exists) reaches, so fish and POSIX sh parse the line
/// without running any of it, and a bash with `set -u` does not trip on an unset name.
const UNKNOWN_SHELL_HISTORY_PRELUDE: &str = " command -v shopt >/dev/null 2>&1 && eval '__hc0=${HISTCONTROL-}; HISTCONTROL=ignorespace:${HISTCONTROL-}'; command -v setopt >/dev/null 2>&1 && eval '__whi=0; [[ -o hist_ignore_space ]] && __whi=1; __wpe=0; [[ -o print_exit_value ]] && __wpe=1; setopt hist_ignore_space; unsetopt print_exit_value'; true";
/// Tail of the probe line: puts back what [`UNKNOWN_SHELL_HISTORY_PRELUDE`] changed, except when
/// the probe is about to report `bash` or `zsh` (the same `$0` test the probe uses). Those are
/// followed by the known-shell stage, whose first line was recorded under the old behaviour
/// because this tail had already switched the option off; now the option stays on until that
/// stage's last line restores it (it reuses `__hc0` / `__whi`). A bash or zsh the probe cannot
/// name (for example bash started as `sh`) gets no known-shell stage, so it is restored here.
const UNKNOWN_SHELL_HISTORY_RESTORE: &str = "command -v shopt >/dev/null 2>&1 && eval 'case $0 in *bash*|*zsh*) ;; *) HISTCONTROL=${__hc0-}; unset __hc0;; esac'; command -v setopt >/dev/null 2>&1 && eval 'case $0 in *bash*|*zsh*) ;; *) [ \"${__whi-}\" = 1 ] || unsetopt hist_ignore_space; unset __whi; [ \"${__wpe-}\" != 1 ] || setopt print_exit_value; unset __wpe;; esac'; true";
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
        ShellType::Nu => "bundled/bootstrap/nu_init_shell.nu",
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

#[cfg(test)]
#[path = "bootstrap_snapshot_tests.rs"]
mod snapshot_tests;
