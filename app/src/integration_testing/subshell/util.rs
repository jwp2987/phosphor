//! The ssh server the ssh / remote-subshell integration scenarios connect to.
//!
//! Before #1506 every scenario drove a real `ssh` into Warp's private GCP instance through an
//! Identity-Aware Proxy (`gcloud compute start-iap-tunnel`). This fork has no credentials for
//! that project, so the scenarios skipped everywhere and no ssh behaviour was ever exercised.
//!
//! They now connect to a LOCAL OpenSSH `sshd` that the scenario starts itself on 127.0.0.1 and
//! stops in its cleanup, the same way the real-app harness does (`script/realapp/appci/sshd.py`):
//!
//! * runs as the current user, no privileges (`UsePAM no`, `StrictModes no`, public-key auth only);
//! * one sshd per remote shell, because an unprivileged sshd can only log in as the user running
//!   it. A `ForceCommand` script behaves like a real sshd for a user whose login shell is
//!   `<shell>`: it sets `HOME` (a scratch "remote home" with empty rc files, so the real home is
//!   never touched) and `SHELL`, runs a command as `$SHELL -c "$SSH_ORIGINAL_COMMAND"`, and
//!   otherwise prints a MotD and starts `$SHELL -l -i`;
//! * the remote shell has `PHOSPHOR_TEST_SSHD=remote` and `WARP_PROFILE_LOADED=true` exported, so
//!   a scenario can tell remote from local and "login shell" from "not";
//! * two client keys: `clientkey` (no passphrase, so the connect never prompts and keys typed
//!   during it are real typeahead) and `pwkey` (passphrase [`SSH_KEY_PASSPHRASE`]), the only
//!   password-style prompt an unprivileged sshd can give (`Enter passphrase for key ...`).
//!
//! If `sshd`, `ssh-keygen` or the requested shell is missing the scenario is SKIPPED and says
//! so (`INTEG-SKIP <scenario> NEEDS: ...`, #2122: counted as a skip, never as a pass) -- unless `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` (set by the `integration-linux` CI
//! job), where a missing fixture is a hard failure so it can never pass by not running.

use std::path::PathBuf;

use command::blocking::Command;

/// Passphrase of the throw-away `pwkey` client key (it protects nothing).
pub const SSH_KEY_PASSPHRASE: &str = "ppCI-7431-test";

/// Where to find a running fixture. Everything the typed `ssh` command needs.
#[derive(Clone, Debug)]
pub struct SshFixture {
    pub port: u16,
    /// Client key without a passphrase.
    pub plain_identity: PathBuf,
    /// Client key protected by [`SSH_KEY_PASSPHRASE`].
    pub passphrase_identity: PathBuf,
}

fn run_trimmed(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!text.is_empty()).then_some(text)
}

/// The user the remote shell reports (`whoami`): the user running the test, because the fixture
/// sshd logs in as its own user.
pub fn remote_user() -> String {
    run_trimmed("whoami", &[])
        .or_else(|| std::env::var("USER").ok())
        .unwrap_or_default()
}

/// The host name the remote shell reports (`hostname`): this machine's.
pub fn remote_hostname() -> String {
    run_trimmed("hostname", &[])
        .or_else(|| run_trimmed("uname", &["-n"]))
        .unwrap_or_default()
}

#[cfg(unix)]
mod local_sshd {
    use super::{SSH_KEY_PASSPHRASE, SshFixture};
    use command::blocking::Command;
    use std::fs::File;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    use std::path::{Path, PathBuf};
    use std::process::{Child, Stdio};
    use std::sync::Mutex;
    use std::time::Duration;

    struct Running {
        fixture: SshFixture,
        child: Child,
        dir: PathBuf,
    }

    /// One entry per remote shell asked for in this process (the result is cached, errors too).
    static RUNNING: Mutex<Vec<(String, Result<Running, String>)>> = Mutex::new(Vec::new());

    fn is_executable(path: &Path) -> bool {
        path.metadata()
            .map(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    }

    fn find_program(name: &str, extra_dirs: &[&str]) -> Option<PathBuf> {
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|path| std::env::split_paths(&path).collect())
            .unwrap_or_default();
        dirs.extend(extra_dirs.iter().map(PathBuf::from));
        dirs.into_iter()
            .map(|dir| dir.join(name))
            .find(|candidate| is_executable(candidate))
    }

