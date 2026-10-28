//! A per-app Job object that kills hosted shells when the app dies (#1107).
//!
//! When Phosphor is force-killed, ConPTY's OpenConsole exits but the hosted
//! shell (e.g. Git Bash's `bash.exe`) survives as an orphan. We create one job
//! with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` and put every shell in it; the
//! kernel closes our job handle when the process dies, however it dies, and
//! terminates the members.
//!
//! `JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK` is also set, so processes *spawned
//! by* a shell are not added to the job. A GUI app the user launches from the
//! shell (`notepad`, `Start-Process`, `start`) therefore keeps running after
//! Phosphor quits. This is the same trade-off Windows Terminal makes: the
//! shell dies with the terminal, deliberately detached children do not.
//! (Plain foreground children of a shell are not killed by the job either; they
//! get the usual console-closed signal.)
//!
//! Nested jobs (Win8+) mean this works even if Phosphor itself runs in a job.

use std::sync::OnceLock;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectExtendedLimitInformation, SetInformationJobObject,
};

/// Wrapper so the raw handle can live in a static. The handle is intentionally
/// never closed: it must live exactly as long as the process.
struct JobHandle(HANDLE);
// SAFETY: a job handle is just a kernel object reference, usable from any thread.
unsafe impl Send for JobHandle {}
unsafe impl Sync for JobHandle {}

static APP_JOB: OnceLock<Option<JobHandle>> = OnceLock::new();

pub(super) fn create_kill_on_close_job() -> windows::core::Result<HANDLE> {
    unsafe {
        let job = CreateJobObjectW(None, windows::core::PCWSTR::null())?;
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE | JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )?;
        Ok(job)
    }
}

/// Adds `process` to the app-wide job, creating it on first use. Failures are
/// logged and otherwise ignored.
pub(super) fn assign_to_app_job(process: HANDLE) {
    let job = APP_JOB.get_or_init(|| match create_kill_on_close_job() {
        Ok(job) => Some(JobHandle(job)),
        Err(err) => {
            log::warn!("Failed to create the shell job object: {err:#}");
            None
        }
    });
    if let Some(job) = job {
        if let Err(err) = unsafe { AssignProcessToJobObject(job.0, process) } {
            log::warn!("Failed to assign shell process to job object: {err:#}");
        }
    }
}

#[cfg(test)]
#[path = "job_tests.rs"]
mod tests;
