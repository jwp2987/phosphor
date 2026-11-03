// Ported from Warp's `app/src/settings/local_control_tests.rs` at the pinned
// oracle (`02b53fcd8`, Warp `2026.07.29.09.05` stable — see `ORACLE.md`).
// Adapted to this fork's inlined secure-storage helpers (see `local_control.rs`
// header comment) instead of the oracle's `SecureSetting` trait; behavior
// asserted is otherwise identical.

use std::collections::HashMap;
use std::sync::Mutex;

use settings::{PrivatePreferences, PublicPreferences, Setting as _, SettingsManager, SyncToCloud};
use warp_core::channel::{Channel, ChannelState};
use warpui::SingletonEntity as _;
use warpui_extras::secure_storage::{self, AppContextExt as _};
use warpui_extras::user_preferences;

use super::{
    default_mode_for_channel, LocalControlMode, LocalControlModeSetting, LocalControlSettings,
};

#[derive(Default)]
struct InMemorySecureStorage {
    values: Mutex<HashMap<String, String>>,
}

impl secure_storage::SecureStorage for InMemorySecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), secure_storage::Error> {
        match self.values.lock() {
            Ok(mut values) => {
                values.insert(key.to_owned(), value.to_owned());
                Ok(())
            }
            Err(err) => Err(secure_storage::Error::Unknown(anyhow::anyhow!(
                err.to_string()
            ))),
        }
    }

    fn read_value(&self, key: &str) -> Result<String, secure_storage::Error> {
        match self.values.lock() {
            Ok(values) => values
                .get(key)
                .cloned()
                .ok_or(secure_storage::Error::NotFound),
            Err(err) => Err(secure_storage::Error::Unknown(anyhow::anyhow!(
                err.to_string()
            ))),
        }
    }

    fn remove_value(&self, key: &str) -> Result<(), secure_storage::Error> {
        match self.values.lock() {
            Ok(mut values) => {
                values.remove(key);
                Ok(())
            }
            Err(err) => Err(secure_storage::Error::Unknown(anyhow::anyhow!(
                err.to_string()
            ))),
        }
    }
}

fn default_settings() -> LocalControlSettings {
    LocalControlSettings {
        local_control_mode: LocalControlModeSetting::new(None),
    }
}

#[test]
fn default_mode_is_enabled_only_on_dogfood_channels() {
    assert_eq!(
        default_mode_for_channel(Channel::Dev),
        LocalControlMode::Enabled
    );
    assert_eq!(
        default_mode_for_channel(Channel::Local),
        LocalControlMode::Enabled
    );
    for channel in [
        Channel::Stable,
        Channel::Preview,
        Channel::Oss,
        Channel::Integration,
    ] {
        assert_eq!(
            default_mode_for_channel(channel),
            LocalControlMode::Disabled,
            "{channel} must require explicit opt-in"
        );
    }
}

#[test]
fn unset_mode_follows_channel_default() {
    let settings = default_settings();

    assert_eq!(LocalControlMode::default(), LocalControlMode::Disabled);
    assert_eq!(
        settings.mode(),
        default_mode_for_channel(ChannelState::channel())
    );
}

#[test]
fn mode_is_persisted_to_secure_storage() {
    warpui::App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| {
                PublicPreferences::new(
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
                )
            });
            ctx.add_singleton_model(|_| {
                PrivatePreferences::new(
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
                )
            });
            ctx.add_singleton_model(|_| SettingsManager::default());
            ctx.add_singleton_model(|_| -> secure_storage::Model {
                Box::<InMemorySecureStorage>::default()
            });
            LocalControlSettings::register(ctx);
        });

        app.update(|ctx| {
            LocalControlSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .local_control_mode
                    .set_value(LocalControlMode::Enabled, ctx)
            })
        })
        .expect("setting update should succeed");

        app.read(|ctx| {
            let stored = ctx
                .secure_storage()
                .read_value(LocalControlModeSetting::storage_key())
                .expect("local-control mode should be stored securely");
            let mode = serde_json::from_str::<LocalControlMode>(&stored)
                .expect("stored local-control mode should deserialize");
            assert_eq!(mode, LocalControlMode::Enabled);

            let private_value = LocalControlModeSetting::preferences_for_setting(ctx)
                .read_value(LocalControlModeSetting::storage_key())
                .expect("private preferences should be readable");
            assert!(private_value.is_none());
        });
    });
}

#[test]
fn mode_does_not_migrate_from_private_preferences() {
    warpui::App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| {
                PublicPreferences::new(
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
                )
            });
            ctx.add_singleton_model(|_| {
                PrivatePreferences::new(
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
                )
            });
            ctx.add_singleton_model(|_| SettingsManager::default());
            ctx.add_singleton_model(|_| -> secure_storage::Model {
                Box::<InMemorySecureStorage>::default()
            });
            LocalControlModeSetting::preferences_for_setting(ctx)
                .write_value(
                    LocalControlModeSetting::storage_key(),
                    serde_json::to_string(&LocalControlMode::Enabled).expect("mode serializes"),
                )
                .expect("private preference is writable");
            LocalControlSettings::register(ctx);
        });

        app.read(|ctx| {
            assert_eq!(
                LocalControlSettings::as_ref(ctx).mode(),
                default_mode_for_channel(ChannelState::channel())
            );
            let private_value = LocalControlModeSetting::preferences_for_setting(ctx)
                .read_value(LocalControlModeSetting::storage_key())
                .expect("private preference is readable");
            assert!(private_value.is_some());
        });
    });
}

