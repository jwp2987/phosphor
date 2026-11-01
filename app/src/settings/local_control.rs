//! Secure local setting that gates local control.
//!
//! This setting is local-only, kept out of the user-visible settings file, and
//! persisted through the platform secure-storage provider. It is the
//! authoritative enablement bit for local control.
//!
//! Ported from Warp's `app/src/settings/local_control.rs` at the pinned
//! oracle (`42effe840`, Warp `2026.08.12` stable — see `ORACLE.md`).
//! The oracle implements this against a `SecureSetting` trait defined in
//! `crates/settings`; that trait does not exist in this fork and adding it is
//! out of this change's scope (`app/src/settings/` only), so the same
//! read/write/clear-through-secure-storage behavior is inlined here as free
//! functions instead of default trait methods. The observable behavior is
//! identical: the setting is private, never cloud-synced, and persisted to
//! secure storage rather than the settings file, mirroring the existing
//! `network_secrets::ProxyCredentials` pattern in this crate.
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use settings::macros::define_settings_group;
use settings::{Setting, SupportedPlatforms, SyncToCloud};
use warp_core::channel::{Channel, ChannelState};
use warpui::{AppContext, ModelContext};
use warpui_extras::secure_storage::{self, AppContextExt as _};

use crate::report_error;

const LOCAL_CONTROL_MODE_STORAGE_KEY: &str = "LocalControlMode";

/// User-selected local-control availability.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Deserialize,
    Eq,
    PartialEq,
    schemars::JsonSchema,
    Serialize,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Whether local control is enabled.",
    rename_all = "snake_case"
)]
pub enum LocalControlMode {
    #[default]
    Disabled,
    Enabled,
}

/// Channel-based default: local control is on for internal dogfood builds and
/// off for public channels, where users must opt in through Settings > Scripting.
fn default_mode_for_channel(channel: Channel) -> LocalControlMode {
    if channel.is_dogfood() {
        LocalControlMode::Enabled
    } else {
        LocalControlMode::Disabled
    }
}

impl LocalControlMode {
    pub const ALL: [Self; 2] = [Self::Disabled, Self::Enabled];

    pub fn is_enabled(self) -> bool {
        matches!(self, Self::Enabled)
    }

    pub fn as_dropdown_label(self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::Enabled => "Enabled",
        }
    }
}

/// Reads and deserializes [`LocalControlMode`] from secure storage.
///
/// Missing, unreadable, or malformed values return `None`, allowing the
/// setting to fail closed to its default value.
fn read_local_control_mode_from_secure_storage(
    storage: &dyn secure_storage::SecureStorage,
) -> Option<LocalControlMode> {
    let value = match storage.read_value(LOCAL_CONTROL_MODE_STORAGE_KEY) {
        Ok(value) => value,
        Err(secure_storage::Error::NotFound) => return None,
        Err(err) => {
            report_error!(anyhow::Error::new(err)
                .context("Failed to read local control mode from secure storage"));
            return None;
        }
    };
    match serde_json::from_str(&value)
        .context("Failed to deserialize local control mode from secure storage")
    {
        Ok(value) => Some(value),
        Err(err) => {
            report_error!(err);
            None
        }
    }
}

/// Persists [`LocalControlMode`] to secure storage if the value changed.
/// Returns whether a write occurred.
fn write_local_control_mode_to_secure_storage(
    new_value: &LocalControlMode,
    storage: &dyn secure_storage::SecureStorage,
) -> Result<bool> {
    let stored_value_matches = match storage.read_value(LOCAL_CONTROL_MODE_STORAGE_KEY) {
        Ok(stored) => serde_json::from_str::<LocalControlMode>(&stored)
            .is_ok_and(|stored| stored == *new_value),
        Err(secure_storage::Error::NotFound) => false,
        Err(err) => {
            return Err(anyhow::anyhow!(err))
                .context("Failed to read existing local control mode from secure storage");
        }
    };
    if stored_value_matches {
        return Ok(false);
    }
    let serialized = serde_json::to_string(new_value)
        .context("Failed to serialize local control mode for secure storage")?;
    // Uses the owner-only-fallback write path since this setting gates a
    // local-automation surface and should not be readable by other users on
    // platforms that fall back to a file-backed store.
    storage
        .write_value_with_owner_only_fallback(LOCAL_CONTROL_MODE_STORAGE_KEY, &serialized)
        .context("Failed to write local control mode to secure storage")?;
    Ok(true)
}

