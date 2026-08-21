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
fn read_local_control_mode_from_secure_storage(ctx: &AppContext) -> Option<LocalControlMode> {
    let value = match ctx
        .secure_storage()
        .read_value(LOCAL_CONTROL_MODE_STORAGE_KEY)
    {
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
    ctx: &AppContext,
) -> Result<bool> {
    let stored_value_matches = match ctx
        .secure_storage()
        .read_value(LOCAL_CONTROL_MODE_STORAGE_KEY)
    {
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
    ctx.secure_storage()
        .write_value_with_owner_only_fallback(LOCAL_CONTROL_MODE_STORAGE_KEY, &serialized)
        .context("Failed to write local control mode to secure storage")?;
    Ok(true)
}

/// Removes [`LocalControlMode`] from secure storage.
fn clear_local_control_mode_from_secure_storage(ctx: &AppContext) -> Result<()> {
    match ctx
        .secure_storage()
        .remove_value(LOCAL_CONTROL_MODE_STORAGE_KEY)
    {
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
}

impl LocalControlModeSetting {
    fn emit_changed(
        ctx: &mut ModelContext<LocalControlSettings>,
        change_event_reason: settings::ChangeEventReason,
    ) {
        ctx.emit(LocalControlSettingsChangedEvent::LocalControlModeSetting {
            change_event_reason,
        });
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
            },
            None => Self {
                inner: Self::default_value(),
                is_explicitly_set: false,
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
        clear_local_control_mode_from_secure_storage(ctx)?;
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
        let changed_in_storage = write_local_control_mode_to_secure_storage(&new_value, ctx)?;
        if self.value() != &new_value || changed_in_storage {
            self.inner = self.validate(new_value);
            self.is_explicitly_set = true;
            Self::emit_changed(ctx, settings::ChangeEventReason::LocalChange);
        }
        Ok(())
    }

    fn default_value() -> Self::Value {
        default_mode_for_channel(ChannelState::channel())
    }

    fn new_from_storage(ctx: &mut AppContext) -> Self {
        Self::new(read_local_control_mode_from_secure_storage(ctx))
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
