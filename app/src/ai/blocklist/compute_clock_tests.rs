use std::future::Future;
use std::sync::mpsc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

use super::ComputeClock;
use crate::ai::blocklist::thread_cpu::{ThreadCpuProbe, current_thread_cpu};

#[test]
fn only_running_stages_count_as_compute() {
    let clock = ComputeClock::default();
    assert_eq!(clock.spent(), Duration::ZERO);
    assert!(!clock.is_running());

    // Time passing with no stage running (queued behind a stalled main thread) is not compute.
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(clock.spent(), Duration::ZERO);

    let (clock, guard) = ComputeClock::running_for_test(Duration::from_millis(30));
    assert!(clock.is_running());
    assert!(clock.spent() >= Duration::from_millis(30));
    drop(guard);
    assert!(!clock.is_running());
    let booked = clock.spent();
    assert!(booked >= Duration::from_millis(30));
    std::thread::sleep(Duration::from_millis(5));
    assert_eq!(clock.spent(), booked, "an idle clock does not advance");
}

#[test]
fn stage_start_is_announced() {
    let clock = ComputeClock::default();
    let (tx, rx) = async_channel::unbounded();
    clock.notify_on_stage_start(tx);
    let first = clock.begin();
    assert!(
        rx.try_recv().is_ok(),
        "beginning a stage notifies the waiter"
    );
    drop(first);
    clock.stop_notifying();
    let _second = clock.begin();
    assert!(rx.try_recv().is_err(), "a stopped watch is not notified");
}

/// Burns CPU on the calling thread until its CPU clock moved by `target` (30 s wall at most).
fn spin_cpu(target: Duration) {
    let start = current_thread_cpu().expect("callers check the clock exists");
    let wall = Instant::now();
    let mut x = 1u64;
    while current_thread_cpu().unwrap().saturating_sub(start) < target {
        for i in 0..10_000u64 {
            x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
        }
        std::hint::black_box(x);
        assert!(
            wall.elapsed() < Duration::from_secs(30),
            "the CPU clock never moved"
        );
    }
}

/// CPU a thread may be charged over a wait of a few hundred ms: bookkeeping, plus a scheduler
/// tick on Windows (~15.6 ms). Far below the wait itself.
const WAIT_CHARGE_LIMIT: Duration = Duration::from_millis(40);

/// #1651: on a thread CPU clock, a stage's busy work counts; its thread being off the CPU (here
/// blocked in a sleep, as a descheduled thread on a loaded machine is) does not.
#[test]
fn thread_cpu_clock_counts_busy_work_not_time_off_cpu() {
    if current_thread_cpu().is_none() {
        // No thread CPU clock here: the clock measures polls on wall time (logged once).
        return;
    }
    let clock = ComputeClock::thread_cpu();
    futures::executor::block_on(clock.measure(async {
        std::thread::sleep(Duration::from_millis(200));
    }));
    assert!(!clock.is_running(), "the stage ended with its future");
    let asleep = clock.spent();
    assert!(
        asleep < WAIT_CHARGE_LIMIT,
        "200 ms off the CPU counted {asleep:?}"
    );

    futures::executor::block_on(clock.measure(async {
        spin_cpu(Duration::from_millis(40));
    }));
    let spent = clock.spent();
    assert!(
        spent >= asleep + Duration::from_millis(40),
        "40 ms of busy work counted only {spent:?}"
    );
}

/// #1651: time between polls (the job yielded and is queued behind other work) is not compute,
/// on any platform; the stage runs from its first poll until the future completes.
#[test]
fn thread_cpu_clock_does_not_count_time_between_polls() {
    let clock = ComputeClock::thread_cpu();
    let (tx, rx) = async_channel::unbounded();
    clock.notify_on_stage_start(tx);
    let mut polled = false;
    let mut stage = Box::pin(clock.measure(std::future::poll_fn(move |cx| {
        if polled {
            Poll::Ready(())
        } else {
            polled = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })));
    assert!(!clock.is_running(), "a queued stage is not running");
    assert!(rx.try_recv().is_err());

    let mut cx = Context::from_waker(Waker::noop());
    assert!(stage.as_mut().poll(&mut cx).is_pending());
    assert!(clock.is_running(), "the stage runs from its first poll");
    assert!(rx.try_recv().is_ok(), "the first poll announces the stage");
    let after_first_poll = clock.spent();

    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(
        clock.spent(),
        after_first_poll,
        "a yielded stage spends nothing while it waits to be polled"
    );

    assert!(stage.as_mut().poll(&mut cx).is_ready());
    assert!(!clock.is_running());
    assert!(
        clock.spent() < after_first_poll + WAIT_CHARGE_LIMIT,
        "{:?}",
        clock.spent()
    );
    assert!(rx.try_recv().is_err(), "one stage, one announcement");
}

/// #1651: the waiting Enter reads the clock from the main thread while the job's poll is still
/// running on a worker: it sees the worker's CPU so far, and not the time the worker is off the
/// CPU.
#[test]
fn thread_cpu_clock_reads_a_running_poll_from_another_thread() {
    if current_thread_cpu().is_none() || ThreadCpuProbe::current().is_none() {
        return;
    }
    let clock = ComputeClock::thread_cpu();
    let (busy_tx, busy_rx) = mpsc::channel();
    let (exit_tx, exit_rx) = mpsc::channel::<()>();
    let job = {
        let clock = clock.clone();
        std::thread::spawn(move || {
            futures::executor::block_on(clock.measure(async move {
                spin_cpu(Duration::from_millis(60));
                busy_tx.send(()).unwrap();
                // Still inside the poll, off the CPU.
                let _ = exit_rx.recv();
            }));
        })
    };
    busy_rx.recv().unwrap();
    assert!(clock.is_running());
    let busy = clock.spent();
    assert!(
        busy >= Duration::from_millis(40),
        "the running poll's 60 ms of work read as {busy:?}"
    );
    std::thread::sleep(Duration::from_millis(150));
    let waited = clock.spent().saturating_sub(busy);
    assert!(
        waited < WAIT_CHARGE_LIMIT,
        "150 ms with the job blocked read as {waited:?} of compute"
    );
    exit_tx.send(()).unwrap();
    job.join().unwrap();
    assert!(!clock.is_running());
    assert!(clock.spent() >= Duration::from_millis(40));
}

/// A wall clock fed by `measure` counts the stage's wall time, as `begin` does (the Tab
/// completer's clock).
#[test]
fn wall_clock_measure_counts_the_stage_wall_time() {
    let clock = ComputeClock::default();
    futures::executor::block_on(clock.measure(async {
        std::thread::sleep(Duration::from_millis(30));
    }));
    assert!(!clock.is_running());
    assert!(clock.spent() >= Duration::from_millis(30));
}
