use std::cell::RefCell;
use std::collections::HashMap;

use ::settings::Setting as _;
use cfg_if::cfg_if;
use lazy_static::lazy_static;
use parking_lot::RwLock;
use warp_cli::RecoveryMechanism;
use warp_core::channel::{Channel, ChannelState};
use warpui::{Entity, ModelContext, SingletonEntity, WindowId};
use warpui_extras::user_preferences::UserPreferences;

use crate::{report_if_error, settings};

/// Keep in sync with [`warp_cli::AppArgs`].
pub const RECOVERY_MECHANISM_ARG: &str = "crash-recovery-mechanism";

lazy_static! {
    static ref IS_CRASH_RECOVERY_PROCESS_RUNNING: RwLock<bool> = RwLock::new(false);
}

#[cfg_attr(not(feature = "crash_reporting"), allow(dead_code))]
pub fn is_crash_recovery_process_running() -> bool {
    *IS_CRASH_RECOVERY_PROCESS_RUNNING.read()
}

pub enum Event {
    /// User has acknowledged the fact that the application crashed and
    /// recovered from the crash.
    UserAcknowledgedCrash,
    /// The crash recovery process was successfully torn down.
    CrashRecoveryProcessTornDown,
}

/// Returns true if this process is the crash recovery process.
pub fn is_crash_recovery_process(args: &warp_cli::AppArgs) -> bool {
    args.crash_recovery_mechanism.is_some()
}

#[derive(Debug)]
enum DrawFrameResult {
    Successful,
    Errored,
}

/// Wrapper struct that holds state for the crash recovery process. Notably, when the crash recovery
/// process is killed, all of the state within this struct is dropped.
struct CrashRecoveryProcess {
    process: std::process::Child,
    /// The number of consecutive errors seen per window.
    consecutive_errors_per_window: HashMap<WindowId, usize>,
    /// The number of successful frames drawn per window.
    successful_frames_per_window: HashMap<WindowId, usize>,
    /// The current window's recent sequence of render successes/failures; exits after writing to the local log once too many consecutive failures occur.
    sequence_of_renders_per_window: HashMap<WindowId, Vec<DrawFrameResult>>,
    is_alive: bool,
}

impl CrashRecoveryProcess {
    fn new(process: std::process::Child) -> Self {
        Self {
            process,
            consecutive_errors_per_window: Default::default(),
            successful_frames_per_window: Default::default(),
            sequence_of_renders_per_window: Default::default(),
            is_alive: true,
        }
    }

    /// Kills the crash recovery process. Noop if the process isn't running anymore.
    fn kill(&mut self) {
        if !self.is_alive {
            return;
        }

        let _ = self.process.kill();
        let _ = self.process.wait();

        *IS_CRASH_RECOVERY_PROCESS_RUNNING.write() = false;
        self.is_alive = false;
        warp_logging::on_crash_recovery_process_killed();
    }

    fn handle_draw_frame_error(&mut self, window_id: WindowId) {
        /// Number of occurrences of a draw frame error before we log.
        const NUM_DRAW_ERRORS_BEFORE_EXITING: usize = 3;

        self.sequence_of_renders_per_window
            .entry(window_id)
            .or_default()
            .push(DrawFrameResult::Errored);

        let num_errors = self
            .consecutive_errors_per_window
            .entry(window_id)
            .or_default();
        *num_errors += 1;

        if *num_errors >= NUM_DRAW_ERRORS_BEFORE_EXITING {
            log::warn!(
                "Exiting process due to draw frame errors, last 10 frames: {:#?}",
                self.sequence_of_renders_per_window
                    .get(&window_id)
                    .expect("sequence of renders map cannot be empty")
            );
            log::error!(
                "Failed to render a frame {NUM_DRAW_ERRORS_BEFORE_EXITING} times in a row; exiting..."
            );

            // Stop the local crash-reporting process before exiting, to avoid leaving behind a background child process.
            #[cfg(feature = "crash_reporting")]
            crate::crash_reporting::uninit_crash_reporting();

            std::process::exit(1);
        }
    }

