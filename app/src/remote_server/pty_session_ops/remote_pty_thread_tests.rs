use std::os::unix::fs::PermissionsExt as _;

use serial_test::serial;

use super::*;
use crate::terminal::local_tty::spawner::PtySpawner;
use crate::terminal::shell::ShellType;

// Fix for the read loop fabricating a signalled exit on a normal exit race:
// it used to treat every read error other than `WouldBlock`/`Interrupted` as
// fatal, including the `EIO` (-> `ErrorKind::Other`) a pty master read
// commonly returns on Linux/FreeBSD once the slave hangs up -- see
// `is_benign_pty_hangup_read_error`'s doc comment. This pins the
// classification directly rather than trying to race a real read against a
// real child's exit, which is inherently timing-dependent and would make
// this test flaky for exactly the race the fix is about.
//
// Breaks if: `is_benign_pty_hangup_read_error` stops treating
// `ErrorKind::Other` as benign, or starts treating some other kind as benign.
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
#[test]
fn benign_pty_hangup_read_error_classification() {
    assert!(is_benign_pty_hangup_read_error(&std::io::Error::from(
        std::io::ErrorKind::Other
    )));

    for kind in [
        std::io::ErrorKind::WouldBlock,
        std::io::ErrorKind::Interrupted,
        std::io::ErrorKind::NotFound,
        std::io::ErrorKind::PermissionDenied,
        std::io::ErrorKind::BrokenPipe,
    ] {
        assert!(
            !is_benign_pty_hangup_read_error(&std::io::Error::from(kind)),
            "{kind:?} must not be treated as a benign hangup"
        );
    }
}

// Fix for the pty leak in `spawn_with_shell_starter`: `Pty::new` succeeding
// only guarantees a live child exists, not that this function will return
// `Ok` -- `Poll::new`, `Waker::new`, `pty.register`, and
// `thread::Builder::spawn` can each still fail afterwards, and neither `Pty`
// nor `DirectPtyHandle` reaps a child on drop. `KillPtyOnDrop` is the guard
// that closes that gap; this test exercises the guard directly rather than
// trying to force one of those four calls to fail. `Poll::new`/`Waker::new`
// need fd-exhaustion, and `thread::Builder::spawn` needs thread/process-limit
// exhaustion, to fail at all -- none of which can be induced hermetically
// without also perturbing every other test running in this process, so a
// direct test of the guard's own cleanup semantics is what is actually
// achievable here.
//
// Breaks if: `KillPtyOnDrop`'s `Drop` impl stops calling `EventedPty::kill`
// (or is removed outright), which is exactly what "just call `pty.kill()` on
// each error branch instead of a guard" would silently regress the next time
// a fallible step is added between `Pty::new` and the guard being disarmed.
#[test]
fn dropping_an_undisarmed_guard_kills_and_reaps_the_child() {
    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(|_ctx| PtySpawner::new_for_test());

        let shell_starter = ShellStarter::Direct(DirectShellStarter::new_for_test(
            ShellType::Bash,
            PathBuf::from("/bin/cat"),
            Vec::new(),
        ));
        let options = PtyOptions {
            size: SizeInfo::new_without_font_metrics(24, 80),
            window_id: None,
            shell_starter,
            start_dir: Some(std::env::temp_dir()),
            env_vars: std::collections::HashMap::new(),
            enable_ssh_wrapper: false,
            reuse_ssh_control_master: false,
            shell_debug_mode: false,
            honor_ps1: false,
            node_version_chip_enabled: false,
            close_fds: true,
        };

        let pty = app
            .update(|ctx| Pty::new(options, false, ctx))
            .expect("spawning /bin/cat in a real pty should succeed");
        let pid = pty.get_pid() as libc::pid_t;

        // Stands in for any of `spawn_with_shell_starter`'s failure paths
        // between a successful `Pty::new` and the guard being disarmed:
        // drop it without ever calling `disarm`.
        drop(KillPtyOnDrop::new(pty));

        // `KillPtyOnDrop::drop` calls `EventedPty::kill`, which blocks on
        // `Child::wait` -- so by the time the `drop` above has returned, the
        // child has already been reaped and its pid is free for reuse.
        // `kill(pid, 0)` delivers no signal (0 is the standard
        // existence/permission probe), so it must now report ESRCH ("no
        // such process").
        let probe = unsafe { libc::kill(pid, 0) };
        assert_eq!(probe, -1, "the child must no longer exist");
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH),
            "the child must already be reaped, not merely killed"
        );
    });
}

