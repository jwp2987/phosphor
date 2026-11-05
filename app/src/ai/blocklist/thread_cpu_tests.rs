use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{ThreadCpuProbe, current_thread_cpu};

/// Burns CPU on the calling thread until `cpu_now` reports `target` more than it did at the
/// start. Panics if that takes more than 30 s of wall time (the clock is not counting work).
fn spin_until(target: Duration, cpu_now: impl Fn() -> Duration) {
    let start = cpu_now();
    let wall = Instant::now();
    let mut x = 1u64;
    while cpu_now().saturating_sub(start) < target {
        for i in 0..10_000u64 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
        }
        std::hint::black_box(x);
        assert!(
            wall.elapsed() < Duration::from_secs(30),
            "{target:?} of busy work never registered on the thread CPU clock"
        );
    }
}

/// Upper bound for the CPU a thread is charged over a sleep of 200 ms: a little bookkeeping,
/// plus one scheduler tick on Windows (~15.6 ms), far below the sleep itself.
const SLEEP_CHARGE_LIMIT: Duration = Duration::from_millis(40);

#[test]
fn busy_work_counts_on_the_thread_clock() {
    let Some(start) = current_thread_cpu() else {
        // No thread CPU clock on this platform: callers fall back to wall time.
        return;
    };
    spin_until(Duration::from_millis(40), || {
        current_thread_cpu().expect("the clock worked a moment ago")
    });
    let spent = current_thread_cpu().unwrap().saturating_sub(start);
    assert!(spent >= Duration::from_millis(40), "{spent:?}");
}

#[test]
fn sleeping_does_not_count_on_the_thread_clock() {
    let Some(start) = current_thread_cpu() else {
        return;
    };
    let wall = Instant::now();
    std::thread::sleep(Duration::from_millis(200));
    assert!(wall.elapsed() >= Duration::from_millis(200));
    let spent = current_thread_cpu().unwrap().saturating_sub(start);
    assert!(
        spent < SLEEP_CHARGE_LIMIT,
        "200 ms asleep was charged {spent:?} of CPU"
    );
}

/// A probe taken on a worker thread reads that worker's clock from this thread: its busy work
/// shows, and this thread's own work does not.
#[test]
fn probe_reads_another_threads_busy_work() {
    let (probe_tx, probe_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        let probe = ThreadCpuProbe::current();
        probe_tx.send(probe).unwrap();
        if let Some(probe) = probe {
            spin_until(Duration::from_millis(60), || {
                probe.read().expect("the probe reads its own thread")
            });
        }
        done_tx.send(()).unwrap();
        // Stay alive while the probe is read.
        let _ = exit_rx.recv();
    });
    let probe = probe_rx.recv().unwrap();
    done_rx.recv().unwrap();
    if let Some(probe) = probe {
        let spent = probe.read().expect("the worker is alive");
        assert!(
            spent >= Duration::from_millis(60),
            "the worker's busy work: {spent:?}"
        );
        let before = probe.read().unwrap();
        // This thread works; the (blocked) worker's clock must not move with it.
        if current_thread_cpu().is_some() {
            spin_until(Duration::from_millis(40), || current_thread_cpu().unwrap());
        }
        let after = probe.read().unwrap();
        assert!(
            after.saturating_sub(before) < SLEEP_CHARGE_LIMIT,
            "a blocked worker was charged {:?} for another thread's work",
            after.saturating_sub(before)
        );
    }
    exit_tx.send(()).unwrap();
    worker.join().unwrap();
}

/// A probe of a sleeping thread barely moves.
#[test]
fn probe_of_a_sleeping_thread_barely_moves() {
    let (probe_tx, probe_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel::<()>();
    let worker = std::thread::spawn(move || {
        probe_tx.send(ThreadCpuProbe::current()).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let _ = exit_rx.recv();
    });
    if let Some(probe) = probe_rx.recv().unwrap() {
        let start = probe.read().expect("the worker is alive");
        std::thread::sleep(Duration::from_millis(250));
        let spent = probe
            .read()
            .expect("the worker is alive")
            .saturating_sub(start);
        assert!(
            spent < SLEEP_CHARGE_LIMIT,
            "a sleeping thread was charged {spent:?}"
        );
    }
    exit_tx.send(()).unwrap();
    worker.join().unwrap();
}
