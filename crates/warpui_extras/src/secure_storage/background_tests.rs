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

fn futures_lite_block_on<F: std::future::Future>(f: F) -> F::Output {
    warpui::r#async::block_on(f)
}
