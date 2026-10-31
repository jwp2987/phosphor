//! Provides an application-agnostic D-Bus client for retrieving the
//! desktop environment's appearance settings.

use std::ops::Deref as _;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use futures::StreamExt as _;
use winit::event_loop::EventLoopProxy;
use zbus::{proxy, zvariant};

use crate::{
    platform::SystemTheme,
    r#async::{executor::Background, FutureExt as _},
    windowing::winit::app::CustomEvent,
};

const COLOR_SCHEME_SETTINGS_NAMESPACE: &str = "org.freedesktop.appearance";
const COLOR_SCHEME_SETTINGS_KEY: &str = "color-scheme";

/// Values used by the desktop environment to encode the user's
/// system color scheme preference.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, zbus::zvariant::Type, PartialEq)]
enum SystemColorScheme {
    #[default]
    NoPreference = 0,
    Dark = 1,
    Light = 2,
}

impl From<&u32> for SystemColorScheme {
    fn from(value: &u32) -> SystemColorScheme {
        match value {
            0 => SystemColorScheme::NoPreference,
            1 => SystemColorScheme::Dark,
            2 => SystemColorScheme::Light,
            _ => SystemColorScheme::NoPreference,
        }
    }
}

impl From<&str> for SystemColorScheme {
    fn from(value: &str) -> SystemColorScheme {
        match value {
            "prefer-dark" => SystemColorScheme::Dark,
            "prefer-light" => SystemColorScheme::Light,
            _ => SystemColorScheme::NoPreference,
        }
    }
}

impl From<&zvariant::Str<'_>> for SystemColorScheme {
    fn from(value: &zvariant::Str) -> SystemColorScheme {
        SystemColorScheme::from(value.as_str())
    }
}

impl From<&zvariant::Value<'_>> for SystemColorScheme {
    fn from(value: &zvariant::Value) -> SystemColorScheme {
        match value {
            zvariant::Value::U32(u) => SystemColorScheme::from(u),
            zvariant::Value::Str(s) => SystemColorScheme::from(s),
            zvariant::Value::Value(boxed_v) => match boxed_v.downcast_ref::<u32>() {
                Ok(v) => SystemColorScheme::from(&v),
                Err(err) => {
                    log::error!(
                            "D-Bus inner variant type {:#?}: {:#?} could not be converted to SystemThemePreference: {err:#}",
                            value.value_signature(),
                            value
                        );
                    SystemColorScheme::NoPreference
                }
            },
            _ => {
                log::error!(
                    "D-Bus outer variant type {:#?}: {:#?} could not be converted to SystemThemePreference",
                    value.value_signature(),
                    value
                );
                SystemColorScheme::NoPreference
            }
        }
    }
}

impl From<&zvariant::OwnedValue> for SystemColorScheme {
    fn from(owned_value: &zvariant::OwnedValue) -> SystemColorScheme {
        SystemColorScheme::from(owned_value.deref())
    }
}

impl From<SystemColorScheme> for SystemTheme {
    fn from(os_value: SystemColorScheme) -> SystemTheme {
        match os_value {
            SystemColorScheme::Dark => SystemTheme::Dark,
            SystemColorScheme::Light => SystemTheme::Light,
            SystemColorScheme::NoPreference => SystemTheme::default(),
        }
    }
}

/// A D-Bus client for connecting to the desktop settings.
#[proxy(
    interface = "org.freedesktop.portal.Settings",
    default_service = "org.freedesktop.portal.Desktop",
    default_path = "/org/freedesktop/portal/desktop"
)]
trait DesktopSettings {
    fn read(&self, namespace: &str, key: &str) -> zbus::fdo::Result<zvariant::OwnedValue>;

    #[zbus(signal)]
    fn setting_changed(
        &self,
        interface_name: &str,
        setting_name: &str,
        new_setting_value: zvariant::Value<'_>,
    ) -> zbus::fdo::Result<()>;
}

/// Sets up a background task to listen to desktop settings change events sent
/// over dbus and inject events into the winit EventLoop accordingly.
pub fn watch_desktop_settings_changes(
    event_proxy: EventLoopProxy<CustomEvent>,
    background: &Background,
) {
    background
        .spawn(async move {
            if let Err(err) = watch_desktop_settings_changes_internal(event_proxy).await {
                log::warn!(
                    "Encountered error while watching for desktop settings change events: {err:#}"
                );
            }
        })
        .detach();
}

