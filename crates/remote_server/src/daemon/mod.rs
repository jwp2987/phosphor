//! The warpui-free daemon core of the bundled `phosphor-remote` helper (feature `daemon`, unix).
//!
//! * [`run_proxy`]: `phosphor-remote proxy <key>`. Uses a ControlMaster-like pattern (flock +
//!   fork + exec) to start the daemon and bridges the ssh stdio channel to its Unix socket.
//! * [`run_daemon`]: `phosphor-remote daemon <key>`. Binds a Unix domain socket, accepts
//!   multiple concurrent proxy connections and exits after a grace period with none.
//!
//! The request handlers cover the families in [`crate::capability::HELPER_CAPABILITIES`] (repo
//! metadata and its live file tree arrived with the watch infrastructure in `watch.rs`; diff
//! state, git status and GitHub info follow the repository through `repo_watch.rs`; codebase
//! indexing is host-scan / app-embed, `index_store.rs`);
//! everything else (the legacy host-embedding codebase index, buffers, git write operations) is answered with
//! `ErrorCode::UNSUPPORTED` and the client runs it on its no-helper path. Wave H, #1804.

mod agent_context;
mod buffers;
mod chunk;
mod conn;
mod diff_state;
mod exec;
mod file_context;
mod fs_ops;
mod git;
pub mod git_runner;
mod git_status;
mod git_write;
mod github;
mod index_store;
mod jobs;
#[cfg(test)]
mod live_tests_support;
pub mod logger;
mod proxy;
mod repo_meta;
mod repo_watch;
mod ripgrep;
mod server;
mod watch;

use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;

/// Entry point for `phosphor-remote proxy <key>`.
pub fn run_proxy(identity_key: &str) -> anyhow::Result<()> {
    proxy::run(identity_key)
}

/// Entry point for `phosphor-remote index-scan <repo> [--known <file>]`: the deterministic JSON
/// (manifest, fragment hashes, root hash; or only what changed since `known`).
pub fn run_index_scan(
    repo: &std::path::Path,
    known: Option<&std::path::Path>,
) -> anyhow::Result<String> {
    chunk::run_index_scan(repo, known)
}

/// Entry point for `phosphor-remote daemon <key>`: binds the socket, writes the pid file,
/// signals readiness to the proxy, then serves until idle for the grace period.
pub fn run_daemon(identity_key: &str) -> anyhow::Result<()> {
    // socket: {base}/{identity}/server-{version_hash}.sock -- the proxies connect to it.
    // pid:    {base}/{identity}/server-{version_hash}.pid  -- the proxy flocks it and uses
    //         kill(pid, 0) on its contents to see whether a daemon is alive.
    let socket_path = proxy::socket_path(identity_key);
    let pid_path = proxy::pid_path(identity_key);

    if let Some(parent) = socket_path.parent() {
        proxy::ensure_private_daemon_dir(parent)?;
        logger::init(&parent.join("helper.log"));
    }
    if socket_path.exists() {
        std::fs::remove_file(&socket_path)?;
    }
    let listener = std::os::unix::net::UnixListener::bind(&socket_path)?;
    std::fs::set_permissions(&socket_path, Permissions::from_mode(0o600))?;
    log::info!("Daemon bound to {}", socket_path.display());

    std::fs::write(&pid_path, std::process::id().to_string())?;
    // The socket is bound and listening and the PID file is written: tell the proxy.
    proxy::signal_ready_from_env();

    let (events_tx, events_rx) = std::sync::mpsc::channel::<server::Event>();
    let accept_events = events_tx.clone();
    std::thread::Builder::new()
        .name("accept".into())
        .spawn(move || {
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => {
                        let conn = server::next_conn_id();
                        log::info!("Daemon: accepted connection {conn}");
                        conn::spawn_connection(stream, conn, accept_events.clone());
                    }
                    Err(e) => log::error!("Daemon: accept error: {e}"),
                }
            }
        })?;
    // The core owns the receiver; the accept thread and the connections hold senders. The core
    // keeps one more (for the filesystem watcher and its workers); it never blocks the core
    // from ending, because the idle grace period ends it.
    let events_tx_for_core = events_tx;

    server::Server::new(server::GRACE_PERIOD)
        .with_events(events_tx_for_core)
        .run(events_rx);

    let _ = std::fs::remove_file(&socket_path);
    let _ = std::fs::remove_file(&pid_path);
    log::info!("Daemon exiting");
    Ok(())
}

/// A connected in-process daemon for the backend contract tests: the returned stream is the
/// client end; the server core runs on its own thread until the stream is dropped.
#[cfg(test)]
pub(crate) fn spawn_in_process_for_tests() -> std::os::unix::net::UnixStream {
    spawn_in_process(None)
}

/// [`spawn_in_process_for_tests`] whose agent context scans `home` instead of the process's.
#[cfg(test)]
pub(crate) fn spawn_in_process_with_home_for_tests(
    home: &std::path::Path,
) -> std::os::unix::net::UnixStream {
    spawn_in_process(Some(home.to_path_buf()))
}

#[cfg(test)]
fn spawn_in_process(home: Option<std::path::PathBuf>) -> std::os::unix::net::UnixStream {
    let (client_end, server_end) =
        std::os::unix::net::UnixStream::pair().expect("socketpair for the in-process daemon");
    let (events_tx, events_rx) = std::sync::mpsc::channel::<server::Event>();
    conn::spawn_connection(server_end, server::next_conn_id(), events_tx.clone());
    std::thread::Builder::new()
        .name("in-process-daemon".into())
        .spawn(move || {
            // The index stores go to a scratch directory, never the real home.
            let index_root = tempfile::tempdir().expect("scratch index directory");
            let mut server = server::Server::new(server::GRACE_PERIOD)
                .with_index_root(index_root.path().to_path_buf());
            if let Some(home) = home {
                server = server.with_home(home);
            }
            server.with_events(events_tx).run(events_rx);
        })
        .expect("spawn the in-process daemon");
    client_end
}
