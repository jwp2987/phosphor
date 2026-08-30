use std::io;
use std::sync::Mutex;

/// Store user preferences in the Windows Registry.
/// Modeled after https://github.com/neovide/neovide/blob/main/src/windows_utils.rs .
use super::UserPreferences;
use windows_registry::{Key, CURRENT_USER};
use windows_result::HRESULT;

pub struct RegistryBackedPreferences {
    app_key_path: String,
    /// Caches the `HKCU\Software\Zap\<channel>` registry Key handle.
    ///
    /// At startup, Zap sequentially calls `read_value` for ~100 settings, and
    /// each one going through `CURRENT_USER.create(...)` to open/create the
    /// Key is a ~3ms synchronous system call, adding up to 300ms+ (the bulk
    /// of the cold-start `READ_USER_DEFAULTS_AND_INITIALIZE_SETTINGS` phase).
    /// Here we cache the Key from the first successful open, so subsequent
    /// reads reuse it directly, saving N-1 system calls.
    ///
    /// Uses `Mutex<Option<Key>>` instead of `OnceLock` because
    /// `windows_registry::Key` doesn't implement `Clone` and needs a mutable
    /// lock to `replace`/`take`; also the `read_value` interface is `&self`,
    /// so `RefCell` (which requires Sync) can't be used.
    cached_key: Mutex<Option<Key>>,
}

/// Root of the per-channel registry path, e.g. `HKCU\Software\Zap\Phosphor`.
///
/// **Kept as `Zap` on purpose — do not rebrand it.** This is a compatibility
/// surface, not an oversight: it is where existing Windows installs already
/// store their private preferences, and renaming it orphans every one of them
/// (settings silently reset to defaults, with the old values still on disk under
/// the old key). Changing it would need a migration that copies the old subtree
/// forward, which is a larger piece of work than a rename. See issue #636, which
/// filed this as a brand leak and concluded it should stay.
static WARP_REGISTRY_BASE_PATH: &str = "Software\\Zap\\";
pub const KEY_NOT_FOUND_ERR: HRESULT = HRESULT::from_win32(0x80070002);

impl RegistryBackedPreferences {
    /// Construct a separate registry path for each channel (stable, dev, local, etc.)
    pub fn new(app_name: &str) -> Self {
        let app_key_path = WARP_REGISTRY_BASE_PATH.to_owned() + app_name;
        // Warm up the Key at startup, so even the first setting read avoids a
        // synchronous system call.
        // A warm-up failure is not an error: `with_warp_registry` will retry when needed.
        let initial_key = CURRENT_USER
            .create(app_key_path.clone())
            .inspect_err(|e| {
                log::warn!("warp registry key prewarm failed (will retry on first access): {e:#}");
            })
            .ok();
        Self {
            app_key_path,
            cached_key: Mutex::new(initial_key),
        }
    }

    /// Operates on the cached Zap registry Key via a callback. The first call
    /// does `CURRENT_USER.create(...)`; subsequent calls reuse it directly.
    /// If the Key lock is poisoned (from an earlier panic), falls back to a
    /// one-off create without caching — degraded behavior, but no further panic.
    fn with_warp_registry<R>(
        &self,
        f: impl FnOnce(&Key) -> Result<R, super::Error>,
    ) -> Result<R, super::Error> {
        let mut guard = match self.cached_key.lock() {
            Ok(g) => g,
            // Mutex poisoned: take the one-off create path, don't cache — behavior
            // equivalent to the original.
            Err(_) => {
                let key = CURRENT_USER
                    .create(self.app_key_path.clone())
                    .map_err(|e| {
                        log::error!("unable to access Phosphor app key in Windows Registry: {e:#}");
                        super::Error::IoError(io::Error::from(e))
                    })?;
                return f(&key);
            }
        };

        if guard.is_none() {
            let key = CURRENT_USER
                .create(self.app_key_path.clone())
                .map_err(|e| {
                    log::error!("unable to access Phosphor app key in Windows Registry: {e:#}");
                    super::Error::IoError(io::Error::from(e))
                })?;
            *guard = Some(key);
        }

        // At this point guard is guaranteed Some; the unwrap is safe.
        f(guard.as_ref().expect("cached_key must be Some after init"))
    }
}

impl UserPreferences for RegistryBackedPreferences {
    fn read_value(&self, name: &str) -> Result<Option<String>, super::Error> {
        self.with_warp_registry(|key| Ok(key.get_string(name).ok()))
    }

    fn write_value(&self, key: &str, value: String) -> Result<(), super::Error> {
        self.with_warp_registry(|reg| {
            reg.set_string(key, value.as_str())
                .map_err(|e| super::Error::from(io::Error::from(e)))
        })
    }

    fn remove_value(&self, key: &str) -> Result<(), super::Error> {
        self.with_warp_registry(|reg| match reg.remove_value(key) {
            Ok(_) => Ok(()),
            // If the key doesn't exist, then treat removal of that nonexistent key as a success.
            Err(e) if e.code() == KEY_NOT_FOUND_ERR => Ok(()),
            Err(e) => Err(super::Error::from(io::Error::from(e))),
        })
    }
}