#[test]
fn mode_is_private_and_never_cloud_synced() {
    assert_eq!(LocalControlModeSetting::sync_to_cloud(), SyncToCloud::Never);
    assert!(LocalControlModeSetting::is_private());
}

#[test]
fn cloud_sync_cannot_disable_local_control() {
    warpui::App::test((), |mut app| async move {
        app.update(|ctx| {
            ctx.add_singleton_model(|_| {
                PublicPreferences::new(
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
                )
            });
            ctx.add_singleton_model(|_| {
                PrivatePreferences::new(
                    Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
                )
            });
            ctx.add_singleton_model(|_| SettingsManager::default());
            ctx.add_singleton_model(|_| -> secure_storage::Model {
                Box::<InMemorySecureStorage>::default()
            });
            LocalControlSettings::register(ctx);
        });

        app.update(|ctx| {
            LocalControlSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .local_control_mode
                    .set_value(LocalControlMode::Enabled, ctx)
            })
        })
        .expect("local control should enable");

        app.update(|ctx| {
            LocalControlSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .local_control_mode
                    .set_value_from_cloud_sync(LocalControlMode::Disabled, ctx)
            })
        })
        .expect("cloud sync update should be ignored without error");

        app.read(|ctx| {
            let settings = LocalControlSettings::as_ref(ctx);
            assert_eq!(settings.mode(), LocalControlMode::Enabled);
            let stored = ctx
                .secure_storage()
                .read_value(LocalControlModeSetting::storage_key())
                .expect("explicitly enabled mode should remain stored securely");
            let mode = serde_json::from_str::<LocalControlMode>(&stored)
                .expect("stored local-control mode should deserialize");
            assert_eq!(mode, LocalControlMode::Enabled);
        });
    });
}

// ── off-thread startup read (#1309) ─────────────────────────────

struct SendStore(
    std::sync::Arc<Mutex<HashMap<String, String>>>,
    std::thread::ThreadId,
);

impl secure_storage::SecureStorage for SendStore {
    fn write_value(&self, key: &str, value: &str) -> Result<(), secure_storage::Error> {
        self.0
            .lock()
            .unwrap()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn read_value(&self, key: &str) -> Result<String, secure_storage::Error> {
        assert_ne!(
            std::thread::current().id(),
            self.1,
            "the keychain must not be read on the UI thread"
        );
        std::thread::sleep(std::time::Duration::from_millis(30));
        self.0
            .lock()
            .unwrap()
            .get(key)
            .cloned()
            .ok_or(secure_storage::Error::NotFound)
    }
    fn remove_value(&self, key: &str) -> Result<(), secure_storage::Error> {
        self.0.lock().unwrap().remove(key);
        Ok(())
    }
}

fn register_async_fixture(
    app: &mut warpui::App,
    stored: Option<LocalControlMode>,
) -> std::sync::Arc<Mutex<HashMap<String, String>>> {
    let data = std::sync::Arc::new(Mutex::new(HashMap::new()));
    if let Some(mode) = stored {
        data.lock().unwrap().insert(
            LocalControlModeSetting::storage_key().to_owned(),
            serde_json::to_string(&mode).unwrap(),
        );
    }
    let main = std::thread::current().id();
    let for_factory = data.clone();
    secure_storage::background::set_factory_for_test(Some(std::sync::Arc::new(move || {
        Box::new(SendStore(for_factory.clone(), main)) as secure_storage::Model
    })));
    app.update(|ctx| {
        ctx.add_singleton_model(|_| {
            PublicPreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        ctx.add_singleton_model(|_| {
            PrivatePreferences::new(
                Box::<user_preferences::in_memory::InMemoryPreferences>::default(),
            )
        });
        ctx.add_singleton_model(|_| SettingsManager::default());
        ctx.add_singleton_model(|_| -> secure_storage::Model {
            Box::<InMemorySecureStorage>::default()
        });
        LocalControlSettings::register(ctx);
        super::begin_async_load(ctx);
    });
    data
}

async fn wait_for(mut done: impl FnMut() -> bool) {
    for _ in 0..400 {
        if done() {
            return;
        }
        warpui::r#async::Timer::after(std::time::Duration::from_millis(10)).await;
    }
    panic!("condition not reached");
}

#[test]
fn stored_mode_arrives_after_startup_and_until_then_local_control_is_off() {
    warpui::App::test((), |mut app| async move {
        register_async_fixture(&mut app, Some(LocalControlMode::Enabled));
        // Fail closed until the read lands, whatever the channel default is.
        assert!(!app.read(|ctx| LocalControlSettings::as_ref(ctx).is_enabled()));
        wait_for(|| app.read(|ctx| LocalControlSettings::as_ref(ctx).is_enabled())).await;
        secure_storage::background::set_factory_for_test(None);
    });
}

#[test]
fn a_change_made_before_the_read_lands_wins_over_the_read() {
    warpui::App::test((), |mut app| async move {
        let data = register_async_fixture(&mut app, Some(LocalControlMode::Enabled));
        app.update(|ctx| {
            LocalControlSettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .local_control_mode
                    .set_value(LocalControlMode::Disabled, ctx)
            })
        })
        .expect("update should succeed");
        let key = LocalControlModeSetting::storage_key();
        wait_for(|| {
            data.lock()
                .unwrap()
                .get(key)
                .is_some_and(|v| v.contains("Disabled"))
        })
        .await;
        // Give the (now stale) read a chance to land; it must not flip the setting back.
        warpui::r#async::Timer::after(std::time::Duration::from_millis(100)).await;
        assert!(!app.read(|ctx| LocalControlSettings::as_ref(ctx).is_enabled()));
        secure_storage::background::set_factory_for_test(None);
    });
}
