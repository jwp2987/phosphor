use std::ffi::OsStr;
use std::os::windows::process::CommandExt as _;

use anyhow::Result;

#[derive(Debug, thiserror::Error)]
pub enum JobObjectError {
    #[error("Failed to create job: {0}")]
    CreateFailed(std::io::Error),

    #[error("Failed to assign process to job: {0}")]
    AssignFailed(std::io::Error),

    #[error("Failed to set info for job: {0}")]
    SetInfoFailed(std::io::Error),

    #[error("Failed to get info for job: {0}")]
    GetInfoFailed(std::io::Error),

    #[error(transparent)]
    Other(anyhow::Error),
}

impl From<win32job::JobError> for JobObjectError {
    fn from(error: win32job::JobError) -> Self {
        match error {
            win32job::JobError::CreateFailed(e) => JobObjectError::CreateFailed(e),
            win32job::JobError::AssignFailed(e) => JobObjectError::AssignFailed(e),
            win32job::JobError::SetInfoFailed(e) => JobObjectError::SetInfoFailed(e),
            win32job::JobError::GetInfoFailed(e) => JobObjectError::GetInfoFailed(e),
            _ => JobObjectError::Other(error.into()),
        }
    }
}

/// We use Job Objects to handle killing child processes when the program is
/// closed. This builder struct is used to configure a Job Object and associate it
/// with processes. Processes associated with a job will be killed when the handle
/// to the job is dropped at the end of the program's lifecycle.
///
/// NOTE: We've encountered issues with assigning some processes to jobs that
/// already contain other processes (i.e. `pwsh.exe`), so we only want to
/// assign a single process to a job.
///
/// For more information on Job Objects, see:
/// https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects
#[derive(Debug, Default)]
pub struct JobObject {
    assign_current_process: bool,
    assign_process: Option<isize>,
    kill_children_on_close: bool,
}

impl JobObject {
    pub fn new() -> Self {
        Self::default()
    }

    /// Assigns the current process to the Job Object. This can be used to ensure
    /// that children of the current process are associated with the job.
    pub fn assign_current_process(mut self) -> Self {
        self.assign_current_process = true;
        self
    }

    /// Assigns a process to the Job Object. This process will be killed when the
    /// current process is closed.
    pub fn assign_process(mut self, process: isize) -> Self {
        self.assign_process = Some(process);
        self
    }

    /// Configures the Job Object so children of the assigned processes are
    /// automatically associated with the job, thus killing them along with
    /// their parents on close.
    pub fn kill_children_on_close(mut self) -> Self {
        self.kill_children_on_close = true;
        self
    }

    fn create_internal(self) -> Result<(), win32job::JobError> {
        let job = win32job::Job::create()?;

        let mut info = job.query_extended_limit_info()?;
        // Mark the job as "kill on job close", so all processes associated with
        // the job are killed when the handle to the job is closed.
        info.limit_kill_on_job_close();
        info.limit_breakaway_ok();
        if !self.kill_children_on_close {
            info.limit_silent_breakaway_ok();
        }
        job.set_extended_limit_info(&info)?;

        if self.assign_current_process {
            job.assign_current_process()?;
        }
        if let Some(process) = self.assign_process {
            job.assign_process(process)?;
        }

        Box::leak(Box::new(job));
        Ok(())
    }

    /// Creates a new Job Object and assigns any specified processes to it. The
    /// handle to the job is leaked to ensure that the job lives for the lifetime
    /// of the program.
    pub fn create(self) -> Result<(), JobObjectError> {
        self.create_internal().map_err(Into::into)
    }
}

/// A Job Object holding a single spawned process (and anything it spawns afterwards) that
/// kills all of them when it is dropped. This is the Windows stand-in for SIGKILLing a unix
/// process group: Windows has no process groups, so a cancelled generator's descendants
/// would otherwise outlive the command that spawned them.
///
/// Unlike [`JobObject`], the handle is NOT leaked; it is owned by the caller. Call
/// [`KillOnCloseJob::disarm`] when the command finished normally so that deliberately
/// detached descendants (e.g. a git fsmonitor daemon) are left alone, as on unix.
///
/// Children are spawned with `CREATE_BREAKAWAY_FROM_JOB` (see `async.rs`), so they are not
/// in any job when we assign them here; a process that is already in another job is the
/// only case in which assignment fails, and the caller treats that as best-effort.
pub struct KillOnCloseJob {
    job: win32job::Job,
}

impl std::fmt::Debug for KillOnCloseJob {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KillOnCloseJob").finish_non_exhaustive()
    }
}

// SAFETY: the wrapped value is a Job Object HANDLE. Win32 kernel handles may be used and
// closed from any thread; `win32job::Job` only lacks the auto traits because it stores a
// raw `HANDLE`.
unsafe impl Send for KillOnCloseJob {}
unsafe impl Sync for KillOnCloseJob {}

impl KillOnCloseJob {
    /// Creates a kill-on-close job and assigns the process with the given raw handle to it.
    pub fn with_process(process: isize) -> Result<Self, JobObjectError> {
        let job = win32job::Job::create()?;
        let mut info = job.query_extended_limit_info()?;
        info.limit_kill_on_job_close();
        job.set_extended_limit_info(&info)?;
        job.assign_process(process)?;
        Ok(Self { job })
    }

    /// Clears the kill-on-close limit and closes the job without killing anything.
    pub fn disarm(self) {
        let info = win32job::ExtendedLimitInfo::new();
        if let Err(e) = self.job.set_extended_limit_info(&info) {
            // Dropping now would kill the (finished) command's remaining descendants.
            // Leaking one handle in this rare failure beats killing a deliberate daemon.
            log::warn!("Failed to disarm job object: {e}");
            std::mem::forget(self);
        }
    }
}

pub fn init() {
    if let Err(e) = JobObject::new()
        .kill_children_on_close()
        .assign_current_process()
        .create()
    {
        log::error!("Failed to create job object for the program: {e:#}");
    }
}

pub trait CommandExt {
    /// Append literal text to the command line without any quoting or escaping.
    ///
    /// This is useful for passing arguments to `cmd.exe /c`, which doesn't follow
    /// `CommandLineToArgvW` escaping rules.
    fn raw_arg<S: AsRef<OsStr>>(&mut self, text_to_append_as_is: S) -> &mut Self;
}

use async_process::windows::CommandExt as _;

impl CommandExt for crate::blocking::Command {
    fn raw_arg<S: AsRef<OsStr>>(&mut self, text_to_append_as_is: S) -> &mut Self {
        self.inner.raw_arg(text_to_append_as_is);
        self
    }
}

impl CommandExt for crate::r#async::Command {
    fn raw_arg<S: AsRef<OsStr>>(&mut self, text_to_append_as_is: S) -> &mut Self {
        self.inner.raw_arg(text_to_append_as_is);
        self
    }
}

#[cfg(test)]
#[path = "windows_tests.rs"]
mod tests;
