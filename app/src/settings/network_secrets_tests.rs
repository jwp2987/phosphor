//! Coverage for the deferred keychain read in [`ProxyCredentials`] (#1164 item 6).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use warpui::App;
use warpui_extras::secure_storage::{self, Error, SecureStorage};

use super::*;

#[derive(Clone, Default)]
struct CountingStorage {
    values: Rc<RefCell<HashMap<String, String>>>,
    reads: Rc<Cell<usize>>,
}

impl SecureStorage for CountingStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        self.values
            .borrow_mut()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn read_value(&self, key: &str) -> Result<String, Error> {
        self.reads.set(self.reads.get() + 1);
        self.values
            .borrow()
            .get(key)
            .cloned()
            .ok_or(Error::NotFound)
    }

    fn remove_value(&self, key: &str) -> Result<(), Error> {
        self.values.borrow_mut().remove(key);
        Ok(())
    }
}

fn register(app: &mut App, store: &CountingStorage) {
    let store = store.clone();
    app.update(move |ctx| {
        ctx.add_singleton_model(move |_| -> secure_storage::Model { Box::new(store) });
    });
}

/// Construction must not touch the keychain; the first consumer loads the stored value.
#[test]
fn keychain_is_not_read_until_needed() {
    App::test((), |mut app| async move {
        let store = CountingStorage::default();
        store
            .values
            .borrow_mut()
            .insert(SECURE_STORAGE_KEY.to_owned(), "hunter2".to_owned());
        register(&mut app, &store);

        let creds = app.add_model(ProxyCredentials::new);
        assert_eq!(store.reads.get(), 0, "launch must not read the keychain");

        creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
        creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
        assert_eq!(store.reads.get(), 1, "loaded once, then cached");
        creds.read(&app, |creds, _| assert_eq!(creds.password(), "hunter2"));
    });
}

/// Clearing before anything loaded the password must still delete the stored one.
#[test]
fn clearing_unloaded_password_removes_stored_value() {
    App::test((), |mut app| async move {
        let store = CountingStorage::default();
        store
            .values
            .borrow_mut()
            .insert(SECURE_STORAGE_KEY.to_owned(), "hunter2".to_owned());
        register(&mut app, &store);

        let creds = app.add_model(ProxyCredentials::new);
        creds.update(&mut app, |creds, ctx| {
            creds.set_password(String::new(), ctx)
        });
        assert!(store.values.borrow().get(SECURE_STORAGE_KEY).is_none());
    });
}

/// #1805: the read runs on the secure-storage worker, never on the UI thread. These install a
/// worker factory whose handle hangs or fails like a keyring that never answers.
mod off_thread {
    use std::collections::HashMap;
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    use warpui::App;
    use warpui_extras::secure_storage::{self, Error, SecureStorage};

    use super::super::*;
    use super::CountingStorage;

    #[derive(Clone, Copy)]
    enum ReadBehaviour {
        /// Blocks until released, then returns the stored value.
        HangUntilReleased,
        /// Fails at once with an error.
        Fail,
        /// What the Linux backend does with no Secret Service: the keyring is marked
        /// unavailable and the read finds nothing (only the empty fallback was consulted).
        UnavailableLikeLinux,
        /// Returns the stored value at once.
        Succeed,
    }

    #[derive(Clone)]
    struct Keyring {
        behaviour: Arc<Mutex<ReadBehaviour>>,
        release: Arc<(Mutex<bool>, Condvar)>,
        data: Arc<Mutex<HashMap<String, String>>>,
        reads: Arc<Mutex<usize>>,
        writes: Arc<Mutex<usize>>,
    }

    impl Keyring {
        fn new(behaviour: ReadBehaviour) -> Self {
            Self {
                behaviour: Arc::new(Mutex::new(behaviour)),
                release: Arc::new((Mutex::new(false), Condvar::new())),
                data: Arc::default(),
                reads: Arc::default(),
                writes: Arc::default(),
            }
        }

        fn set_behaviour(&self, behaviour: ReadBehaviour) {
            *self.behaviour.lock().unwrap() = behaviour;
        }

        fn release(&self) {
            *self.release.0.lock().unwrap() = true;
            self.release.1.notify_all();
        }

        fn reads(&self) -> usize {
            *self.reads.lock().unwrap()
        }