/// Guards the process-wide `SHELL` env var for the duration of a test,
/// restoring its original value (or absence) on drop -- including on
/// panic/unwind, since `#[serial]` alone only keeps tests from *racing* on
/// the var, not from leaving it mutated for whichever test runs next in this
/// process.
struct ShellEnvGuard(Option<std::ffi::OsString>);

impl ShellEnvGuard {
    fn set(value: &std::ffi::OsStr) -> Self {
        let guard = Self(std::env::var_os("SHELL"));
        // SAFETY: serialized via #[serial(remote_pty_thread_shell_env)] --
        // no other test in this process observes SHELL while this guard is
        // alive.
        unsafe { std::env::set_var("SHELL", value) };
        guard
    }
}

impl Drop for ShellEnvGuard {
    fn drop(&mut self) {
        // SAFETY: see `set`.
        unsafe {
            match &self.0 {
                Some(value) => std::env::set_var("SHELL", value),
                None => std::env::remove_var("SHELL"),
            }
        }
    }
}

/// Writes a trivial, executable file at `dir/name` and returns its path.
/// That is enough for `supported_shell_path_and_type` to resolve it as a
/// supported shell binary -- it only checks that the path exists, is
/// executable, and is *named* one of bash/zsh/fish; it never runs it.
fn fake_shell_binary(dir: &std::path::Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"#!/bin/sh\n").expect("write fake shell binary");
    let mut perms = std::fs::metadata(&path)
        .expect("stat fake shell binary")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&path, perms).expect("chmod fake shell binary");
    path
}

// Fix for `resolve_shell_starter`'s fallback chain: it used to try `$SHELL`
// before the passwd-entry resolution `ShellStarter::compute_fallback_shell`
// uses for a local session, a materially weaker signal since a daemon
// started by a service manager has no inherited login shell. Planting a
// fake, resolvable "bash" at a path nothing else on the host could ever
// produce pins this down: if `resolve_shell_starter` ever reads `$SHELL`
// again, it resolves to *this* path, which `compute_fallback_shell` could
// never independently produce.
//
// Breaks if: `resolve_shell_starter`'s `None` branch reads `$SHELL` again
// before falling back to `ShellStarter::compute_fallback_shell`.
#[test]
#[serial(remote_pty_thread_shell_env)]
fn resolve_shell_starter_fallback_ignores_shell_env_var() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let fake_bash = fake_shell_binary(dir.path(), "bash");
    let _guard = ShellEnvGuard::set(fake_bash.as_os_str());

    let resolved = resolve_shell_starter(None).expect("a supported shell should still resolve");
    let expected = ShellStarter::from(ShellStarter::compute_fallback_shell().expect(
        "this host must have at least one fallback shell for the rest of the suite to run at all",
    ));

    let (ShellStarter::Direct(resolved), ShellStarter::Direct(expected)) = (resolved, expected)
    else {
        panic!(
            "resolve_shell_starter's fallback and compute_fallback_shell's result always \
             convert to ShellStarter::Direct"
        );
    };

    assert_ne!(
        resolved.shell_path(),
        fake_bash.as_path(),
        "resolve_shell_starter must not resolve to the fake $SHELL"
    );
    assert_eq!(resolved.shell_path(), expected.shell_path());
    assert_eq!(resolved.shell_type(), expected.shell_type());
}
