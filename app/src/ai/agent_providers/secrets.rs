//! `AgentProviderSecrets`: stores each custom provider's API key in the OS key store.
//!
//! Data shape: `HashMap<provider_id, api_key>`, serialized via `serde_json` and
//! written under the `AgentProviderSecrets` key in `secure_storage`.
//!
//! Design modeled after `crates/ai/src/api_keys.rs::ApiKeyManager`.

use std::collections::HashMap;

use futures::channel::oneshot;

use warpui::{Entity, ModelContext, SingletonEntity};
use warpui_extras::secure_storage::{self, AppContextExt};

const SECURE_STORAGE_KEY: &str = "AgentProviderSecrets";

/// Emitted whenever any provider's API key changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentProviderSecretsEvent {
    KeysUpdated,
}

/// Singleton: manages the user's custom provider API keys.
///
/// The keychain read happens off the UI thread (#1309): [`Self::new`] returns an empty store and
/// the read lands through [`Self::finish_load`], which emits `KeysUpdated` so subscribers
/// re-derive from [`Self::get`]. Writes go through the ordered secure-storage worker.
pub struct AgentProviderSecrets {
    keys: HashMap<String, String>,
    /// Edits made while a keychain read is in flight. They are re-applied on top of what the read
    /// returns (so neither side is lost) and only then persisted -- persisting the in-memory map
    /// before the read lands would overwrite the stored keys with a partial set.
    pending: Vec<(String, Option<String>)>,
    /// Whether a read is in flight.
    loading: bool,
    /// Identifies the latest read; a result from an older one is stale and dropped.
    load_epoch: u64,
    /// Whether the first read has completed.
    loaded: bool,
    /// The last read failed (the keyring errored or the secure-storage worker gave up on it), so
    /// the in-memory map is NOT the stored one. Nothing is written while this is the case, and
    /// the next [`Self::wait_for_load`] starts a fresh read.
    load_failed: bool,
    /// Requests parked until the first read settles (see [`Self::wait_for_load`]).
    waiters: Vec<oneshot::Sender<Option<HashMap<String, String>>>>,
}