/// Removes [`LocalControlMode`] from secure storage.
fn clear_local_control_mode_from_secure_storage(
    storage: &dyn secure_storage::SecureStorage,
) -> Result<()> {
    match storage.remove_value(LOCAL_CONTROL_MODE_STORAGE_KEY) {
        Ok(()) | Err(secure_storage::Error::NotFound) => Ok(()),
        Err(err) => Err(anyhow::anyhow!(err))
            .context("Failed to clear local control mode from secure storage"),
    }
}

define_settings_group!(LocalControlSettings, settings: [
    local_control_mode: LocalControlModeSetting,
]);

/// Setting wrapper for the authoritative local-control mode.
pub struct LocalControlModeSetting {
    inner: LocalControlMode,
    is_explicitly_set: bool,
    /// The stored value is being read off the UI thread (#1309) and has not landed yet. Until it
    /// does, `inner` is `Disabled` (fail closed: the stored value may be a deliberate "off" on a
    /// channel whose default is "on"). Cleared when the read lands, or when the user changes the
    /// setting first (their choice wins over a stale read).
    awaiting_load: bool,
}

impl LocalControlModeSetting {
    /// Adopts the value read from secure storage, unless the user changed the setting meanwhile.
    fn finish_async_load(
        &mut self,
        stored: Option<LocalControlMode>,
        ctx: &mut ModelContext<LocalControlSettings>,
    ) {
        if !self.awaiting_load {
            return;
        }
        self.awaiting_load = false;
        let explicitly_set = stored.is_some();
        let value = stored.unwrap_or_else(Self::default_value);
        if let Err(err) = self.load_value(value, explicitly_set, ctx) {
            report_error!(err);
        }
    }

    fn emit_changed(
        ctx: &mut ModelContext<LocalControlSettings>,
        change_event_reason: settings::ChangeEventReason,
    ) {
        ctx.emit(LocalControlSettingsChangedEvent::LocalControlModeSetting {
            change_event_reason,
        });
    }
}

/// Outcome of a write queued on the secure-storage worker: `Ok(changed_in_storage)`.
pub(crate) type PendingModeWrite = secure_storage::background::BackgroundResult<Result<bool>>;

impl LocalControlModeSetting {
    /// [`Setting::set_value`], but when the write is queued off-thread the caller gets its
    /// eventual outcome, so a UI can surface a failure (the in-memory value is already updated).
    /// `None` means the write already completed synchronously (an `Err` is returned directly).
    pub(crate) fn set_value_tracked(
        &mut self,
        new_value: LocalControlMode,
        ctx: &mut ModelContext<LocalControlSettings>,
    ) -> Result<Option<PendingModeWrite>> {
        // The compare-and-write runs on the secure-storage worker, ordered after the startup read
        // and any earlier write.
        self.awaiting_load = false;
        let to_store = new_value;
        let (changed_in_storage, pending) = match secure_storage::background::run_or_inline(
            move |storage| write_local_control_mode_to_secure_storage(&to_store, storage),
            ctx.secure_storage(),
        ) {
            Ok(result) => (!self.is_explicitly_set, Some(result)),
            Err(result) => (result?, None),
        };
        if self.value() != &new_value || changed_in_storage {
            self.inner = self.validate(new_value);
            self.is_explicitly_set = true;
            Self::emit_changed(ctx, settings::ChangeEventReason::LocalChange);
        }
        Ok(pending)
    }
}

impl Setting for LocalControlModeSetting {
    type Group = LocalControlSettings;
    type Value = LocalControlMode;

    fn new(value: Option<Self::Value>) -> Self {
        match value {
            Some(value) => Self {
                inner: value,
                is_explicitly_set: true,
                awaiting_load: false,
            },
            None => Self {
                inner: Self::default_value(),
                is_explicitly_set: false,
                awaiting_load: false,
            },
        }
    }