    /// Returns whether the crash recovery process is alive.
    fn is_alive(&self) -> bool {
        self.is_alive
    }

    /// Handles the case where we were able to successfully draw a frame. Returns `true` if this
    /// triggered the crash recovery process to be killed.
    fn handle_frame_drawn(&mut self, window_id: WindowId) {
        /// The number of successful frames of a given Window before we tear down the crash
        /// reporting process. We don't tear down the crash recovery process on the first frame
        /// because there are cases where a call to render returns `Ok` even though the render
        /// wasn't actually successful. From the perspective of a user, we don't want to tear down
        /// the crash recovery process until we feel confident the app won't crash from the
        /// discrete --> integrated or Xwayland --> native Wayland change. This may require multiple
        /// "successful" frames in the case where we _think_ a frame was successful but it failed to
        /// present.
        const NUM_SUCCESSFUL_DRAW_FRAMES_PER_WINDOW: usize = 10;

        // Reset the number of errors now that we've seen a successful render for this window.
        self.consecutive_errors_per_window.insert(window_id, 0);

        self.sequence_of_renders_per_window
            .entry(window_id)
            .or_default()
            .push(DrawFrameResult::Successful);

        let num_successful_draws = self
            .successful_frames_per_window
            .entry(window_id)
            .or_default();
        *num_successful_draws += 1;

        if *num_successful_draws >= NUM_SUCCESSFUL_DRAW_FRAMES_PER_WINDOW {
            // Once we've managed to successfully draw frames, we can kill the
            // crash recovery child process and collect its exit status.
            self.kill();

            log::info!(
                "Successfully drew {NUM_SUCCESSFUL_DRAW_FRAMES_PER_WINDOW} frames; killing crash recovery child process"
            );
        }
    }
}

pub struct CrashRecovery {
    child_process: RefCell<Option<CrashRecoveryProcess>>,

    /// If the user should be notified that we recovered from a crash, this
    /// stores the recovery mechanism that was used to successfully recover
    /// from the crash.
    should_notify_user_about_crash: Option<RecoveryMechanism>,
}

impl CrashRecovery {
    pub fn new(launch_mode: &crate::LaunchMode, user_preferences: &dyn UserPreferences) -> Self {
        let mut should_notify_user_about_crash = None;

        let args = launch_mode.args();
        if let Some(recovery_mechanism) = args.as_ref().crash_recovery_mechanism {
            // If we're a crash recovery process, wait for the parent to crash
            // before letting execution continue.
            wait_for_parent_crash(args.as_ref());

            // If we get to this point, the parent crashed. Handle the crash and then continue
            // execution, allowing another crash recovery process to start if necessary.
            should_notify_user_about_crash =
                handle_parent_crash(recovery_mechanism, user_preferences)
                    .then_some(recovery_mechanism);
        }

        // If we want automated recovery from a crash in this process, spawn a
        // a child recovery process that uses the given crash recovery
        // mechanism.
        if launch_mode.crash_recovery_enabled() {
            if let Some(recovery_mechanism) = choose_crash_recovery_mechanism(user_preferences) {
                let child_process = match spawn_recovery_process(recovery_mechanism) {
                    Ok(child_process) => child_process,
                    Err(err) => {
                        log::error!("Failed to spawn crash recovery child process: {err:#}");
                        return Self {
                            child_process: Default::default(),
                            should_notify_user_about_crash,
                        };
                    }
                };

                *IS_CRASH_RECOVERY_PROCESS_RUNNING.write() = true;
                return Self {
                    child_process: RefCell::new(Some(CrashRecoveryProcess::new(child_process))),
                    should_notify_user_about_crash,
                };
            }
        }

        Self {
            child_process: Default::default(),
            should_notify_user_about_crash: None,
        }
    }

