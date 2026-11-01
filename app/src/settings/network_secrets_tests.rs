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
