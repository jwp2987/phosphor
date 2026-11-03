//! The in-memory channel of the PowerShell one-operation insert (#1581).
//!
//! A single-line command must reach PSReadLine as one `Insert`, not ~80 synthesized key presses,
//! but commands can contain secrets, so the text never touches the disk. Phosphor instead serves
//! it on a Windows NAMED PIPE and sends only the chord `ESC 3`; the bootstrap's Alt+3 handler
//! connects, reads the bytes and inserts them.
//!
//! * Naming: each PowerShell session gets one random 128-bit name, `phosphor-cmd-<32 hex>` (full
//!   path `\\.\pipe\phosphor-cmd-<hex>`), from the OS CSPRNG. It is not derived from the session
//!   id. It reaches the shell once, in the init script (`$global:_warpCmdPipe`).
//! * Access: the pipe's DACL grants only the current user's SID (protected, no inheritance), so
//!   neither Everyone nor Authenticated Users nor other local users can connect.
//!   `PIPE_REJECT_REMOTE_CLIENTS` keeps SMB clients out.
//! * Lifetime: one pipe instance per command (`FILE_FLAG_FIRST_PIPE_INSTANCE`, max instances 1, so
//!   a squatter that created the name first makes creation fail and the command is typed). It is
//!   created before the chord is written, serves exactly one client on a background thread and is
//!   gone after the bytes are delivered. [`CommandPipe`] cancels and closes it on drop.
//! * Non-Windows builds have no pipe: [`CommandPipe::serve`] returns `None` (tests substitute an
//!   in-memory stand-in so the controller logic can be exercised anywhere).

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use rand::RngCore as _;
use rand::rngs::OsRng;

/// Prefix of every pipe name; the 32 hex digits of randomness follow.
pub(crate) const PIPE_NAME_PREFIX: &str = "phosphor-cmd-";

/// A fresh unguessable pipe name: the prefix and 128 random bits as lower-case hex.
pub(crate) fn generate_pipe_name() -> String {
    let mut bytes = [0u8; 16];
    OsRng.fill_bytes(&mut bytes);
    let mut name = String::with_capacity(PIPE_NAME_PREFIX.len() + 32);
    name.push_str(PIPE_NAME_PREFIX);
    for byte in bytes {
        name.push_str(&format!("{byte:02x}"));
    }
    name
}

static PIPE_NAMES: LazyLock<Mutex<HashMap<u64, String>>> = LazyLock::new(Default::default);

/// The session's pipe name, minted on first use. Called while the init script is built, so the
/// shell and the controller agree on it.
pub(crate) fn pipe_name_for_session(session_id: u64) -> String {
    let mut names = PIPE_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    names
        .entry(session_id)
        .or_insert_with(generate_pipe_name)
        .clone()
}

/// The name minted for `session_id`, if any (only local PowerShell sessions on Windows have one).
pub(crate) fn lookup_pipe_name(session_id: u64) -> Option<String> {
    let names = PIPE_NAMES.lock().unwrap_or_else(|e| e.into_inner());
    names.get(&session_id).cloned()
}

/// What the init script gets for `session_id`: the name on Windows, an empty string elsewhere
/// (the chord is never sent there).
pub(crate) fn init_script_value(session_id: u64) -> String {
    if cfg!(windows) {
        pipe_name_for_session(session_id)
    } else {
        String::new()
    }
}

/// A served command: the pipe exists (so the client cannot connect too early) and a background
/// thread delivers the bytes to the first client. Dropping it cancels a delivery still waiting.
pub(crate) struct CommandPipe {
    #[cfg(windows)]
    #[allow(dead_code)] // held for its Drop
    inner: windows_impl::Server,
    #[cfg(not(windows))]
    #[allow(dead_code)]
    inner: (),
}

impl CommandPipe {
    /// Creates the pipe `name` and starts serving `command` (UTF-8, no BOM) on it. `None` if the
    /// pipe cannot be created; the caller then types the command.
    pub(crate) fn serve(name: &str, command: &str) -> Option<Self> {
        #[cfg(windows)]
        {
            windows_impl::Server::start(name, command.as_bytes().to_vec())
                .map(|inner| Self { inner })
        }
        #[cfg(all(not(windows), test))]
        {
            let _ = (name, command);
            Some(Self { inner: () })
        }
        #[cfg(all(not(windows), not(test)))]
        {
            let _ = (name, command);
            None
        }
    }
}

#[cfg(windows)]
pub(crate) mod windows_impl {
    //! Raw kernel32/advapi32 declarations: a handful of calls with stable signatures, so no
    //! feature list of the `windows` crate has to be kept in step with them.

