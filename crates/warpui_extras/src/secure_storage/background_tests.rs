use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::*;
use crate::secure_storage::Error;

/// A `Send` in-memory store shared between every handle the factory builds, recording the thread
/// each operation ran on and the order of operations.
#[derive(Clone, Default)]
struct Shared {
    data: Arc<Mutex<HashMap<String, String>>>,
    log: Arc<Mutex<Vec<(String, std::thread::ThreadId)>>>,
}

struct Handle(Shared);

impl SecureStorage for Handle {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        self.0
            .log
            .lock()
            .unwrap()
            .push((format!("w:{value}"), std::thread::current().id()));
        self.0
            .data
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn read_value(&self, key: &str) -> Result<String, Error> {
        self.0
            .log
            .lock()
            .unwrap()
            .push(("r".to_owned(), std::thread::current().id()));
        self.0
            .data
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(Error::NotFound)
    }
    fn remove_value(&self, _key: &str) -> Result<(), Error> {
        Ok(())
    }
}

fn install(shared: &Shared) {
    let shared = shared.clone();
    set_factory_for_test(Some(Arc::new(move || -> Model {
        Box::new(Handle(shared.clone()))
    })));
}

#[test]
fn jobs_run_off_the_calling_thread_in_submission_order() {
    let shared = Shared::default();
    install(&shared);
    assert!(is_available());

    let w1 = run(|s| s.write_value("k", "1")).unwrap();
    let w2 = run(|s| s.write_value("k", "2")).unwrap();
    let r = run(|s| s.read_value("k")).unwrap();
    let read = futures_lite_block_on(r.wait()).unwrap().unwrap();
    let _ = (
        futures_lite_block_on(w1.wait()),
        futures_lite_block_on(w2.wait()),
    );

    // The read, queued after both writes, sees the later one.
    assert_eq!(read, "2");
    let log = shared.log.lock().unwrap();
    let ops: Vec<_> = log.iter().map(|(op, _)| op.as_str()).collect();
    assert_eq!(ops, ["w:1", "w:2", "r"]);
    assert!(log.iter().all(|(_, t)| *t != std::thread::current().id()));
    set_factory_for_test(None);
}

#[test]
fn without_a_factory_run_declines_so_callers_stay_synchronous() {
    set_factory_for_test(None);
    assert!(!is_available());
    assert!(run(|_| ()).is_none());
}

/// A handle whose reads of the key "hang" never return until `release` is set, like a locked
/// keyring whose D-Bus call is waiting on a prompt nobody answers.
struct Hanging {
    release: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

impl SecureStorage for Hanging {
    fn write_value(&self, _key: &str, _value: &str) -> Result<(), Error> {
        Ok(())
    }
    fn read_value(&self, key: &str) -> Result<String, Error> {
        if key == "hang" {
            let (lock, cv) = &*self.release;
            let mut released = lock.lock().unwrap();
            while !*released {
                released = cv.wait(released).unwrap();
            }
        }
        Ok(format!("value:{key}"))
    }
    fn remove_value(&self, _key: &str) -> Result<(), Error> {
        Ok(())
    }
}

#[test]
fn a_stuck_call_is_given_up_on_and_later_calls_use_a_fresh_worker() {
    let release = Arc::new((Mutex::new(false), std::sync::Condvar::new()));
    let for_factory = release.clone();
    set_give_up_for_test(Some(std::time::Duration::from_millis(150)));
    set_factory_for_test(Some(Arc::new(move || -> Model {
        Box::new(Hanging {
            release: for_factory.clone(),
        })
    })));

    let stuck = run(|s| s.read_value("hang")).unwrap();
    // Queued behind the stuck call: it would never run on the old worker.
    let behind = run(|s| s.read_value("behind")).unwrap();
    assert_eq!(futures_lite_block_on(stuck.wait()), None);
    assert_eq!(futures_lite_block_on(behind.wait()), None);

    // A later call is served by a fresh worker instead of hanging forever.
    let later = run(|s| s.read_value("later")).unwrap();
    assert_eq!(
        futures_lite_block_on(later.wait()).unwrap().unwrap(),
        "value:later"
    );

    // Unblock the abandoned thread so the test leaves nothing parked.
    *release.0.lock().unwrap() = true;
    release.1.notify_all();
    set_give_up_for_test(None);
    set_factory_for_test(None);
}

fn futures_lite_block_on<F: std::future::Future>(f: F) -> F::Output {
    warpui::r#async::block_on(f)
}