impl AgentProviderSecrets {
    /// Starts reading all keys from secure storage; they arrive shortly after construction.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let mut me = Self {
            keys: HashMap::new(),
            pending: Vec::new(),
            loading: false,
            load_epoch: 0,
            loaded: false,
            load_failed: false,
            waiters: Vec::new(),
        };
        me.start_load(ctx);
        me
    }

    /// Whether the initial read of the stored keys has completed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// For a request that must not go out with an empty key because the first keyring read has
    /// not landed. `None`: the keys are loaded, read them with [`Self::get`]. `Some(rx)`: await it;
    /// it resolves with the loaded keys when the read lands, or `None` if the keyring could not
    /// be read (error, or the secure-storage worker gave up on a call that never returned).
    /// Event-driven: resolved by the read's own completion, no polling.
    ///
    /// After a failed read this starts a fresh one, so the next user action retries.
    pub fn wait_for_load(
        &mut self,
        ctx: &mut ModelContext<Self>,
    ) -> Option<oneshot::Receiver<Option<HashMap<String, String>>>> {
        if self.loaded {
            return None;
        }
        let (tx, rx) = oneshot::channel();
        self.waiters.push(tx);
        if !self.loading {
            self.start_load(ctx);
        }
        if self.loaded {
            // The read finished synchronously (no worker): nothing to wait for.
            self.waiters.clear();
            return None;
        }
        // Still in flight, or finished synchronously and failed (`finish_load` has then already
        // resolved `rx` with `None`).
        Some(rx)
    }

    /// Reads the API key for the given provider; returns `None` if not configured (or not yet
    /// loaded -- see [`Self::is_loaded`]; `KeysUpdated` is emitted when the load lands).
    pub fn get(&self, provider_id: &str) -> Option<&str> {
        self.keys.get(provider_id).map(String::as_str)
    }

    /// Sets/updates the API key for a provider.
    /// Passing an empty string is equivalent to deleting it.
    pub fn set(&mut self, provider_id: &str, api_key: String, ctx: &mut ModelContext<Self>) {
        let value = (!api_key.is_empty()).then_some(api_key);
        self.apply(provider_id, value.clone());
        if self.read_unsettled() {
            self.pending.push((provider_id.to_owned(), value));
        }
        ctx.emit(AgentProviderSecretsEvent::KeysUpdated);
        self.persist(ctx);
    }

    /// Removes a provider (along with its secret).
    pub fn remove(&mut self, provider_id: &str, ctx: &mut ModelContext<Self>) {
        // While the stored keys are still loading, the key may exist only on disk.
        if self.keys.remove(provider_id).is_some() || self.read_unsettled() {
            if self.read_unsettled() {
                self.pending.push((provider_id.to_owned(), None));
            }
            ctx.emit(AgentProviderSecretsEvent::KeysUpdated);
            self.persist(ctx);
        }
    }

    /// Whether the in-memory map may be missing stored keys (a read is in flight, the first one
    /// has not landed, or the last one failed): edits are parked and nothing is persisted.
    fn read_unsettled(&self) -> bool {
        self.loading || !self.loaded
    }

    fn apply(&mut self, provider_id: &str, value: Option<String>) {
        match value {
            Some(v) => {
                self.keys.insert(provider_id.to_owned(), v);
            }
            None => {
                self.keys.remove(provider_id);
            }
        }
    }

    /// Re-reads every key from secure storage, discarding the in-memory copy.
    ///
    /// [`Self::new`] reads secure storage once, at construction. That is enough
    /// while a process is the only writer, but this store is shared with every
    /// other Zap process (the GUI and the TUI use one app id, so one keyring
    /// namespace). Both surfaces edit BYOP keys — the GUI through Settings > AI,
    /// the TUI through `/api-keys` — so without this an already-running process
    /// keeps serving the keys it read at startup and the newly-saved key appears
    /// to have been ignored. Driven by the revision file; see
    /// [`crate::ai::tui_api_keys`].
    ///
    /// Emits [`AgentProviderSecretsEvent::KeysUpdated`] when the read lands, matching
    /// [`Self::set`]: subscribers re-derive from [`Self::get`] rather than diffing.
    #[cfg_attr(target_family = "wasm", allow(dead_code))]
    pub fn reload_from_secure_storage(&mut self, ctx: &mut ModelContext<Self>) {
        self.start_load(ctx);
    }

    fn start_load(&mut self, ctx: &mut ModelContext<Self>) {
        self.load_epoch += 1;
        self.loading = true;
        let epoch = self.load_epoch;
        match secure_storage::background::run_or_inline(
            |storage| Self::read_from(storage),
            ctx.secure_storage(),
        ) {
            Ok(result) => {
                ctx.spawn(result.wait(), move |me, loaded, ctx| {
                    // `None`: the worker gave up on the call (a locked keyring that never
                    // answered), which is a failed read just like an error from the keyring.
                    let loaded = loaded.unwrap_or_else(|| {
                        Err("the secure-storage worker gave up on the read".to_owned())
                    });
                    me.finish_load(epoch, loaded, true, ctx);
                });
            }
            // Synchronous backend. During construction nothing can be subscribed yet, and the
            // model is not registered, so don't emit then.
            Err(keys) => {
                let emit = self.loaded;
                self.finish_load(epoch, keys, emit, ctx)
            }
        }
    }

    /// Adopts a completed read, unless a newer one was started since.
    fn finish_load(
        &mut self,
        epoch: u64,
        keys: Result<HashMap<String, String>, String>,
        emit: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if epoch != self.load_epoch {
            return;
        }
        self.loading = false;
        let keys = match keys {
            Ok(keys) => keys,
            Err(reason) => {
                log::error!("Could not read agent provider API keys from the keyring: {reason}");
                // Keep whatever was loaded before (a reload) and keep edits parked; waiting
                // requests fail with a clear error instead of sending an empty key.
                self.load_failed = true;
                for waiter in std::mem::take(&mut self.waiters) {
                    let _ = waiter.send(None);
                }
                return;
            }
        };
        self.keys = keys;
        self.loaded = true;
        self.load_failed = false;
        let pending = std::mem::take(&mut self.pending);
        let had_pending = !pending.is_empty();
        for (id, value) in pending {
            self.apply(&id, value);
        }
        for waiter in std::mem::take(&mut self.waiters) {
            let _ = waiter.send(Some(self.keys.clone()));
        }
        if emit {
            ctx.emit(AgentProviderSecretsEvent::KeysUpdated);
        }
        if had_pending {
            self.persist(ctx);
        }
    }

    /// `Err` only when the keyring itself could not be read; "nothing stored" and an unparsable
    /// value are an empty map (as before).
    fn read_from(
        storage: &dyn secure_storage::SecureStorage,
    ) -> Result<HashMap<String, String>, String> {
        let raw = match storage.read_value(SECURE_STORAGE_KEY) {
            Ok(json) => json,
            Err(secure_storage::Error::NotFound) => return Ok(HashMap::new()),
            Err(e) => return Err(format!("{e:#}")),
        };
        Ok(serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::error!("Failed to deserialize agent provider secrets: {e:#}");
            HashMap::new()
        }))
    }

    /// The single choke point for every write to this store: [`Self::set`] and
    /// [`Self::remove`] are its only callers, and between them they cover every
    /// GUI and TUI mutation (add, edit, clear, delete a provider). The
    /// cross-process notification is therefore stamped here rather than at each
    /// of the five call sites, so a new mutation path cannot forget it.
    ///
    /// Deferred while a read is in flight (see `pending`); [`Self::finish_load`] persists then.
    fn persist(&self, ctx: &mut ModelContext<Self>) {
        if self.read_unsettled() {
            return;
        }
        let json = match serde_json::to_string(&self.keys) {
            Ok(json) => json,
            Err(e) => {
                log::error!("Failed to serialize agent provider secrets: {e:#}");
                return;
            }
        };
        // Queued behind any earlier write on the secure-storage worker, so writes land in the
        // order they were made; runs synchronously where no worker exists.
        let _ = secure_storage::background::run_or_inline(
            move |storage| {
                if let Err(e) = storage.write_value(SECURE_STORAGE_KEY, &json) {
                    log::error!("Failed to write agent provider secrets: {e:#}");
                    // Deliberately no revision bump on a failed write -- see the same
                    // note in `ai::api_keys::ApiKeyManager::write_keys_to_secure_storage`.
                    return;
                }
                // The keyring is shared with every other Zap process, so tell them to
                // re-read it. See `ai::secret_revision`.
                ::ai::secret_revision::bump_or_log();
            },
            ctx.secure_storage(),
        );
    }
}

impl Entity for AgentProviderSecrets {
    type Event = AgentProviderSecretsEvent;
}

impl SingletonEntity for AgentProviderSecrets {}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
