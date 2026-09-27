//! Unit tests for the Linux `middle_click_paste_enabled` gating fixed for #708.
//!
//! These exercise `SelectionSettings::middle_click_paste_gate` directly rather than
//! `read_for_middle_click_paste`, because the latter needs a live `AppContext` (clipboard
//! access) to run at all, while the defect and its fix are entirely in the gating decision —
//! whether the setting's value is consulted before any clipboard is touched. See that
//! function's doc comment for the AppContext-dependent clipboard-selection half, which this
//! file does not re-test.

use super::*;
use settings::Setting;

/// A `SelectionSettings` with every field at its declared default except the two callers pass in.
fn settings_with(
    middle_click_paste_enabled: bool,
    linux_selection_clipboard: bool,
) -> SelectionSettings {
    SelectionSettings {
        copy_on_select: CopyOnSelect::new(None),
        linux_selection_clipboard: LinuxSelectionClipboard::new(Some(linux_selection_clipboard)),
        middle_click_paste_enabled: MiddleClickPasteEnabled::new(Some(middle_click_paste_enabled)),
        right_click_behavior: RightClickBehaviorSetting::new(None),
    }
}

/// The regression itself: before this fix, `middle_click_paste_enabled` declared
/// `SupportedPlatforms::OR(WINDOWS, MAC)`, so `is_supported_on_current_platform()` was
/// unconditionally `false` on Linux/FreeBSD and the setting's value never mattered there. This
/// crate's tests run on Linux, so if that regression came back this assertion is what would
/// catch it.
#[test]
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn middle_click_paste_enabled_is_supported_on_linux() {
    let settings = settings_with(true, true);
    assert!(
        settings
            .middle_click_paste_enabled
            .is_supported_on_current_platform(),
        "middle_click_paste_enabled must be SupportedPlatforms::DESKTOP (or wider), not an \
         OR that excludes Linux/FreeBSD -- see #708"
    );
}

/// Disabling the setting suppresses a middle-click paste outright -- the whole point of #708 --
/// independent of `linux_selection_clipboard`'s value. Before the fix, this had no effect on
/// Linux: the read path never consulted `middle_click_paste_enabled` at all.
#[test]
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn disabling_middle_click_paste_gates_it_off_on_linux() {
    assert!(!settings_with(false, true).middle_click_paste_gate());
    assert!(!settings_with(false, false).middle_click_paste_gate());
}

/// Enabling the setting does not, by itself, guarantee a paste -- `linux_selection_clipboard`
/// is a second, independent gate applied later by
/// `maybe_read_from_linux_selection_clipboard`. This test pins down only the first gate: it
/// must say "go ahead" regardless of the second setting's value, so the two remain
/// independently toggleable rather than one silently overriding the other.
#[test]
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
fn enabling_middle_click_paste_permits_it_regardless_of_primary_clipboard_setting() {
    assert!(settings_with(true, true).middle_click_paste_gate());
    assert!(settings_with(true, false).middle_click_paste_gate());
}
