//! `ProxyCredentials`: stores the proxy's Basic Auth password in the OS keychain (see Issue #72).
//!
//! Only the password is stored; the username, URL, and other non-sensitive fields remain in
//! `NetworkSettings`'s settings.toml. The design mirrors
//! `crate::ai::agent_providers::AgentProviderSecrets`: it is based on
//! `warpui_extras::secure_storage` (macOS Keychain / Windows DPAPI / Linux Keyring).
//!
//! Note: the proxy has only one global password, so storage uses a single key whose value is
//! the raw password string (no JSON map involved).

use warpui::{Entity, ModelContext, SingletonEntity};
use warpui_extras::secure_storage::{self, AppContextExt};

const SECURE_STORAGE_KEY: &str = "ProxyPassword";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProxyCredentialsEvent {
    /// The password value changed (may be empty), or the stored one finished loading.
    PasswordChanged,
}

/// Where the stored password is in its life (#1805).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyPasswordLoad {
    /// Nothing has needed it yet.
    NotStarted,
    /// The read is queued on the secure-storage worker; [`ProxyCredentials::password`] is empty
    /// until it lands.
    Loading,
    /// [`ProxyCredentials::password`] is the stored value (or the one the user has set since).
    Loaded,
    /// The keyring could not be read; the password is empty unless the user sets one.
    Unavailable,
}

/// Singleton: manages the Basic Auth password for the global HTTP proxy.
pub struct ProxyCredentials {
    password: String,
    /// The read is deferred (#1164 item 6): most installs use no custom proxy, so nothing needs
    /// the password and the keychain read is skipped at launch. Everything that consumes the
    /// password calls [`Self::ensure_loaded`] first.
    ///
    /// The read never runs on the UI thread (#1805): with no Secret Service answering, a
    /// synchronous read here held the first frame for the bus's 120 s activation timeout. It is
    /// queued on the secure-storage worker and lands later with a `PasswordChanged` event.
    load: ProxyPasswordLoad,
    /// Bumped by every read started and every explicit set, so a read that lands after the user
    /// has set a password is discarded (the user's value is newer).
    load_epoch: u64,
}

impl ProxyCredentials {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let _ = ctx;
        Self {
            password: String::new(),
            load: ProxyPasswordLoad::NotStarted,
            load_epoch: 0,
        }
    }

    /// Starts reading the password from secure storage the first time it is needed. Returns at
    /// once; when the read lands, [`ProxyCredentialsEvent::PasswordChanged`] is emitted. (With a
    /// backend that has no worker -- no-op and test storage -- the read is synchronous and does
    /// not emit.) A failed read is not retried by this call; see [`Self::retry_if_unavailable`].
    pub fn ensure_loaded(&mut self, ctx: &mut ModelContext<Self>) {
        if self.load == ProxyPasswordLoad::NotStarted {
            self.start_load(ctx);
        }
    }

    /// After a failed read only: reads the stored password again. Called on an explicit user
    /// retry of the keyring; a loaded (or never needed) password is left alone.
    pub fn retry_if_unavailable(&mut self, ctx: &mut ModelContext<Self>) {
        if self.load == ProxyPasswordLoad::Unavailable {
            self.start_load(ctx);
        }
    }

    /// Where the stored password is in its life.
    pub fn load_state(&self) -> ProxyPasswordLoad {
        self.load
    }

    /// Reads the current password; returns an empty string if none is set (or it has not
    /// loaded, see [`Self::load_state`]).
    pub fn password(&self) -> &str {
        &self.password
    }

    /// Sets / updates the password. Passing an empty string is equivalent to deleting it.
    pub fn set_password(&mut self, password: String, ctx: &mut ModelContext<Self>) {
        // Only a loaded value can be compared against: before that, an empty `password` is a
        // placeholder, and skipping the write would leave a stored password undeleted.
        if self.load == ProxyPasswordLoad::Loaded && self.password == password {
            return;
        }
        // The user's value is authoritative: a read still in flight is stale once it lands, and
        // the write is queued behind it on the worker, so storage ends with this value.
        self.load_epoch += 1;
        self.load = ProxyPasswordLoad::Loaded;
        self.password = password;
        self.persist(ctx);
        ctx.emit(ProxyCredentialsEvent::PasswordChanged);
    }

    fn start_load(&mut self, ctx: &mut ModelContext<Self>) {
        self.load_epoch += 1;
        let epoch = self.load_epoch;
        self.load = ProxyPasswordLoad::Loading;
        match secure_storage::background::run_or_inline(
            |storage| Self::read_from(storage),
            ctx.secure_storage(),
        ) {
            Ok(pending) => {
                ctx.spawn(pending.wait(), move |me, read, ctx| {
                    // `None`: the worker gave up on a keyring that never answered.
                    let read = read.unwrap_or_else(|| {
                        Err("the secure-storage worker gave up on the read".to_owned())
                    });
                    if me.finish_load(epoch, read) {
                        ctx.emit(ProxyCredentialsEvent::PasswordChanged);
                    }
                });
            }
            // Synchronous backend (no worker): never blocks; callers read the value right after.
            Err(read) => {
                self.finish_load(epoch, read);
            }
        }
    }

    /// Adopts a completed read unless it is stale. Returns whether it was adopted.
    fn finish_load(&mut self, epoch: u64, read: Result<String, String>) -> bool {
        if epoch != self.load_epoch {
            return false;
        }
        match read {
            Ok(password) => {
                self.password = password;
                self.load = ProxyPasswordLoad::Loaded;
            }
            Err(reason) => {
                log::warn!("The proxy password could not be read from secure storage: {reason}");
                self.password.clear();
                self.load = ProxyPasswordLoad::Unavailable;
            }
        }
        true
    }

    /// Runs on the secure-storage worker. "Nothing stored" is an empty password, not a failure --
    /// unless the keyring is unavailable: the Linux backend then reads only its fallback and
    /// reports a password kept in the keyring as "not found", which must surface as unavailable
    /// (so the page says so and an explicit retry reads it again), not as "no password".
    fn read_from(storage: &dyn secure_storage::SecureStorage) -> Result<String, String> {
        match storage.read_value(SECURE_STORAGE_KEY) {
            Ok(value) => Ok(value),
            Err(secure_storage::Error::NotFound)
                if secure_storage::availability::current()
                    == secure_storage::availability::Availability::Unavailable =>
            {
                Err("the OS keyring is unavailable".to_owned())
            }
            Err(secure_storage::Error::NotFound) => Ok(String::new()),
            Err(e) => Err(format!("{e:#}")),
        }
    }

    /// Queued on the secure-storage worker, in order with every other secure-storage call;
    /// synchronous only for backends that never block.
    fn persist(&self, ctx: &mut ModelContext<Self>) {
        let password = self.password.clone();
        let _ = secure_storage::background::run_or_inline(
            move |storage| {
                if password.is_empty() {
                    // An empty string means "no password"; a failed delete is tolerated, just
                    // logged.
                    if let Err(e) = storage.remove_value(SECURE_STORAGE_KEY) {
                        if !matches!(e, secure_storage::Error::NotFound) {
                            log::error!("Failed to remove proxy password: {e:#}");
                        }
                    }
                } else if let Err(e) = storage.write_value(SECURE_STORAGE_KEY, &password) {
                    log::error!("Failed to write proxy password: {e:#}");
                }
            },
            ctx.secure_storage(),
        );
    }
}

impl Entity for ProxyCredentials {
    type Event = ProxyCredentialsEvent;
}

impl SingletonEntity for ProxyCredentials {}

#[cfg(test)]
#[path = "network_secrets_tests.rs"]
mod tests;
