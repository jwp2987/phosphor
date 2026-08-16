use std::fs::{File, OpenOptions};
use std::os::unix::io::AsRawFd;
use std::sync::{LazyLock, Mutex};

use ipc::ServerBuilder;
use warp_core::channel::ChannelState;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::service_impl::UriServiceImpl;

/// RAII wrapper around an `flock`'d lock file. The kernel releases the lock automatically when
/// the last file descriptor referencing it closes -- including on crash -- matching the
/// crash-safety property of Windows' `CreateMutexW`-based equivalent.
struct LockFileHandle(#[allow(dead_code)] File);

enum LockState {
    /// We hold the real lock; also responsible for running the IPC server.
    Acquired(LockFileHandle),
    /// Another live instance already holds the lock.
    AnotherInstanceRunning,
    /// Couldn't cleanly determine lock ownership (e.g. the state directory isn't writable).
    /// Treated the same as `Acquired` for launch purposes -- see `is_sole_running_instance` --
    /// but without a real lock file handle, so no server is started for this process.
    ErrorAssumeSole,
}

/// The single-instance lock state. Lives for the process lifetime.
///
/// * `LazyLock` -- goes from un-initialized to initialized without `mut`, not vice-versa.
/// * `Mutex` -- interior mutability; we don't actually need to access it from another thread,
///   but a `static` needs `Sync`.
static SOLE_INSTANCE_LOCK: LazyLock<Mutex<LockState>> =
    LazyLock::new(|| Mutex::new(try_acquire_lock()));

pub(super) fn uri_socket_name() -> String {
    format!("Zap{:?}_URI_CHANNEL", ChannelState::channel())
}

fn lock_file_path() -> std::io::Result<std::path::PathBuf> {
    let dir = warp_core::paths::state_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join(format!("single-instance-{:?}.lock", ChannelState::channel())))
}

fn try_acquire_lock() -> LockState {
    let path = match lock_file_path() {
        Ok(path) => path,
        Err(err) => {
            log::error!("Failed to resolve Phosphor single-instance lock path, allowing launch anyway: {err:#}");
            return LockState::ErrorAssumeSole;
        }
    };
    let file = match OpenOptions::new().create(true).write(true).open(&path) {
        Ok(file) => file,
        Err(err) => {
            log::error!("Failed to open Phosphor single-instance lock file, allowing launch anyway: {err:#}");
            return LockState::ErrorAssumeSole;
        }
    };
    // SAFETY: `flock` is a simple syscall; `file`'s fd is valid for the duration of the call,
    // and we keep `file` alive afterward (inside `LockFileHandle`) for as long as the lock
    // needs to be held.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result == 0 {
        LockState::Acquired(LockFileHandle(file))
    } else {
        let err = std::io::Error::last_os_error();
        if err.kind() == std::io::ErrorKind::WouldBlock {
            // Another instance already holds the lock -- the expected, common case.
            LockState::AnotherInstanceRunning
        } else {
            log::error!("Failed to acquire Phosphor single-instance lock, allowing launch anyway: {err:#}");
            LockState::ErrorAssumeSole
        }
    }
}

/// A singleton model that is responsible for ensuring there is only one instance of Zap running.
pub(super) struct SingleInstanceManager {
    _server: Option<ipc::Server>,
}

impl SingleInstanceManager {
    /// Starts the URI-forwarding IPC server, but only if this process actually holds the real
    /// single-instance lock -- an unlocked "assume sole" fallback process has no way to
    /// guarantee it's uniquely reachable at `uri_socket_name()`, so it doesn't try to serve.
    pub(super) fn new(ctx: &mut ModelContext<Self>) -> Self {
        let holds_lock = matches!(
            &*SOLE_INSTANCE_LOCK.lock().unwrap_or_else(|p| p.into_inner()),
            LockState::Acquired(_)
        );
        if !holds_lock {
            return Self { _server: None };
        }

        let (tx, rx) = async_channel::unbounded();
        let server = match ServerBuilder::default()
            .with_fixed_address(uri_socket_name())
            .with_service(UriServiceImpl::new(tx))
            .build_and_run(ctx.background_executor())
        {
            Ok((server, _)) => {
                ctx.spawn_stream_local(
                    rx,
                    |_single_instance_manager, event, ctx| {
                        for uri in event {
                            crate::uri::handle_incoming_uri(&uri, ctx);
                        }
                    },
                    |_, _| {},
                );
                Some(server)
            }
            Err(err) => {
                log::error!("Failed to initialize UriService Server: {err:#}");
                None
            }
        };

        Self { _server: server }
    }

    /// Returns whether or not this process should be treated as the main instance of Zap.
    ///
    /// NOTE: `ErrorAssumeSole` counts as sole -- it's better to open a second (unlocked)
    /// instance than to fail to launch at all because of a lock-file hiccup.
    pub(super) fn is_sole_running_instance() -> bool {
        !matches!(
            &*SOLE_INSTANCE_LOCK.lock().unwrap_or_else(|p| p.into_inner()),
            LockState::AnotherInstanceRunning
        )
    }
}

impl Entity for SingleInstanceManager {
    type Event = ();
}

impl SingletonEntity for SingleInstanceManager {}
