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

/// #1651 review: a detection stage that waits off the CPU on something outside the classifier (a
/// channel fed 1.5 s later stands in for a cold filesystem or a loading model) spends no CPU, but
/// its wall time runs, and the Enter's cap is reached at the 1 s wall ceiling, well before the
/// 2 s lost-job backstop. The timer the Enter arms at the stage's start is for that ceiling.
#[test]
fn a_stage_waiting_off_the_cpu_reaches_the_cap_at_the_wall_ceiling() {
    use crate::ai::blocklist::input_model::{
        ENTER_VERDICT_BUDGET, ENTER_VERDICT_WALL_CEILING, enter_verdict_cap_reached,
        enter_verdict_cap_remaining,
    };

    let clock = ComputeClock::thread_cpu();
    let (feed_tx, feed_rx) = async_channel::bounded::<()>(1);
    let (started_tx, started_rx) = mpsc::channel();
    let job = {
        let clock = clock.clone();
        std::thread::spawn(move || {
            futures::executor::block_on(clock.measure(async move {
                started_tx.send(()).unwrap();
                let _ = feed_rx.recv().await;
            }));
        })
    };
    started_rx.recv().unwrap();
    let started = Instant::now();
    assert!(clock.is_running());
    // The Enter's timer is armed for whichever runs out first. With no CPU spent that is the
    // CPU budget's rest (<= 50 ms, so it re-checks at that pace, never in a tight loop), until the
    // ceiling's rest is the shorter one.
    assert!(
        enter_verdict_cap_remaining(clock.spent(), clock.stage_wall(), 10) <= ENTER_VERDICT_BUDGET
    );

    std::thread::sleep(Duration::from_millis(300));
    // Judged on the measured stage wall time, not on the sleep: a loaded CI thread can oversleep.
    let (cpu, wall) = (clock.spent(), clock.stage_wall());
    if wall < ENTER_VERDICT_WALL_CEILING {
        assert!(
            !enter_verdict_cap_reached(cpu, wall, 10),
            "under the wall ceiling ({wall:?}) with {cpu:?} of CPU: neither cap is reached"
        );
    }
    let remaining = enter_verdict_cap_remaining(cpu, wall, 10);
    assert!(remaining <= ENTER_VERDICT_BUDGET);

    // Wait out the ceiling (the channel is fed at 1.5 s).
    while started.elapsed() < ENTER_VERDICT_WALL_CEILING + Duration::from_millis(50) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let (cpu, wall) = (clock.spent(), clock.stage_wall());
    assert!(
        cpu < ENTER_VERDICT_BUDGET,
        "a stage waiting on a channel spent {cpu:?} of CPU"
    );
    assert!(wall >= ENTER_VERDICT_WALL_CEILING, "{wall:?}");
    assert!(
        enter_verdict_cap_reached(cpu, wall, 10),
        "at ~1 s of wall time the cap is reached, before the 2 s backstop"
    );
    while started.elapsed() < Duration::from_millis(1500) {
        std::thread::sleep(Duration::from_millis(10));
    }
    feed_tx.try_send(()).unwrap();
    job.join().unwrap();
    assert!(!clock.is_running());
}

/// #1651 review: a stage starved of CPU (yielded and queued, or descheduled) under the wall
/// ceiling is not given up on before it has spent its 50 ms of CPU.
#[test]
fn a_cpu_starved_stage_under_the_ceiling_keeps_its_cpu_budget() {
    use crate::ai::blocklist::input_model::{ENTER_VERDICT_BUDGET, enter_verdict_cap_reached};

    // Injected: 600 ms of wall time with only 10 ms of CPU, then the budget reached.
    let (clock, guard) = ComputeClock::thread_cpu_running_for_test(
        Duration::from_millis(10),
        Duration::from_millis(600),
    );
    assert!(!enter_verdict_cap_reached(
        clock.spent(),
        clock.stage_wall(),
        10
    ));
    drop(guard);
    let (clock, _guard) =
        ComputeClock::thread_cpu_running_for_test(ENTER_VERDICT_BUDGET, Duration::from_millis(600));
    assert!(enter_verdict_cap_reached(
        clock.spent(),
        clock.stage_wall(),
        10
    ));

    // Real: a stage that yields between polls for 300 ms of wall time spends almost no CPU.
    if current_thread_cpu().is_none() {
        // Without a thread CPU clock a poll is timed on wall time, sleeps included.
        return;
    }
    let clock = ComputeClock::thread_cpu();
    let mut stage = Box::pin(clock.measure(async {
        let until = Instant::now() + Duration::from_millis(300);
        while Instant::now() < until {
            futures_lite::future::yield_now().await;
            std::thread::sleep(Duration::from_millis(5));
        }
    }));
    let mut cx = Context::from_waker(Waker::noop());
    while stage.as_mut().poll(&mut cx).is_pending() {
        // Queued behind other work between polls.
        std::thread::sleep(Duration::from_millis(5));
        assert!(
            !enter_verdict_cap_reached(clock.spent(), clock.stage_wall(), 10),
            "starved under the ceiling: CPU {:?}, wall {:?}",
            clock.spent(),
            clock.stage_wall()
        );
    }
}

/// #1651 review: the main thread reads a poll that is STILL running and still on the CPU: the
/// clock (through the job thread's probe) rises past 40 ms while the worker spins, and only then
/// is the worker stopped. This is what the macOS `thread_info` path is for.
#[test]
fn spent_rises_while_the_poll_is_still_spinning() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    if current_thread_cpu().is_none() || ThreadCpuProbe::current().is_none() {
        return;
    }
    let clock = ComputeClock::thread_cpu();
    let stop = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let job = {
        let (clock, stop, finished) = (clock.clone(), stop.clone(), finished.clone());
        std::thread::spawn(move || {
            futures::executor::block_on(clock.measure(async move {
                let mut x = 1u64;
                while !stop.load(Ordering::Relaxed) {
                    for i in 0..10_000u64 {
                        x = x.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(i);
                    }
                    std::hint::black_box(x);
                }
            }));
            finished.store(true, Ordering::Relaxed);
        })
    };
    let wall = Instant::now();
    loop {
        let spent = clock.spent();
        assert!(
            !finished.load(Ordering::Relaxed),
            "the worker cannot stop before it is told"
        );
        if spent >= Duration::from_millis(40) {
            break;
        }
        assert!(
            wall.elapsed() < Duration::from_secs(30),
            "a spinning poll never showed on the clock: {spent:?}"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(clock.is_running(), "read while the poll is still running");
    stop.store(true, Ordering::Relaxed);
    job.join().unwrap();
    assert!(clock.spent() >= Duration::from_millis(40));
}
