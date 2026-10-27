//! The ssh wrapper's background ControlMaster (#1035).
//!
//! `warp_ssh_helper` starts a Phosphor-owned `ssh -o ControlMaster=yes -N -f` master at
//! `$SSH_SOCKET_DIR/<local shell session id>` and runs the session as a client of it. The
//! wrapper ends the master when the session returns, but a killed shell cannot, and a Windows
//! OpenSSH server never closes an abandoned session channel, so `ControlPersist` alone never
//! expires it. The app therefore ends the master itself when a pane is discarded
//! (`TerminalView::shutdown_for_discard`) and when the app quits ([`stop_all_blocking`]).
//!
//! Only sockets for sessions THIS process saw start a wrapped ssh are ever touched, so a second
//! running instance's masters are left alone.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long quitting waits for the `ssh -O exit` calls (all run concurrently).
pub const QUIT_STOP_BUDGET: Duration = Duration::from_secs(1);

static REGISTERED: Mutex<Option<HashSet<PathBuf>>> = Mutex::new(None);

/// The control socket the wrapper uses for the local shell session `session_id`. A leading `~`
/// in `socket_dir` (the app exports the literal `~/.ssh`) is expanded like ssh does.
pub fn socket_path(socket_dir: &str, session_id: u64) -> PathBuf {
    let dir = shellexpand::tilde(socket_dir).into_owned();
    PathBuf::from(dir).join(session_id.to_string())
}

/// Remembers that a wrapped ssh started in the session owning `socket`.
pub fn register(socket: PathBuf) {
    if let Ok(mut set) = REGISTERED.lock() {
        set.get_or_insert_with(HashSet::new).insert(socket);
    }
}

/// Forgets `socket` (its pane was discarded and its master stopped).
pub fn unregister(socket: &Path) {
    if let Ok(mut set) = REGISTERED.lock() {
        if let Some(set) = set.as_mut() {
            set.remove(socket);
        }
    }
}

fn registered() -> Vec<PathBuf> {
    REGISTERED
        .lock()
        .ok()
        .and_then(|set| set.as_ref().map(|set| set.iter().cloned().collect()))
        .unwrap_or_default()
}

fn is_socket(path: &Path) -> bool {
    use std::os::unix::fs::FileTypeExt as _;
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket())
}

/// Runs `ssh -O exit` for every registered socket that still exists, concurrently, waiting at
/// most `budget` in total and killing a call that is still running then. Blocking: for the
/// app-quit path, where nothing async is available any more.
pub fn stop_all_blocking(budget: Duration) {
    stop_sockets_blocking("ssh", &registered(), budget);
}

pub(crate) fn stop_sockets_blocking(program: &str, sockets: &[PathBuf], budget: Duration) {
    let deadline = Instant::now() + budget;
    let mut children = Vec::new();
    for socket in sockets.iter().filter(|socket| is_socket(socket)) {
        match Command::new(program)
            .arg("-O")
            .arg("exit")
            .args(remote_server::ssh::ssh_args(socket))
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            Ok(child) => children.push((socket.clone(), child)),
            Err(e) => log::info!("could not run ssh -O exit for {}: {e}", socket.display()),
        }
    }
    for (socket, mut child) in children {
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = child.kill();
                    let _ = child.wait();
                    log::warn!(
                        "ssh -O exit for {} did not finish in time",
                        socket.display()
                    );
                    break;
                }
            }
        }
        remote_server::ssh::remove_stale_control_socket(&socket);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt as _;
    use std::os::unix::net::UnixListener;

    #[test]
    fn socket_path_expands_the_literal_tilde_the_app_exports() {
        let home = dirs::home_dir().unwrap();
        assert_eq!(
            socket_path("~/.ssh", 42),
            home.join(".ssh").join("42"),
            "the app exports SSH_SOCKET_DIR=~/.ssh literally"
        );
        assert_eq!(socket_path("/abs/dir", 7), PathBuf::from("/abs/dir/7"));
    }

    fn stub(dir: &Path, body: &str) -> String {
        let path = dir.join("fakessh");
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path.display().to_string()
    }

    /// App quit runs `ssh -O exit` for each live wrapper master, and only for sockets that exist
    /// (a registered id whose session never started a master is skipped).
    #[test]
    fn quit_stops_every_live_wrapper_master_and_skips_missing_sockets() {
        let dir = PathBuf::from(format!("/tmp/wm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = dir.join("log");
        let ssh = stub(&dir, &format!("echo \"$@\" >> {}", log.display()));
        let live_a = dir.join("1");
        let live_b = dir.join("2");
        let _a = UnixListener::bind(&live_a).unwrap();
        let _b = UnixListener::bind(&live_b).unwrap();
        let missing = dir.join("3");

        stop_sockets_blocking(
            &ssh,
            &[live_a.clone(), live_b.clone(), missing.clone()],
            Duration::from_secs(5),
        );

        let logged = std::fs::read_to_string(&log).unwrap();
        for socket in [&live_a, &live_b] {
            assert!(
                logged.lines().any(|line| line.starts_with("-O exit")
                    && line.contains(&format!("ControlPath={}", socket.display()))),
                "no ssh -O exit for {}: {logged}",
                socket.display()
            );
        }
        assert!(!logged.contains(&missing.display().to_string()), "{logged}");
        assert_eq!(logged.lines().count(), 2, "{logged}");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A wedged `ssh -O exit` cannot delay quitting beyond the budget.
    #[test]
    fn quit_is_bounded_when_ssh_hangs() {
        let dir = PathBuf::from(format!("/tmp/wmh-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ssh = stub(&dir, "sleep 30");
        let live = dir.join("1");
        let _l = UnixListener::bind(&live).unwrap();
        let started = Instant::now();
        stop_sockets_blocking(&ssh, &[live], Duration::from_millis(300));
        assert!(started.elapsed() < Duration::from_secs(5));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn register_and_unregister_round_trip() {
        let socket = PathBuf::from("/tmp/wm-registry-test/1");
        register(socket.clone());
        assert!(registered().contains(&socket));
        unregister(&socket);
        assert!(!registered().contains(&socket));
    }
}
