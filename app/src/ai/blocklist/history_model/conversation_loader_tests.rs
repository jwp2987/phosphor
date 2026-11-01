use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use super::load_pool::{MAX_LOAD_THREADS, start};

#[test]
fn many_concurrent_loads_use_at_most_the_cap_threads() {
    // #1363: the old code spawned one thread per load.
    let tx = start(MAX_LOAD_THREADS).expect("pool starts");
    let threads = Arc::new(Mutex::new(HashSet::new()));
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let (done_tx, done_rx) = mpsc::channel();
    const LOADS: usize = 40;
    for _ in 0..LOADS {
        let (threads, running, peak, done_tx) = (
            threads.clone(),
            running.clone(),
            peak.clone(),
            done_tx.clone(),
        );
        tx.send(Box::new(move || {
            threads.lock().unwrap().insert(std::thread::current().id());
            let now = running.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(now, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(2));
            running.fetch_sub(1, Ordering::SeqCst);
            done_tx.send(()).unwrap();
        }))
        .unwrap();
    }
    for _ in 0..LOADS {
        done_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("every queued load runs");
    }
    assert!(threads.lock().unwrap().len() <= MAX_LOAD_THREADS);
    assert!(peak.load(Ordering::SeqCst) <= MAX_LOAD_THREADS);
}
