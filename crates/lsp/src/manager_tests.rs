use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;

// jwp2987/phosphor#680: quitting waits for language servers to shut down, but
// that wait must be bounded so a wedged server cannot hang the app's exit.

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shutdown_wait_returns_as_soon_as_every_server_is_done() {
    let (tx, rx) = mpsc::channel();
    tx.send(()).unwrap();
    tx.send(()).unwrap();

    let start = Instant::now();
    let finished = wait_for_shutdowns(&rx, 2, Duration::from_secs(30));

    assert_eq!(finished, 2);
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "a completed shutdown must not wait out the grace period"
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shutdown_wait_is_bounded_by_the_grace_period_when_a_server_hangs() {
    let (tx, rx) = mpsc::channel();
    // One server finishes; the other never does. `tx` stays alive for the
    // whole wait, exactly like a shutdown future stuck on an unresponsive server.
    tx.send(()).unwrap();

    let grace = Duration::from_millis(200);
    let start = Instant::now();
    let finished = wait_for_shutdowns(&rx, 2, grace);
    let elapsed = start.elapsed();

    assert_eq!(finished, 1);
    assert!(
        elapsed >= grace,
        "gave up before the grace period: {elapsed:?}"
    );
    assert!(
        elapsed < grace + Duration::from_secs(5),
        "wait overran its bound: {elapsed:?}"
    );
    drop(tx);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shutdown_wait_stops_early_when_every_sender_is_gone() {
    let (tx, rx) = mpsc::channel::<()>();
    drop(tx);

    let start = Instant::now();
    assert_eq!(wait_for_shutdowns(&rx, 3, Duration::from_secs(30)), 0);
    assert!(start.elapsed() < Duration::from_secs(5));
}

#[test]
fn terminate_for_app_exit_with_no_servers_returns_immediately_and_is_recorded() {
    warpui_core::App::test((), |mut app| async move {
        let manager = app.add_singleton_model(|_| LspManagerModel::new());
        assert!(!manager.read(&app, |manager, _| manager.terminated_for_app_exit()));

        let finished = manager.update(&mut app, |manager, ctx| {
            manager.terminate_for_app_exit(Duration::from_secs(30), ctx)
        });

        assert_eq!(finished, 0);
        assert!(manager.read(&app, |manager, _| manager.terminated_for_app_exit()));
    });
}