    fn setting_name() -> &'static str {
        "LocalControlModeSetting"
    }

    fn storage_key() -> &'static str {
        LOCAL_CONTROL_MODE_STORAGE_KEY
    }

    fn supported_platforms() -> SupportedPlatforms {
        SupportedPlatforms::DESKTOP
    }

    fn sync_to_cloud() -> SyncToCloud {
        SyncToCloud::Never
    }

    fn is_private() -> bool {
        true
    }

    fn value(&self) -> &Self::Value {
        &self.inner
    }

    fn clear_value(&mut self, ctx: &mut ModelContext<Self::Group>) -> Result<()> {
        self.awaiting_load = false;
        match secure_storage::background::run_or_inline(
            clear_local_control_mode_from_secure_storage,
            ctx.secure_storage(),
        ) {
            Ok(result) => report_background_failure(result, ctx),
            Err(result) => result?,
        }
        self.inner = self.validate(Self::default_value());
        self.is_explicitly_set = false;
        Self::emit_changed(ctx, settings::ChangeEventReason::Clear);
        Ok(())
    }

    fn load_value(
        &mut self,
        new_value: Self::Value,
        explicitly_set: bool,
        ctx: &mut ModelContext<Self::Group>,
    ) -> Result<()> {
        let validated = self.validate(new_value);
        if self.value() != &validated || self.is_explicitly_set != explicitly_set {
            self.inner = validated;
            self.is_explicitly_set = explicitly_set;
            Self::emit_changed(ctx, settings::ChangeEventReason::LocalChange);
        }
        Ok(())
    }

    fn set_value_from_cloud_sync(
        &mut self,
        _: Self::Value,
        _: &mut ModelContext<Self::Group>,
    ) -> Result<()> {
        Ok(())
    }

    fn set_value(
        &mut self,
        new_value: Self::Value,
        ctx: &mut ModelContext<Self::Group>,
    ) -> Result<()> {
        if let Some(pending) = self.set_value_tracked(new_value, ctx)? {
            report_background_failure(pending, ctx);
        }
        Ok(())
    }

    fn default_value() -> Self::Value {
        default_mode_for_channel(ChannelState::channel())
    }

    fn new_from_storage(ctx: &mut AppContext) -> Self {
        if secure_storage::background::is_available() {
            // Read off the UI thread; `begin_async_load` starts it once the group is registered.
            return Self {
                inner: LocalControlMode::Disabled,
                is_explicitly_set: false,
                awaiting_load: true,
            };
        }
        Self::new(read_local_control_mode_from_secure_storage(
            ctx.secure_storage(),
        ))
    }

    fn is_supported_on_current_platform(&self) -> bool {
        SupportedPlatforms::DESKTOP.matches_current_platform()
    }

    fn is_value_explicitly_set(&self) -> bool {
        self.is_explicitly_set
    }
}

impl std::ops::Deref for LocalControlModeSetting {
    type Target = LocalControlMode;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

/// Surfaces the outcome of a write that ran on the secure-storage worker.
fn report_background_failure<T: Send + 'static>(
    result: secure_storage::background::BackgroundResult<Result<T>>,
    ctx: &mut ModelContext<LocalControlSettings>,
) {
    ctx.spawn(result.wait(), |_, outcome, _| {
        if let Some(Err(err)) = outcome {
            report_error!(err);
        }
    });
}

/// Starts the off-thread read of the stored mode, if `new_from_storage` deferred it (#1309).
/// Call right after `LocalControlSettings::register`.
pub(crate) fn begin_async_load(ctx: &mut AppContext) {
    use warpui::SingletonEntity as _;
    if !LocalControlSettings::as_ref(ctx)
        .local_control_mode
        .awaiting_load
    {
        return;
    }
    LocalControlSettings::handle(ctx).update(ctx, |group, mctx| {
        match secure_storage::background::run_or_inline(
            read_local_control_mode_from_secure_storage,
            mctx.secure_storage(),
        ) {
            Ok(result) => {
                mctx.spawn(result.wait(), |settings, stored, ctx| {
                    if let Some(stored) = stored {
                        settings.local_control_mode.finish_async_load(stored, ctx);
                    }
                });
            }
            Err(stored) => group.local_control_mode.finish_async_load(stored, mctx),
        }
    });
}

impl LocalControlSettings {
    pub fn mode(&self) -> LocalControlMode {
        *self.local_control_mode
    }

    pub fn is_enabled(&self) -> bool {
        self.mode().is_enabled()
    }
}

#[cfg(test)]
#[path = "local_control_tests.rs"]
mod tests;