    use std::ffi::c_void;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, LazyLock};
    use std::thread::JoinHandle;

    type Handle = isize;
    const INVALID_HANDLE_VALUE: Handle = -1;

    const PIPE_ACCESS_OUTBOUND: u32 = 0x2;
    const FILE_FLAG_FIRST_PIPE_INSTANCE: u32 = 0x0008_0000;
    const FILE_FLAG_OVERLAPPED: u32 = 0x4000_0000;
    const PIPE_TYPE_BYTE: u32 = 0;
    const PIPE_READMODE_BYTE: u32 = 0;
    const PIPE_WAIT: u32 = 0;
    const PIPE_REJECT_REMOTE_CLIENTS: u32 = 0x8;
    const TOKEN_QUERY: u32 = 0x8;
    const TOKEN_USER_CLASS: u32 = 1;
    const SDDL_REVISION_1: u32 = 1;
    const ERROR_IO_PENDING: u32 = 997;
    const ERROR_PIPE_CONNECTED: u32 = 535;
    const WAIT_OBJECT_0: u32 = 0;
    /// A pipe nobody connects to (the chord was swallowed by a program reading keys) is closed
    /// after this long.
    const CONNECT_DEADLINE_MS: u32 = 30_000;
    const BUFFER_SIZE: u32 = 4096;

    #[repr(C)]
    struct SecurityAttributes {
        length: u32,
        security_descriptor: *mut c_void,
        inherit_handle: i32,
    }

    #[repr(C)]
    struct Overlapped {
        internal: usize,
        internal_high: usize,
        offset: u32,
        offset_high: u32,
        event: Handle,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateNamedPipeW(
            name: *const u16,
            open_mode: u32,
            pipe_mode: u32,
            max_instances: u32,
            out_buffer: u32,
            in_buffer: u32,
            default_timeout: u32,
            attributes: *const SecurityAttributes,
        ) -> Handle;
        fn ConnectNamedPipe(pipe: Handle, overlapped: *mut Overlapped) -> i32;
        fn WriteFile(
            file: Handle,
            buffer: *const u8,
            len: u32,
            written: *mut u32,
            overlapped: *mut Overlapped,
        ) -> i32;
        fn FlushFileBuffers(file: Handle) -> i32;
        fn CloseHandle(handle: Handle) -> i32;
        fn CreateEventW(
            attributes: *const c_void,
            manual_reset: i32,
            initial_state: i32,
            name: *const u16,
        ) -> Handle;
        fn SetEvent(event: Handle) -> i32;
        fn WaitForMultipleObjects(
            count: u32,
            handles: *const Handle,
            wait_all: i32,
            milliseconds: u32,
        ) -> u32;
        fn GetOverlappedResult(
            file: Handle,
            overlapped: *mut Overlapped,
            transferred: *mut u32,
            wait: i32,
        ) -> i32;
        fn CancelIoEx(file: Handle, overlapped: *mut Overlapped) -> i32;
        fn GetLastError() -> u32;
        fn GetCurrentProcess() -> Handle;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
        fn GetTokenInformation(
            token: Handle,
            class: u32,
            info: *mut c_void,
            len: u32,
            returned: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *mut c_void, string: *mut *mut u16) -> i32;
        fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl: *const u16,
            revision: u32,
            descriptor: *mut *mut c_void,
            size: *mut u32,
        ) -> i32;
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// The SDDL of the pipe: a protected DACL with one allow entry, for the current user's SID.
    static PIPE_SDDL: LazyLock<Option<String>> = LazyLock::new(|| unsafe {
        let mut token: Handle = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut needed = 0u32;
        GetTokenInformation(
            token,
            TOKEN_USER_CLASS,
            std::ptr::null_mut(),
            0,
            &mut needed,
        );
        let mut sddl = None;
        if needed > 0 {
            // u64 elements keep the TOKEN_USER buffer pointer-aligned.
            let mut buffer = vec![0u64; (needed as usize).div_ceil(8)];
            if GetTokenInformation(
                token,
                TOKEN_USER_CLASS,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            ) != 0
            {
                // TOKEN_USER starts with SID_AND_ATTRIBUTES, whose first field is the SID pointer.
                let sid = *(buffer.as_ptr() as *const *mut c_void);
                let mut string: *mut u16 = std::ptr::null_mut();
                if ConvertSidToStringSidW(sid, &mut string) != 0 && !string.is_null() {
                    let mut len = 0;
                    while *string.add(len) != 0 {
                        len += 1;
                    }
                    let sid_text =
                        String::from_utf16_lossy(std::slice::from_raw_parts(string, len));
                    LocalFree(string.cast());
                    sddl = Some(format!("D:P(A;;GA;;;{sid_text})"));
                }
            }
        }
        CloseHandle(token);
        sddl
    });

    /// An event handle closed when the last reference goes.
    struct Event(Handle);
    // SAFETY: a kernel event handle may be used and closed from any thread.
    unsafe impl Send for Event {}
    unsafe impl Sync for Event {}
    impl Drop for Event {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    /// The pipe handle, owned by the serving thread.
    struct Pipe(Handle);
    // SAFETY: as above.
    unsafe impl Send for Pipe {}

    /// Creates one pipe instance: outbound byte pipe, overlapped, this user only, local clients
    /// only, first and only instance of the name.
    fn create_pipe(name: &str) -> Option<Pipe> {
        let sddl = wide(PIPE_SDDL.as_deref()?);
        let full_name = wide(&format!(r"\\.\pipe\{name}"));
        unsafe {
            let mut descriptor: *mut c_void = std::ptr::null_mut();
            if ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                std::ptr::null_mut(),
            ) == 0
            {
                return None;
            }
            let attributes = SecurityAttributes {
                length: std::mem::size_of::<SecurityAttributes>() as u32,
                security_descriptor: descriptor,
                inherit_handle: 0,
            };
            let handle = CreateNamedPipeW(
                full_name.as_ptr(),
                PIPE_ACCESS_OUTBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE | FILE_FLAG_OVERLAPPED,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                BUFFER_SIZE,
                0,
                0,
                &attributes,
            );
            LocalFree(descriptor);
            if handle == INVALID_HANDLE_VALUE || handle == 0 {
                log::warn!(
                    "Could not create the PowerShell command pipe (error {})",
                    GetLastError()
                );
                return None;
            }
            Some(Pipe(handle))
        }
    }

    /// Waits for `overlapped` on `pipe`, or for `cancel`. `true` when the operation completed.
    unsafe fn wait_or_cancel(pipe: Handle, overlapped: &mut Overlapped, cancel: Handle) -> bool {
        unsafe {
            let handles = [overlapped.event, cancel];
            let woken = WaitForMultipleObjects(2, handles.as_ptr(), 0, CONNECT_DEADLINE_MS);
            let mut transferred = 0u32;
            if woken == WAIT_OBJECT_0 {
                return GetOverlappedResult(pipe, overlapped, &mut transferred, 1) != 0;
            }
            // Cancelled or timed out: the operation must be finished before `overlapped` dies.
            CancelIoEx(pipe, overlapped);
            GetOverlappedResult(pipe, overlapped, &mut transferred, 1);
            false
        }
    }

    fn deliver(pipe: &Pipe, cancel: &Event, payload: &[u8], delivering: &AtomicBool) {
        unsafe {
            let event = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
            if event == 0 {
                return;
            }
            let event = Event(event);
            let mut overlapped = Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                event: event.0,
            };
            let connected = if ConnectNamedPipe(pipe.0, &mut overlapped) != 0 {
                true
            } else {
                match GetLastError() {
                    ERROR_PIPE_CONNECTED => true,
                    ERROR_IO_PENDING => wait_or_cancel(pipe.0, &mut overlapped, cancel.0),
                    _ => false,
                }
            };
            if !connected {
                return;
            }
            // A client is in: from here the instance is spoken for, and a drop must not wait for it.
            delivering.store(true, Ordering::SeqCst);
            let mut write = Overlapped {
                internal: 0,
                internal_high: 0,
                offset: 0,
                offset_high: 0,
                event: event.0,
            };
            let mut written = 0u32;
            let started = WriteFile(
                pipe.0,
                payload.as_ptr(),
                payload.len() as u32,
                &mut written,
                &mut write,
            );
            if started == 0 {
                if GetLastError() != ERROR_IO_PENDING {
                    return;
                }
                if GetOverlappedResult(pipe.0, &mut write, &mut written, 1) == 0 {
                    return;
                }
            }
            // Returns once the client has read everything (or hung up).
            FlushFileBuffers(pipe.0);
        }
    }

    /// The running server: cancelling and joining (while nobody has connected) on drop.
    pub(crate) struct Server {
        cancel: Arc<Event>,
        delivering: Arc<AtomicBool>,
        thread: Option<JoinHandle<()>>,
    }

    impl Server {
        pub(super) fn start(name: &str, payload: Vec<u8>) -> Option<Self> {
            let pipe = create_pipe(name)?;
            let cancel = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
            if cancel == 0 {
                unsafe { CloseHandle(pipe.0) };
                return None;
            }
            let cancel = Arc::new(Event(cancel));
            let delivering = Arc::new(AtomicBool::new(false));
            let thread = std::thread::Builder::new()
                .name("phosphor-cmd-pipe".to_owned())
                .spawn({
                    let cancel = cancel.clone();
                    let delivering = delivering.clone();
                    move || {
                        deliver(&pipe, &cancel, &payload, &delivering);
                        unsafe { CloseHandle(pipe.0) };
                    }
                })
                .ok()?;
            Some(Self {
                cancel,
                delivering,
                thread: Some(thread),
            })
        }
    }

    impl Drop for Server {
        fn drop(&mut self) {
            unsafe { SetEvent(self.cancel.0) };
            // Waiting for the close keeps the name free for the next command's first-instance
            // create. Once a client is connected the flush may wait on that client, so detach.
            if !self.delivering.load(Ordering::SeqCst) {
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "command_pipe_tests.rs"]
mod tests;