async fn watch_desktop_settings_changes_internal(
    event_proxy: EventLoopProxy<CustomEvent>,
) -> zbus::Result<()> {
    let connection = zbus::Connection::session().await?;
    let desktop_settings_proxy = DesktopSettingsProxy::new(&connection).await?;
    let mut stream = desktop_settings_proxy.receive_setting_changed().await?;
    while let Some(msg) = stream.next().await {
        let Ok(args) = msg.args() else {
            log::warn!("appearance settings signal should have arguments");
            continue;
        };
        // As of now, we are only interested in system color scheme changes.
        // In the future, we may check for other types of signals.
        if let (&COLOR_SCHEME_SETTINGS_NAMESPACE, &COLOR_SCHEME_SETTINGS_KEY) =
            (args.interface_name(), args.setting_name())
        {
            // Refresh the cache before telling the UI to re-read it.
            match desktop_settings_proxy
                .read(COLOR_SCHEME_SETTINGS_NAMESPACE, COLOR_SCHEME_SETTINGS_KEY)
                .await
            {
                Ok(value) => THEME_CACHE.set(SystemColorScheme::from(&value).into()),
                Err(err) => {
                    log::warn!("Failed to re-read system color scheme: {err:#}");
                    THEME_CACHE.set(SystemColorScheme::from(args.new_setting_value()).into());
                }
            }
            let _ = event_proxy.send_event(CustomEvent::SystemThemeChanged);
        }
    }
    Ok(())
}

/// How long the UI thread may wait for the first (prefetched) answer. Only reached when the
/// user asked to follow the system theme (see `derived_theme_kind_with`) and only until the
/// background prefetch lands (~5-10 ms on a desktop with a portal); a missing or slow portal
/// costs at most this once, instead of the old 200 ms on every lookup.
const FIRST_ANSWER_WAIT: Duration = Duration::from_millis(50);

/// How long the background prefetch waits for the portal before giving up.
const PREFETCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Last known system color scheme, filled by [`prefetch_system_theme`] and kept current by
/// the `SettingChanged` watcher, so lookups never touch D-Bus on the calling thread.
#[derive(Debug, Default)]
struct ThemeCache {
    value: Mutex<Option<SystemTheme>>,
    ready: Condvar,
}

impl ThemeCache {
    const fn new() -> Self {
        Self {
            value: Mutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn set(&self, theme: SystemTheme) {
        if let Ok(mut v) = self.value.lock() {
            *v = Some(theme);
        }
        self.ready.notify_all();
    }

    fn get(&self) -> Option<SystemTheme> {
        self.value.lock().ok().and_then(|v| *v)
    }

    /// Returns the cached value, waiting up to `timeout` for the first one to arrive.
    fn get_or_wait(&self, timeout: Duration) -> Option<SystemTheme> {
        let guard = self.value.lock().ok()?;
        if guard.is_some() {
            return *guard;
        }
        let (guard, _) = self
            .ready
            .wait_timeout_while(guard, timeout, |v| v.is_none())
            .ok()?;
        *guard
    }
}

static THEME_CACHE: ThemeCache = ThemeCache::new();

/// Returns the system color scheme from the cache. Never queries D-Bus on the calling
/// thread; errors only if the first background answer has not arrived within
/// [`FIRST_ANSWER_WAIT`] (the caller falls back to light and is told to re-read via
/// `SystemThemeChanged` once the answer lands).
pub fn get_system_theme() -> Result<SystemTheme, zbus::Error> {
    THEME_CACHE.get_or_wait(FIRST_ANSWER_WAIT).ok_or_else(|| {
        zbus::Error::from(zbus::fdo::Error::TimedOut(
            "system color scheme not fetched yet".to_owned(),
        ))
    })
}

/// Queries the system color scheme once, off-thread, at process start and caches it. If the
/// answer differs from the light fallback callers used while waiting, injects a
/// `SystemThemeChanged` event so the UI re-reads it.
pub fn prefetch_system_theme(event_proxy: EventLoopProxy<CustomEvent>, background: &Background) {
    background
        .spawn(async move {
            let theme = match query_system_theme_from_dbus()
                .with_timeout(PREFETCH_TIMEOUT)
                .await
            {
                Ok(Ok(theme)) => theme,
                Ok(Err(err)) => {
                    log::info!("Unable to fetch Linux system color scheme: {err:#}");
                    SystemTheme::Light
                }
                Err(_) => {
                    log::info!(
                        "Unable to fetch Linux system color scheme: no response within {PREFETCH_TIMEOUT:?}"
                    );
                    SystemTheme::Light
                }
            };
            THEME_CACHE.set(theme);
            if theme != SystemTheme::Light {
                let _ = event_proxy.send_event(CustomEvent::SystemThemeChanged);
            }
        })
        .detach();
}

/// Queries the current D-Bus session bus to get the system color scheme.
async fn query_system_theme_from_dbus() -> Result<SystemTheme, zbus::Error> {
    let client_conn = zbus::Connection::session().await?;
    let settings_proxy = DesktopSettingsProxy::new(&client_conn).await?;
    let owned_val = settings_proxy
        .read(COLOR_SCHEME_SETTINGS_NAMESPACE, COLOR_SCHEME_SETTINGS_KEY)
        .await?;
    Ok(SystemColorScheme::from(&owned_val).into())
}

#[cfg(test)]
#[path = "desktop_settings_tests.rs"]
mod tests;