    #[cfg(test)]
    pub fn register_for_test(app: &mut warpui::App) {
        use warp_core::user_preferences::GetUserPreferences as _;

        app.update(|ctx| {
            ctx.add_singleton_model(|ctx| {
                let user_preferences = ctx.private_user_preferences();
                let launch_mode = crate::LaunchMode::App {
                    args: warp_cli::AppArgs::default(),
                    api_key: None,
                };
                crate::crash_recovery::CrashRecovery::new(&launch_mode, user_preferences)
            })
        });
    }

    pub fn should_notify_user_about_crash(&self) -> Option<RecoveryMechanism> {
        self.should_notify_user_about_crash
    }

    pub fn handle_user_acknowledged_crash(&mut self, ctx: &mut ModelContext<Self>) {
        self.should_notify_user_about_crash = None;
        ctx.emit(Event::UserAcknowledgedCrash);
    }

    pub fn on_draw_frame_error(&mut self, window_id: WindowId) {
        if let Some(child_process) = self.child_process.borrow_mut().as_mut() {
            child_process.handle_draw_frame_error(window_id);
        }
    }

    pub fn on_frame_drawn(&self, window_id: WindowId, ctx: &mut ModelContext<Self>) {
        let mut child_process_borrow = self.child_process.borrow_mut();
        let mut child_process = child_process_borrow.take();

        if let Some(child_process) = child_process.as_mut() {
            child_process.handle_frame_drawn(window_id);

            // If the process is no longer alive, fire a `CrashRecoveryProcessTornDown` event. We
            // do this here as opposed to below to ensure we only omit the event once as opposed to
            // on every render.
            if !child_process.is_alive {
                ctx.emit(Event::CrashRecoveryProcessTornDown);
            }
        }

        let is_child_process_alive = child_process
            .as_ref()
            .map(CrashRecoveryProcess::is_alive)
            .unwrap_or_default();

        if is_child_process_alive {
            *child_process_borrow = child_process;
        }
    }

    pub fn teardown(&mut self) {
        if let Some(mut child_process) = self.child_process.take() {
            child_process.kill();
        }
    }
}

impl Entity for CrashRecovery {
    type Event = Event;
}

impl SingletonEntity for CrashRecovery {}

/// Returns the crash recovery mechanism that we want to use to handle a crash
/// in the current process.
///
/// If non-None, a child process will be spawned that uses the provided
/// mechanism if it detects that this process has crashed.
fn choose_crash_recovery_mechanism(
    user_preferences: &dyn UserPreferences,
) -> Option<RecoveryMechanism> {
    if ChannelState::channel() == Channel::Integration {
        return None;
    }

    #[cfg(target_os = "linux")]
    {
        let force_x11 = settings::ForceX11::read_from_preferences(user_preferences);
        // Prioritize X11 crash recovery first. If the user has actively
        // enabled Wayland, we want to check its stability over the case where we fallback to the
        // integrated GPU because the user hasn't explicitly set a value.
        if (force_x11 == Some(false)) && (std::env::var_os("WAYLAND_DISPLAY").is_some()) {
            return Some(RecoveryMechanism::X11);
        }
    }

    let wgpu_backends = wgpu::Backends::from_env();
    // On Windows, if we haven't overridden the set of wgpu backends to use,
    // spawn a crash recovery process that won't attempt to initialize the
    // OpenGL backend.
    if cfg!(windows) && wgpu_backends.is_none() {
        return Some(RecoveryMechanism::DisableOpenGL);
    }

    let is_only_vulkan_enabled = wgpu_backends
        .as_ref()
        .is_some_and(|backends| *backends == wgpu::Backends::VULKAN);

    // If a backend other than Vulkan is enabled, start a crash recovery process to force the use of
    // Vulkan.
    if cfg!(windows) && !is_only_vulkan_enabled {
        return Some(RecoveryMechanism::ForceVulkan);
    }

    // If the user hasn't specified a preference for which type of GPU to use,
    // try recovering from a crash by forcing the use of the dedicated GPU.
    let prefer_low_power_gpu = settings::PreferLowPowerGPU::read_from_preferences(user_preferences);
    if prefer_low_power_gpu.is_none() {
        return Some(RecoveryMechanism::DedicatedGpu);
    }

    None
}