        fn writes(&self) -> usize {
            *self.writes.lock().unwrap()
        }
    }

    impl SecureStorage for Keyring {
        fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
            self.data
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.to_owned());
            *self.writes.lock().unwrap() += 1;
            Ok(())
        }

        fn read_value(&self, key: &str) -> Result<String, Error> {
            let behaviour = *self.behaviour.lock().unwrap();
            match behaviour {
                ReadBehaviour::Succeed => {}
                ReadBehaviour::UnavailableLikeLinux => {
                    *self.reads.lock().unwrap() += 1;
                    secure_storage::availability::mark_unavailable();
                    return Err(Error::NotFound);
                }
                ReadBehaviour::HangUntilReleased => {
                    let (lock, cv) = &*self.release;
                    let mut released = lock.lock().unwrap();
                    while !*released {
                        released = cv.wait(released).unwrap();
                    }
                }
                ReadBehaviour::Fail => {
                    *self.reads.lock().unwrap() += 1;
                    return Err(Error::Unknown(anyhow::anyhow!("no Secret Service")));
                }
            }
            *self.reads.lock().unwrap() += 1;
            self.data
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .ok_or(Error::NotFound)
        }

        fn remove_value(&self, key: &str) -> Result<(), Error> {
            self.data.lock().unwrap().remove(key);
            *self.writes.lock().unwrap() += 1;
            Ok(())
        }
    }

    fn install(app: &mut App, keyring: &Keyring) {
        // `run_or_inline` is handed the app's own handle for backends without a worker; it must
        // never be used here (it is empty and counts nothing the assertions rely on).
        super::register(app, &CountingStorage::default());
        let keyring = keyring.clone();
        secure_storage::background::set_factory_for_test(Some(Arc::new(move || {
            Box::new(keyring.clone()) as secure_storage::Model
        })));
    }

    async fn wait_until(mut done: impl FnMut() -> bool) {
        for _ in 0..400 {
            if done() {
                return;
            }
            warpui::r#async::Timer::after(Duration::from_millis(10)).await;
        }
        panic!("condition not reached");
    }

    /// A keyring that never answers must not hold the model update (and with it the first
    /// frame): `ensure_loaded` returns at once, the UI sees "loading", and the value arrives
    /// with an event when the read lands.
    #[test]
    fn a_hanging_keyring_does_not_block_the_model_update() {
        App::test((), |mut app| async move {
            let keyring = Keyring::new(ReadBehaviour::HangUntilReleased);
            keyring
                .data
                .lock()
                .unwrap()
                .insert(SECURE_STORAGE_KEY.to_owned(), "hunter2".to_owned());
            install(&mut app, &keyring);
            let creds = app.add_model(ProxyCredentials::new);
            let events = std::rc::Rc::new(std::cell::Cell::new(0));
            let seen = events.clone();
            app.update(|ctx| {
                ctx.subscribe_to_model(&creds, move |_, _, _| seen.set(seen.get() + 1));
            });

            let started = Instant::now();
            creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
            creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
            assert!(
                started.elapsed() < Duration::from_secs(1),
                "the model update waited on the keyring"
            );
            creds.read(&app, |creds, _| {
                assert_eq!(creds.load_state(), ProxyPasswordLoad::Loading);
                assert_eq!(creds.password(), "");
            });

            keyring.release();
            wait_until(|| creds.read(&app, |c, _| c.load_state() == ProxyPasswordLoad::Loaded))
                .await;
            creds.read(&app, |creds, _| assert_eq!(creds.password(), "hunter2"));
            assert_eq!(
                keyring.reads(),
                1,
                "one read, however often it was asked for"
            );
            assert_eq!(events.get(), 1, "the landed read is announced once");
            secure_storage::background::set_factory_for_test(None);
        });
    }

    /// A keyring that fails leaves the password unavailable (the app keeps working without it)
    /// and is not asked again by later consumers; a password the user sets still works.
    #[test]
    fn a_failing_keyring_is_read_once_and_the_password_stays_usable() {
        App::test((), |mut app| async move {
            let keyring = Keyring::new(ReadBehaviour::Fail);
            install(&mut app, &keyring);
            let creds = app.add_model(ProxyCredentials::new);

            creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
            wait_until(|| {
                creds.read(&app, |c, _| {
                    c.load_state() == ProxyPasswordLoad::Unavailable
                })
            })
            .await;
            for _ in 0..3 {
                creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
            }
            // Give a (wrong) retry the chance to reach the worker before counting.
            warpui::r#async::Timer::after(Duration::from_millis(50)).await;
            assert_eq!(keyring.reads(), 1, "no repeated failing reads");
            creds.read(&app, |creds, _| assert_eq!(creds.password(), ""));

            creds.update(&mut app, |creds, ctx| {
                creds.set_password("typed".to_owned(), ctx)
            });
            creds.read(&app, |creds, _| {
                assert_eq!(creds.load_state(), ProxyPasswordLoad::Loaded);
                assert_eq!(creds.password(), "typed");
            });
            wait_until(|| keyring.writes() == 1).await;

            // Once the user has set a password it is authoritative: a keyring retry does not
            // re-read (and so cannot replace) it.
            creds.update(&mut app, |creds, ctx| creds.retry_if_unavailable(ctx));
            warpui::r#async::Timer::after(Duration::from_millis(50)).await;
            assert_eq!(
                keyring.reads(),
                1,
                "a loaded password is not re-read by a retry"
            );
            secure_storage::background::set_factory_for_test(None);
        });
    }

    /// A password set while the read is still in flight is newer than whatever the read
    /// returns: the stale read is discarded and the stored value ends as the one set.
    #[test]
    fn a_password_set_while_the_read_is_in_flight_wins() {
        App::test((), |mut app| async move {
            let keyring = Keyring::new(ReadBehaviour::HangUntilReleased);
            keyring
                .data
                .lock()
                .unwrap()
                .insert(SECURE_STORAGE_KEY.to_owned(), "old".to_owned());
            install(&mut app, &keyring);
            let creds = app.add_model(ProxyCredentials::new);

            creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
            creds.update(&mut app, |creds, ctx| {
                creds.set_password("new".to_owned(), ctx)
            });
            keyring.release();
            // The write is queued behind the read on the worker.
            wait_until(|| keyring.writes() == 1).await;
            warpui::r#async::Timer::after(Duration::from_millis(50)).await;

            creds.read(&app, |creds, _| {
                assert_eq!(creds.load_state(), ProxyPasswordLoad::Loaded);
                assert_eq!(creds.password(), "new");
            });
            assert_eq!(
                keyring
                    .data
                    .lock()
                    .unwrap()
                    .get(SECURE_STORAGE_KEY)
                    .cloned(),
                Some("new".to_owned())
            );
            secure_storage::background::set_factory_for_test(None);
        });
    }

    /// #1805 review: on Linux a keyring outage reads as "not found" from the backend; it must
    /// still surface as Unavailable, and the explicit retry must then load the stored password.
    #[test]
    fn an_outage_is_unavailable_and_a_retry_loads_the_stored_password() {
        App::test((), |mut app| async move {
            secure_storage::availability::reset_for_test();
            let keyring = Keyring::new(ReadBehaviour::UnavailableLikeLinux);
            keyring
                .data
                .lock()
                .unwrap()
                .insert(SECURE_STORAGE_KEY.to_owned(), "hunter2".to_owned());
            install(&mut app, &keyring);
            let creds = app.add_model(ProxyCredentials::new);

            creds.update(&mut app, |creds, ctx| creds.ensure_loaded(ctx));
            wait_until(|| {
                creds.read(&app, |c, _| {
                    c.load_state() == ProxyPasswordLoad::Unavailable
                })
            })
            .await;
            creds.read(&app, |creds, _| assert_eq!(creds.password(), ""));

            // The keyring is back; the user's retry (KeyringNotice) resets the state and re-reads.
            keyring.set_behaviour(ReadBehaviour::Succeed);
            assert!(secure_storage::availability::retry());
            creds.update(&mut app, |creds, ctx| creds.retry_if_unavailable(ctx));
            wait_until(|| creds.read(&app, |c, _| c.load_state() == ProxyPasswordLoad::Loaded))
                .await;
            creds.read(&app, |creds, _| assert_eq!(creds.password(), "hunter2"));
            assert_eq!(keyring.reads(), 2, "one failed read, one retry");
            secure_storage::background::set_factory_for_test(None);
            secure_storage::availability::reset_for_test();
        });
    }
}