    fn sh_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    fn generate_key(keygen: &Path, file: &Path, passphrase: &str) -> Result<(), String> {
        let output = Command::new(keygen)
            .args(["-q", "-t", "ed25519", "-N", passphrase, "-f"])
            .arg(file)
            .output()
            .map_err(|err| format!("could not run ssh-keygen: {err}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "ssh-keygen failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
        }
    }

    fn free_port() -> Result<u16, String> {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .map_err(|err| format!("no free loopback port: {err}"))?;
        let port = listener
            .local_addr()
            .map_err(|err| format!("no free loopback port: {err}"))?
            .port();
        Ok(port)
    }

    /// The fixture's `ForceCommand` script: behaves like sshd for a user whose login shell is
    /// `shell_path` and whose home is `home`. The banner is printed for every session that has
    /// a terminal, a wrapped one (which carries a remote command) included, so a scenario has a
    /// MotD to look for on a machine with no MotD file of its own. A command without a
    /// terminal (the app's own multiplexed commands, scp) gets none: its output is read by a
    /// program.
    pub(super) fn force_command_script(home: &Path, shell_path: &Path) -> String {
        format!(
            "#!/bin/sh\n\
             HOME={home}\n\
             SHELL={shell_path}\n\
             PHOSPHOR_TEST_SSHD=remote\n\
             WARP_PROFILE_LOADED=true\n\
             export HOME SHELL PHOSPHOR_TEST_SSHD WARP_PROFILE_LOADED\n\
             cd \"$HOME\" || exit 1\n\
             if [ -z \"$SSH_ORIGINAL_COMMAND\" ] || [ -t 1 ]; then echo \"{motd}\"; fi\n\
             if [ -n \"$SSH_ORIGINAL_COMMAND\" ]; then exec \"$SHELL\" -c \"$SSH_ORIGINAL_COMMAND\"; fi\n\
             exec \"$SHELL\" -l -i\n",
            home = sh_quote(&home.to_string_lossy()),
            shell_path = sh_quote(&shell_path.to_string_lossy()),
            motd = super::FIXTURE_MOTD,
        )
    }

    fn start_fixture(shell: &str) -> Result<Running, String> {
        let sshd = find_program(
            "sshd",
            &[
                "/usr/sbin",
                "/usr/local/sbin",
                "/opt/homebrew/sbin",
                "/usr/libexec",
            ],
        )
        .ok_or_else(|| "sshd (the OpenSSH server) is not installed".to_owned())?;
        let keygen = find_program("ssh-keygen", &["/usr/bin"])
            .ok_or_else(|| "ssh-keygen is not installed".to_owned())?;
        let shell_path = find_program(
            shell,
            &["/bin", "/usr/bin", "/usr/local/bin", "/opt/homebrew/bin"],
        )
        .ok_or_else(|| format!("the remote shell `{shell}` is not installed"))?;

        let dir =
            std::env::temp_dir().join(format!("phosphor-sshfx-{}-{}", std::process::id(), shell));
        let _ = std::fs::remove_dir_all(&dir);
        let home = dir.join("home");
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&home)
            .map_err(|err| format!("could not create {}: {err}", home.display()))?;
        // Empty rc files: zsh's first-run wizard must not start, and nothing of the real
        // user's configuration is read.
        for rc in [".bashrc", ".zshrc", ".profile"] {
            std::fs::write(home.join(rc), "")
                .map_err(|err| format!("could not write {rc}: {err}"))?;
        }

        generate_key(&keygen, &dir.join("hostkey"), "")?;
        generate_key(&keygen, &dir.join("clientkey"), "")?;
        generate_key(&keygen, &dir.join("pwkey"), SSH_KEY_PASSPHRASE)?;
        let mut authorized = String::new();
        for public in ["clientkey.pub", "pwkey.pub"] {
            authorized.push_str(
                &std::fs::read_to_string(dir.join(public))
                    .map_err(|err| format!("could not read {public}: {err}"))?,
            );
        }
        let authorized_path = dir.join("authorized_keys");
        std::fs::write(&authorized_path, authorized)
            .map_err(|err| format!("could not write authorized_keys: {err}"))?;
        std::fs::set_permissions(&authorized_path, std::fs::Permissions::from_mode(0o600))
            .map_err(|err| format!("could not chmod authorized_keys: {err}"))?;

        let force = dir.join("force.sh");
        std::fs::write(&force, force_command_script(&home, &shell_path))
            .map_err(|err| format!("could not write force.sh: {err}"))?;
        std::fs::set_permissions(&force, std::fs::Permissions::from_mode(0o755))
            .map_err(|err| format!("could not chmod force.sh: {err}"))?;

        let port = free_port()?;
        let config = dir.join("sshd_config");
        std::fs::write(
            &config,
            format!(
                "Port {port}\nListenAddress 127.0.0.1\nHostKey {d}/hostkey\nPidFile {d}/sshd.pid\n\
                 AuthorizedKeysFile {d}/authorized_keys\nPasswordAuthentication no\n\
                 KbdInteractiveAuthentication no\nUsePAM no\nStrictModes no\nPubkeyAuthentication yes\n\
                 ForceCommand {d}/force.sh\nLogLevel ERROR\nPermitTTY yes\nX11Forwarding no\n\
                 PrintMotd no\nPrintLastLog no\n",
                d = dir.display(),
            ),
        )
        .map_err(|err| format!("could not write sshd_config: {err}"))?;

        let log = File::create(dir.join("sshd.log"))
            .map_err(|err| format!("could not create sshd.log: {err}"))?;
        let log_err = log
            .try_clone()
            .map_err(|err| format!("could not clone sshd.log: {err}"))?;
        let mut child = Command::new(&sshd)
            .arg("-D")
            .arg("-e")
            .arg("-f")
            .arg(&config)
            .stdin(Stdio::null())
            .stdout(log)
            .stderr(log_err)
            .spawn()
            .map_err(|err| format!("could not start {}: {err}", sshd.display()))?;

        let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        for _ in 0..100 {
            if std::net::TcpStream::connect_timeout(&address, Duration::from_millis(200)).is_ok() {
                return Ok(Running {
                    fixture: SshFixture {
                        port,
                        plain_identity: dir.join("clientkey"),
                        passphrase_identity: dir.join("pwkey"),
                    },
                    child,
                    dir,
                });
            }
            if let Ok(Some(status)) = child.try_wait() {
                let log_text = std::fs::read_to_string(dir.join("sshd.log")).unwrap_or_default();
                return Err(format!("sshd exited at startup ({status}): {log_text}"));
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err(format!(
            "sshd did not listen on 127.0.0.1:{port} (see {}/sshd.log)",
            dir.display()
        ))
    }

    /// Starts (once per process and shell) and returns the fixture whose remote login shell is
    /// `shell`, or why there is none.
    pub fn ssh_fixture(shell: &str) -> Result<SshFixture, String> {
        let mut running = RUNNING.lock().unwrap_or_else(|err| err.into_inner());
        if let Some((_, existing)) = running.iter().find(|(name, _)| name == shell) {
            return existing
                .as_ref()
                .map(|run| run.fixture.clone())
                .map_err(|err| err.clone());
        }
        let started = start_fixture(shell);
        let result = started
            .as_ref()
            .map(|run| run.fixture.clone())
            .map_err(|err| err.clone());
        running.push((shell.to_owned(), started));
        result
    }

    /// Stops every sshd this process started and removes its scratch directory.
    pub fn stop_ssh_fixtures() {
        let mut running = RUNNING.lock().unwrap_or_else(|err| err.into_inner());
        for (_, entry) in running.drain(..) {
            if let Ok(mut run) = entry {
                let _ = run.child.kill();
                let _ = run.child.wait();
                let _ = std::fs::remove_dir_all(&run.dir);
            }
        }
    }
}

#[cfg(unix)]
pub use local_sshd::{ssh_fixture, stop_ssh_fixtures};

/// There is no local sshd fixture on this platform; the scenarios skip.
#[cfg(not(unix))]
pub fn ssh_fixture(_shell: &str) -> Result<SshFixture, String> {
    Err("the local sshd fixture is only implemented for unix".to_owned())
}

#[cfg(not(unix))]
pub fn stop_ssh_fixtures() {}

/// Whether the ssh integration fixture for a remote login shell `shell` is available.
///
/// This starts the local sshd (see the module docs), so call it LAST in a scenario's
/// `set_should_run_test`, after the cheap checks, and register `stop_ssh_fixtures` as the
/// builder's cleanup. When it is not available the scenario is skipped and the reason is
/// recorded for the driver's `INTEG-SKIP ... NEEDS:` line (#2122). With `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` (the `integration-linux` CI job)
/// a missing or broken sshd panics instead (a missing remote shell still skips), so the
/// scenarios cannot silently stop running.
pub fn ssh_test_fixture_available(shell: &str) -> bool {
    match ssh_fixture(shell) {
        Ok(_) => true,
        Err(why) => {
            // A missing REMOTE SHELL is a legitimate skip on any machine (no fish, no ash); a
            // missing or broken sshd / ssh-keygen is not.
            let missing_shell = why.contains("the remote shell");
            if !missing_shell
                && std::env::var("PHOSPHOR_REQUIRE_SSH_FIXTURE").is_ok_and(|value| value == "1")
            {
                panic!(
                    "PHOSPHOR_REQUIRE_SSH_FIXTURE=1 but the ssh fixture for `{shell}` is unavailable: {why}"
                );
            }
            // #2122: the driver prints this as `INTEG-SKIP <scenario> NEEDS: ...` and the CI
            // summary counts it; the scenario is never reported as one that ran.
            warpui::integration::skip_because(format!(
                "the local sshd fixture with remote shell `{shell}`: {why}"
            ))
        }
    }
}

/// Whether a typed `ssh` runs the legacy (ControlMaster) ssh wrapper, which is what the
/// `test_legacy_ssh_*` scenarios and the two connect scenarios drive (#2128). True in every
/// build of this fork: see [`TMUX_SSH_WRAPPER_TAKES_SSH`]. Upstream's gate for these scenarios
/// was `!FeatureFlag::SSHTmuxWrapper.is_enabled()`, which is false in every default build here
/// (the flag is kept on for tmux control mode), so they skipped in every run.
///
/// [`TMUX_SSH_WRAPPER_TAKES_SSH`]: crate::terminal::ssh::ssh_detection::TMUX_SSH_WRAPPER_TAKES_SSH
pub fn legacy_ssh_wrapper_takes_ssh() -> bool {
    !crate::terminal::ssh::ssh_detection::TMUX_SSH_WRAPPER_TAKES_SSH
}

/// The message-of-the-day files the ssh wrapper's remote command and the bootstraps print (the
/// first readable one, unless `~/.hushlogin` exists), in their order (`bash_body.sh`).
const MOTD_FILES: [&str; 5] = [
    "/etc/motd",
    "/run/motd",
    "/run/motd.dynamic",
    "/usr/lib/motd",
    "/usr/lib/motd.dynamic",
];

/// The start of the first plain line of `motd` (at most 40 characters, so a wrapped line of a
/// narrow grid still holds it): what a scenario looks for to see that the MotD was printed.
/// `None` when no line is plain printable text (an empty file, colour codes only).
pub fn motd_marker(motd: &str) -> Option<String> {
    motd.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && line.chars().all(|ch| ch == ' ' || ch.is_ascii_graphic()))
        .map(|line| {
            line.chars()
                .take(40)
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
}

/// What a session through the ssh wrapper prints as the MotD on THIS machine. The fixture's
/// "remote" is the machine the scenario runs on: the wrapper's remote command (or the remote
/// bootstrap) prints the machine's MotD file, as sshd would have. `None` when the machine has
/// none; a scenario then looks for the fixture's own banner ([`FIXTURE_MOTD`]).
pub fn machine_motd_marker() -> Option<String> {
    let first_readable = MOTD_FILES
        .iter()
        .find_map(|path| std::fs::read(path).ok())?;
    motd_marker(&String::from_utf8_lossy(&first_readable))
}

/// The banner the fixture's `ForceCommand` script prints for every session with a terminal
/// (a wrapped session too) and for a session without a remote command. It is printed before
/// the remote command starts, so it lands in the output of the block that ran `ssh`, not in
/// the remote shell's bootstrap block where the machine's own MotD goes.
pub const FIXTURE_MOTD: &str = "Welcome to Ubuntu (phosphor local test sshd)";

/// The full ssh command to run to ssh into the fixture whose remote login shell is `shell`,
/// authenticating with the passphrase-protected key (so ssh stops at a passphrase prompt on the
/// tty).
pub fn ssh_command(shell: &str, should_use_ssh_wrapper: bool) -> String {
    ssh_command_with_key(shell, should_use_ssh_wrapper, true)
}

/// Like [`ssh_command`]; `with_passphrase` false uses the key without a passphrase, so the
/// connect never prompts.
pub fn ssh_command_with_key(
    shell: &str,
    should_use_ssh_wrapper: bool,
    with_passphrase: bool,
) -> String {
    let program = if should_use_ssh_wrapper {
        "ssh"
    } else {
        "command ssh"
    };
    match ssh_fixture(shell) {
        Ok(fixture) => {
            let identity = if with_passphrase {
                &fixture.passphrase_identity
            } else {
                &fixture.plain_identity
            };
            [
                program.to_owned(),
                format!("{}@127.0.0.1", remote_user()),
                format!("-p {}", fixture.port),
                format!("-i {}", identity.display()),
                "-o IdentitiesOnly=yes".to_owned(),
                "-o IdentityAgent=none".to_owned(),
                "-o PreferredAuthentications=publickey".to_owned(),
                "-o StrictHostKeyChecking=no".to_owned(),
                "-o UserKnownHostsFile=/dev/null".to_owned(),
                "-o LogLevel=ERROR".to_owned(),
            ]
            .join(" ")
        }
        // The scenario's `set_should_run_test` skips it before this is ever typed.
        Err(_) => format!("{program} phosphor-ssh-fixture-unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use super::{FIXTURE_MOTD, legacy_ssh_wrapper_takes_ssh, motd_marker};

    #[test]
    fn motd_marker_is_the_start_of_the_first_plain_line() {
        assert_eq!(
            motd_marker("\n\nWelcome to Ubuntu 24.04 LTS (GNU/Linux 6.8.0 x86_64)\n\n * Docs\n"),
            Some("Welcome to Ubuntu 24.04 LTS (GNU/Linux 6".to_owned())
        );
        // A line with colour codes cannot be found in the grid's text: the next plain one is.
        assert_eq!(
            motd_marker("\u{1b}[1mBANNER\u{1b}[0m\n  plain line  \n"),
            Some("plain line".to_owned())
        );
        assert_eq!(motd_marker(""), None);
        assert_eq!(motd_marker("\n \n\u{1b}[0m\n"), None);
        // The fixture's own banner is one plain line a marker can be made of.
        assert_eq!(
            motd_marker(FIXTURE_MOTD).as_deref(),
            Some("Welcome to Ubuntu (phosphor local test s")
        );
    }

    /// #2128: the gate of the legacy ssh scenarios is the fact that decides which wrapper a
    /// typed `ssh` runs, and in this fork that is the legacy one. If this fails the tmux
    /// option is back and the scenarios' gates need a look.
    #[test]
    fn a_typed_ssh_runs_the_legacy_wrapper_in_this_fork() {
        assert!(legacy_ssh_wrapper_takes_ssh());
    }

    /// Owner decision 2026-11-11: the fixture prints its banner for a wrapped session too (one
    /// with a remote command and a terminal), so the MotD assertion of the ssh scenarios is
    /// never vacuous. A remote command WITHOUT a terminal must get none: that is the app's own
    /// multiplexed command (the helper check, the no-helper executor, scp), whose output a
    /// program reads. Run here with the real `sh`, stdout a pipe.
    #[cfg(unix)]
    #[test]
    fn the_fixture_banner_is_not_printed_for_a_command_without_a_terminal() {
        let script = super::local_sshd::force_command_script(
            &std::env::temp_dir(),
            std::path::Path::new("/bin/sh"),
        );
        let banner_at = script
            .find(FIXTURE_MOTD)
            .expect("the script prints the banner");
        let command_at = script
            .find("exec \"$SHELL\" -c")
            .expect("the script runs a remote command");
        assert!(
            banner_at < command_at,
            "the banner is decided before a remote command replaces the script: {script}"
        );
        let output = command::blocking::Command::new("/bin/sh")
            .arg("-c")
            .arg(format!(
                "SSH_ORIGINAL_COMMAND='echo ran'; export SSH_ORIGINAL_COMMAND\n{script}"
            ))
            .stdin(std::process::Stdio::null())
            .output()
            .expect("sh runs the script");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "ran\n");
    }
}
