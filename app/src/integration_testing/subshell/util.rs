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
//! If `sshd`, `ssh-keygen` or the requested shell is missing the scenario is SKIPPED with the
//! reason on stderr -- unless `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` (set by the `integration-linux` CI
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

        // Behave like sshd for a user whose login shell is `shell`.
        let force = dir.join("force.sh");
        std::fs::write(
            &force,
            format!(
                "#!/bin/sh\n\
                 HOME={home}\n\
                 SHELL={shell_path}\n\
                 PHOSPHOR_TEST_SSHD=remote\n\
                 WARP_PROFILE_LOADED=true\n\
                 export HOME SHELL PHOSPHOR_TEST_SSHD WARP_PROFILE_LOADED\n\
                 cd \"$HOME\" || exit 1\n\
                 if [ -n \"$SSH_ORIGINAL_COMMAND\" ]; then exec \"$SHELL\" -c \"$SSH_ORIGINAL_COMMAND\"; fi\n\
                 echo \"Welcome to Ubuntu (phosphor local test sshd)\"\n\
                 exec \"$SHELL\" -l -i\n",
                home = sh_quote(&home.to_string_lossy()),
                shell_path = sh_quote(&shell_path.to_string_lossy()),
            ),
        )
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
/// printed to stderr. With `PHOSPHOR_REQUIRE_SSH_FIXTURE=1` (the `integration-linux` CI job)
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
            eprintln!("SKIP ssh integration scenario (remote shell `{shell}`): {why}");
            false
        }
    }
}

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