fn spawn_recovery_process(
    recovery_mechanism: RecoveryMechanism,
) -> std::io::Result<std::process::Child> {
    log::debug!("Spawning crash recovery child process");
    let current_exe = std::env::current_exe()?;
    let mut command = command::blocking::Command::new(current_exe);
    cfg_if! {
        if #[cfg(windows)] {
            use windows::Win32::System::Threading;

            // Create a handle to our process that allows a recipient to
            // read our process ID and wait on our termination.  This is
            // more robust than passing a process ID, as Windows can reuse
            // process IDs.
            let handle = unsafe { Threading::OpenProcess(
                Threading::PROCESS_QUERY_LIMITED_INFORMATION | Threading::PROCESS_SYNCHRONIZE,
                true,
                Threading::GetCurrentProcessId()
            )? };
            // Pass this handle to the child by serialized value.
            command.arg(format!("--parent-handle={}", handle.0 as isize));
        } else {
            command.arg(format!("--parent-pid={}", std::process::id()));
        }
    }

    match recovery_mechanism {
        RecoveryMechanism::DisableOpenGL => {
            // If our recovery mechanism is to disable OpenGL, set an explicit list of
            // wgpu backends that doesn't include it.
            command.env("WGPU_BACKEND", "vulkan,dx12");
        }
        RecoveryMechanism::ForceVulkan => {
            command.env("WGPU_BACKEND", "vulkan");
        }
        _ => {}
    }

    command
        .arg(format!("--crash-recovery-mechanism={recovery_mechanism}",))
        .spawn()
}

#[cfg(windows)]
fn wait_for_parent_crash(args: &warp_cli::AppArgs) {
    use windows::Win32::{
        Foundation::{GetLastError, WAIT_FAILED, WAIT_OBJECT_0},
        System::Threading::{GetProcessId, INFINITE, WaitForSingleObject},
    };

    let parent_handle = match args.parent.handle {
        Some(handle) => handle.into_inner(),
        None => panic!("--parent-handle must be set if {RECOVERY_MECHANISM_ARG} is set"),
    };

    unsafe {
        log::debug!(
            "Waiting for parent with pid {} to crash...",
            GetProcessId(parent_handle)
        );
        loop {
            let result = WaitForSingleObject(parent_handle, INFINITE);
            if result == WAIT_OBJECT_0 {
                log::info!("Parent has crashed; continuing execution.");
                break;
            } else if result == WAIT_FAILED {
                log::error!(
                    "Encountered error while waiting on parent process: {:?}",
                    GetLastError()
                );
            }
        }
    }
}

#[cfg(unix)]
fn wait_for_parent_crash(args: &warp_cli::AppArgs) {
    use nix::unistd::Pid;

    let parent_pid = args
        .parent
        .pid
        .map(|pid| Pid::from_raw(pid as i32))
        .unwrap_or_else(|| panic!("--parent-pid must be set if {RECOVERY_MECHANISM_ARG} is set"));

    // Block until the parent exits.  "Our parent process ID no longer matches the expected one"
    // is the same fact (we are reparented when it dies), re-checked after the event source is
    // registered so a parent that died before registration is never waited on.
    log::debug!("Waiting for parent with pid {parent_pid} to crash...");
    let parent_is_alive = || Pid::parent() == parent_pid;
    if !unix_wait::wait_for_pid_exit(parent_pid.as_raw(), parent_is_alive) {
        // No process-exit event source on this platform or setup failed: poll.  This is the
        // only remaining timer in this path, and runs only in the short-lived recovery process.
        log::debug!("No process-exit event source available; polling for parent death.");
        while parent_is_alive() {
            std::thread::sleep(instant::Duration::from_secs(1));
        }
    }
    log::info!("Parent has crashed; continuing execution.");
}

