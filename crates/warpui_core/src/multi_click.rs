//! The single source of truth for the multi-click (double/triple-click) interval.
//!
//! Read from the OS where it is cheaply available, otherwise [`DEFAULT_MULTI_CLICK_INTERVAL`],
//! and cached for the life of the process (the OS value is read once, on first use, which is
//! the first click or earlier). macOS does not use this: AppKit hands every mouse event its own
//! `clickCount` (`warpui::platform::mac::event`), computed by the OS from the user's setting.

use std::sync::OnceLock;
use std::time::Duration;

/// Used when the OS value cannot be read: roughly the standard desktop double-click window.
pub const DEFAULT_MULTI_CLICK_INTERVAL: Duration = Duration::from_millis(400);

/// Values outside this range are treated as unreadable/garbage and replaced by the default.
const SANE_RANGE_MS: std::ops::RangeInclusive<u64> = 100..=2000;

/// The longest gap between consecutive presses of the same button that still counts as part of
/// the same multi-click.
pub fn multi_click_interval() -> Duration {
    // Tests that drive click timing must not depend on the machine's settings.
    #[cfg(test)]
    if true {
        return DEFAULT_MULTI_CLICK_INTERVAL;
    }
    static INTERVAL: OnceLock<Duration> = OnceLock::new();
    *INTERVAL.get_or_init(|| {
        let interval = sanitize(read_os_interval_ms());
        log::debug!("multi-click interval: {interval:?}");
        interval
    })
}

fn sanitize(os_ms: Option<u64>) -> Duration {
    match os_ms {
        Some(ms) if SANE_RANGE_MS.contains(&ms) => Duration::from_millis(ms),
        _ => DEFAULT_MULTI_CLICK_INTERVAL,
    }
}

#[cfg(windows)]
fn read_os_interval_ms() -> Option<u64> {
    #[link(name = "user32")]
    extern "system" {
        fn GetDoubleClickTime() -> u32;
    }
    // Safety: takes no arguments and has no preconditions.
    Some(u64::from(unsafe { GetDoubleClickTime() }))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn read_os_interval_ms() -> Option<u64> {
    // GTK's `gtk-double-click-time` in the user's settings file: a cheap file read, unlike
    // spawning gsettings. Desktops that keep the value only in dconf/xsettings fall back to the
    // default; there is no cheap synchronous source for those.
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config")))?;
    ["gtk-3.0", "gtk-4.0"].iter().find_map(|dir| {
        let text = std::fs::read_to_string(base.join(dir).join("settings.ini")).ok()?;
        parse_gtk_double_click_time(&text)
    })
}

#[cfg(not(any(windows, all(unix, not(target_os = "macos")))))]
fn read_os_interval_ms() -> Option<u64> {
    None
}

#[cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]
fn parse_gtk_double_click_time(settings_ini: &str) -> Option<u64> {
    settings_ini.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == "gtk-double-click-time").then(|| value.trim().parse().ok())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insane_or_missing_os_values_fall_back_to_the_default() {
        assert_eq!(sanitize(None), DEFAULT_MULTI_CLICK_INTERVAL);
        assert_eq!(sanitize(Some(0)), DEFAULT_MULTI_CLICK_INTERVAL);
        assert_eq!(sanitize(Some(60_000)), DEFAULT_MULTI_CLICK_INTERVAL);
    }

    #[test]
    fn sane_os_values_are_used() {
        assert_eq!(sanitize(Some(250)), Duration::from_millis(250));
        assert_eq!(sanitize(Some(900)), Duration::from_millis(900));
    }

    #[test]
    fn parses_the_gtk_settings_key() {
        let ini = "[Settings]\ngtk-theme-name=Adwaita\ngtk-double-click-time = 600\n";
        assert_eq!(parse_gtk_double_click_time(ini), Some(600));
        assert_eq!(parse_gtk_double_click_time("[Settings]\n"), None);
        assert_eq!(
            parse_gtk_double_click_time("gtk-double-click-time=abc"),
            None
        );
    }

    #[test]
    fn the_unit_test_interval_is_the_default() {
        assert_eq!(multi_click_interval(), DEFAULT_MULTI_CLICK_INTERVAL);
    }
}
