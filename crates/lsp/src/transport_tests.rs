use std::time::{Duration, Instant};

use command::r#async::Command;

use super::*;

#[test]
fn sigterm_wait_returns_when_the_child_exits_not_after_the_grace_period() {
    warpui_core::r#async::block_on(async {
        let mut child = Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn sleep");
        let grace = Duration::from_secs(20);
        let start = Instant::now();
        let exited = terminate_and_reap(&mut child, grace).await;
        assert!(exited, "sleep dies on SIGTERM");
        // The old code slept the full grace period unconditionally.
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            start.elapsed()
        );
    });
}

#[test]
fn sigterm_wait_gives_up_for_a_child_that_ignores_sigterm() {
    warpui_core::r#async::block_on(async {
        let mut child = Command::new("sh")
            .args(["-c", "trap '' TERM; while :; do sleep 1; done"])
            .spawn()
            .expect("spawn sh");
        // Let the shell install its trap before signalling it.
        std::thread::sleep(Duration::from_millis(300));
        let exited = terminate_and_reap(&mut child, Duration::from_millis(200)).await;
        assert!(!exited);
        let _ = child.kill();
    });
}