/// Blocking process-exit notification for Unix: a pidfd on Linux, `kqueue` `EVFILT_PROC` on
/// macOS/BSD.  Windows uses `WaitForSingleObject` on the parent handle instead.
#[cfg(unix)]
mod unix_wait {
    /// Blocks until process `pid` exits, using an OS exit event.
    ///
    /// `still_alive` is evaluated after the event source is registered (closing the race with a
    /// process that exited, and a pid that was reused, before registration); if it returns
    /// `false` the wait ends immediately.  Returns `false` only if no event source could be set
    /// up, in which case the caller must fall back to polling.
    pub(super) fn wait_for_pid_exit(pid: i32, still_alive: impl Fn() -> bool) -> bool {
        imp::wait_for_pid_exit(pid, still_alive)
    }

    #[cfg(any(target_os = "linux", target_os = "android"))]
    mod imp {
        pub(super) fn wait_for_pid_exit(pid: i32, still_alive: impl Fn() -> bool) -> bool {
            // SAFETY: plain syscall with integer arguments.
            let fd = unsafe {
                libc::syscall(libc::SYS_pidfd_open, pid as libc::c_long, 0 as libc::c_long)
            };
            if fd < 0 {
                // ESRCH: already gone, which is the event we were waiting for.  Anything else
                // (ENOSYS on kernels before 5.3, EPERM in a sandbox): no event source.
                return std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            }
            let fd = fd as libc::c_int;
            let mut result = true;
            if still_alive() {
                loop {
                    let mut pfd = libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    };
                    // SAFETY: `pfd` is a valid pollfd and nfds is 1.
                    let r = unsafe { libc::poll(&mut pfd, 1, -1) };
                    if r >= 0 {
                        break;
                    }
                    if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
                        result = false;
                        break;
                    }
                }
            }
            // SAFETY: `fd` is the pidfd opened above and is not used afterwards.
            unsafe { libc::close(fd) };
            result
        }
    }

    #[cfg(any(
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    ))]
    mod imp {
        pub(super) fn wait_for_pid_exit(pid: i32, still_alive: impl Fn() -> bool) -> bool {
            // SAFETY: kqueue takes no arguments.
            let kq = unsafe { libc::kqueue() };
            if kq < 0 {
                return false;
            }
            // SAFETY: an all-zero kevent is a valid value.
            let mut change: libc::kevent = unsafe { std::mem::zeroed() };
            change.ident = pid as libc::uintptr_t;
            change.filter = libc::EVFILT_PROC;
            change.flags = libc::EV_ADD | libc::EV_ONESHOT;
            change.fflags = libc::NOTE_EXIT;

            // Register first (no wait), so the liveness re-check below cannot miss an exit.
            // SAFETY: `change` is valid for one change; no event list is requested.
            let registered =
                unsafe { libc::kevent(kq, &change, 1, std::ptr::null_mut(), 0, std::ptr::null()) };
            let mut result = true;
            if registered < 0 {
                // ESRCH: the process is already gone, which is the event we wanted.
                result = std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
            } else if still_alive() {
                loop {
                    // SAFETY: an all-zero kevent is a valid output buffer.
                    let mut event: libc::kevent = unsafe { std::mem::zeroed() };
                    // SAFETY: one output slot, no timeout (block).
                    let r = unsafe {
                        libc::kevent(kq, std::ptr::null(), 0, &mut event, 1, std::ptr::null())
                    };
                    if r > 0 {
                        break;
                    }
                    if r < 0
                        && std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted
                    {
                        result = false;
                        break;
                    }
                }
            }
            // SAFETY: `kq` was opened above and is not used afterwards.
            unsafe { libc::close(kq) };
            result
        }
    }

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "macos",
        target_os = "ios",
        target_os = "freebsd",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "dragonfly"
    )))]
    mod imp {
        pub(super) fn wait_for_pid_exit(_pid: i32, _still_alive: impl Fn() -> bool) -> bool {
            false
        }
    }

    #[cfg(all(test, any(target_os = "linux", target_os = "macos")))]
    mod tests {
        use super::wait_for_pid_exit;
        use std::process::{Command, Stdio};

        /// The wait is driven by the process's exit event: when it returns, the process has
        /// exited (no sleep loop, no timing assumption).
        #[test]
        fn returns_when_the_process_exits() {
            let mut child = Command::new("sh")
                .args(["-c", "read _"])
                .stdin(Stdio::piped())
                .spawn()
                .expect("spawn sh");
            let pid = child.id() as i32;
            // Closing stdin makes `read` return and the shell exit.
            drop(child.stdin.take());
            assert!(wait_for_pid_exit(pid, || true));
            assert!(child.try_wait().expect("try_wait").is_some());
        }

        /// A process that is already gone (or a reused pid, reported by `still_alive`) must
        /// not block forever.
        #[test]
        fn returns_immediately_when_liveness_check_fails() {
            let mut child = Command::new("sh")
                .args(["-c", "read _"])
                .stdin(Stdio::piped())
                .spawn()
                .expect("spawn sh");
            let pid = child.id() as i32;
            // The child is still blocked in `read`; only the liveness check ends the wait.
            assert!(wait_for_pid_exit(pid, || false));
            drop(child.stdin.take());
            child.wait().expect("wait");
        }

        #[test]
        fn already_exited_process_is_reported_as_exited() {
            let mut child = Command::new("true").spawn().expect("spawn true");
            let pid = child.id() as i32;
            child.wait().expect("wait");
            // Reaped: the pid no longer exists, so registration reports ESRCH.
            assert!(wait_for_pid_exit(pid, || true));
        }
    }
}

