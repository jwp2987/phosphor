use std::os::windows::io::AsRawHandle as _;
use std::process::Stdio;
use std::time::{Duration, Instant};

use super::KillOnCloseJob;

fn spawn_sleeper() -> std::process::Child {
    // `ping -n 60` waits roughly a minute without needing a console.
    std::process::Command::new("ping")
        .args(["-n", "60", "127.0.0.1"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ping")
}

fn wait_for_exit(child: &mut std::process::Child, within: Duration) -> bool {
    let deadline = Instant::now() + within;
    while Instant::now() < deadline {
        if child.try_wait().expect("try_wait").is_some() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

#[test]
fn dropping_the_job_kills_the_process() {
    let mut child = spawn_sleeper();
    let job = KillOnCloseJob::with_process(child.as_raw_handle() as isize).expect("assign to job");
    assert!(!wait_for_exit(&mut child, Duration::from_millis(300)));
    drop(job);
    assert!(wait_for_exit(&mut child, Duration::from_secs(5)));
}

#[test]
fn disarming_the_job_leaves_the_process_running() {
    let mut child = spawn_sleeper();
    let job = KillOnCloseJob::with_process(child.as_raw_handle() as isize).expect("assign to job");
    job.disarm();
    assert!(!wait_for_exit(&mut child, Duration::from_millis(500)));
    child.kill().expect("clean up");
}
