use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::JobObjects::{AssignProcessToJobObject, IsProcessInJob};
use windows::Win32::System::Threading::WaitForSingleObject;

use super::create_kill_on_close_job;

/// Closing the last handle to the job must terminate its members; this is what
/// reaps shells when the app is force-killed.
#[test]
fn closing_job_handle_kills_member_process() {
    let mut child = std::process::Command::new("cmd.exe")
        .args(["/c", "ping -n 60 127.0.0.1 >nul"])
        .spawn()
        .expect("spawn child");
    let process = windows::Win32::Foundation::HANDLE(
        std::os::windows::io::AsRawHandle::as_raw_handle(&child) as _,
    );
    let job = create_kill_on_close_job().expect("create job");
    unsafe { AssignProcessToJobObject(job, process) }.expect("assign");
    let mut in_job = windows::core::BOOL::default();
    unsafe { IsProcessInJob(process, Some(job), &mut in_job) }.expect("query");
    assert!(in_job.as_bool());

    unsafe { CloseHandle(job) }.expect("close job");
    let wait = unsafe { WaitForSingleObject(process, 5000) };
    assert_eq!(wait, WAIT_OBJECT_0, "child should die when job closes");
    let _ = child.wait();
}