/// Handles a crash in the parent process by using the given recovery mechanism.
///
/// Returns whether or not the user should be notified about the crash.
fn handle_parent_crash(
    recovery_mechanism: RecoveryMechanism,
    user_preferences: &dyn UserPreferences,
) -> bool {
    warp_logging::on_parent_process_crash();

    match recovery_mechanism {
        #[cfg(target_os = "linux")]
        RecoveryMechanism::X11 => {
            let force_x11 = settings::ForceX11::read_from_preferences(user_preferences);
            if force_x11 != Some(true) {
                report_if_error!(settings::ForceX11::write_to_preferences(
                    &true,
                    user_preferences,
                ));
            }

            true
        }
        RecoveryMechanism::DedicatedGpu => {
            let prefer_low_power_gpu =
                settings::PreferLowPowerGPU::read_from_preferences(user_preferences);

            // If the user hasn't explicitly set a GPU preference, set
            // the preference to dedicated GPU for them.
            if prefer_low_power_gpu.is_none() {
                report_if_error!(settings::PreferLowPowerGPU::write_to_preferences(
                    &false,
                    user_preferences
                ));
            }

            // We're not showing anything to the user when we
            // recover from a crash by switching from preferring
            // integrated to dedicated gpu due to the fact that
            // this recovery mechanism is only used when the user
            // has not explicitly set their preference.
            false
        }
        RecoveryMechanism::DisableOpenGL | RecoveryMechanism::ForceVulkan => {
            // This has already been handled for us, due to the parent process
            // overriding the `WGPU_BACKEND` environment variable, so there's
            // nothing to do here.

            // We don't show anything to the user, as the crash occurs before
            // there is any visible window, so they won't even know that a
            // crash occurred.  Any information we give them would be pretty
            // unactionable.
            false
        }
    }
}
