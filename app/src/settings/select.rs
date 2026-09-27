use std::ops::Not;

use serde::{Deserialize, Serialize};
use warpui::{clipboard::ClipboardContent, AppContext};

use settings::{
    macros::define_settings_group, RespectUserSyncSetting, Setting, SupportedPlatforms, SyncToCloud,
};

/// What a bare (unmodified) right-click does in the terminal.
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Default,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "What a bare right-click does in the terminal.",
    rename_all = "snake_case"
)]
pub enum RightClickBehavior {
    #[default]
    /// Right-click opens the context menu.
    ContextMenu,
    /// Right-click pastes from the clipboard. Shift+right-click opens the context menu instead.
    Paste,
}

impl RightClickBehavior {
    pub fn as_dropdown_label(&self) -> &str {
        match self {
            Self::ContextMenu => "Open the context menu",
            Self::Paste => "Paste from the clipboard",
        }
    }
}

define_settings_group!(SelectionSettings, settings: [
    copy_on_select: CopyOnSelect {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "terminal.copy_on_select",
        description: "Whether text is automatically copied to the clipboard when selected.",
    },
    linux_selection_clipboard: LinuxSelectionClipboard {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::LINUX,
        sync_to_cloud: SyncToCloud::PerPlatform(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "system.linux_selection_clipboard",
        description: "Whether the Linux primary selection clipboard is used.",
    },
    // Widened from the pin's `OR(WINDOWS, MAC)` to `DESKTOP` (#708, sub-item of the closed #638):
    // on Linux/FreeBSD this now actually gates `read_for_middle_click_paste` instead of being
    // ignored outright.
    // See that function's doc comment and `DECLINED.md`'s `IMPROVED` section for why this is a
    // deliberate divergence from the pin rather than an oversight, and for what stays unchanged
    // (copy-to-primary-on-select is NOT gated on this setting, on Linux or anywhere else).
    middle_click_paste_enabled: MiddleClickPasteEnabled {
        type: bool,
        default: true,
        supported_platforms: SupportedPlatforms::DESKTOP,
        sync_to_cloud: SyncToCloud::PerPlatform(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "terminal.input.middle_click_paste_enabled",
        description: "Whether middle-click pastes from the clipboard.",
    },
    // Ported from the pin's `RightClickBehaviorSetting` (upstream 4111d08f9,
    // introduced by c25ac4070). The pin's version also carries
    // `surface: SettingSurfaces::GUI`; this fork dropped `SettingSurfaces`
    // (see `DECLINED.md`), so that key is omitted.
    right_click_behavior: RightClickBehaviorSetting {
        type: RightClickBehavior,
        default: RightClickBehavior::ContextMenu,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Globally(RespectUserSyncSetting::Yes),
        private: false,
        toml_path: "terminal.input.right_click_behavior",
        description: "What a bare right-click does in the terminal.",
    }
]);

impl SelectionSettings {
    pub fn copy_on_select_enabled(&self) -> bool {
        *self.copy_on_select.value()
    }

    pub fn right_click_pastes(&self) -> bool {
        *self.right_click_behavior.value() == RightClickBehavior::Paste
    }

    /// Returns whether honoring the Linux primary selection clipboard is enabled. On non-linux
    /// platforms this always returns false.
    pub fn linux_selection_clipboard_enabled(&self) -> bool {
        *self.linux_selection_clipboard.value()
            && self
                .linux_selection_clipboard
                .is_supported_on_current_platform()
    }

    /// Writes the selection to the system clipboard if `copy_on_select` is enabled, and to the
    /// Linux primary selection if `system.linux_selection_clipboard` is enabled.
    ///
    /// The primary-selection write is deliberately **not** gated on `copy_on_select`: the two
    /// settings govern two different clipboards. `copy_on_select` is the switch for CLIPBOARD,
    /// `linux_selection_clipboard` is the switch for PRIMARY, and populating PRIMARY on selection
    /// is the X11/Wayland convention. Turning `copy_on_select` off therefore stops the CLIPBOARD
    /// write only; stopping the PRIMARY write is `system.linux_selection_clipboard = false`. The
    /// asymmetry is documented for users in `docs/manual/02-terminal-basics.md`.
    ///
    /// This ordering is byte-identical to the pin (`4111d08f9:app/src/settings/select.rs:108-113`).
    /// #638 filed it as a defect ("disabling `copy_on_select` does not stop primary-selection
    /// writes"); reordering it would change observable behavior away from Warp, which AGENTS.md
    /// §5.10 makes a maintainer-sign-off decision rather than a silent fix. Do not reorder these
    /// two statements without that sign-off and a tracking issue.
    pub fn maybe_copy_on_select(&self, clipboard_content: ClipboardContent, ctx: &mut AppContext) {
        self.maybe_write_to_linux_selection_clipboard(|_| clipboard_content.clone(), ctx);
        if self.copy_on_select_enabled() && !clipboard_content.plain_text.is_empty() {
            ctx.clipboard().write(clipboard_content);
        }
    }

    /// Writes the selected content to the user's primary selection clipboard. On non-Linux
    /// platforms this is a noop.
    pub fn maybe_write_to_linux_selection_clipboard(
        &self,
        clipboard_contents_fn: impl FnOnce(&mut AppContext) -> ClipboardContent,
        ctx: &mut AppContext,
    ) {
        if self.linux_selection_clipboard_enabled() {
            let clipboard_content = clipboard_contents_fn(ctx);
            if !clipboard_content.plain_text.is_empty() {
                ctx.clipboard()
                    .write_to_primary_clipboard(clipboard_content);
            }
        }
    }

    fn maybe_read_from_linux_selection_clipboard(
        &self,
        ctx: &mut AppContext,
    ) -> Option<ClipboardContent> {
        self.linux_selection_clipboard_enabled()
            .then(|| ctx.clipboard().read_from_primary_clipboard())
    }

    /// Implements the correct middle-click paste behavior for the current platform.
    ///
    /// Linux has the "primary clipboard" to which it maps the middle mouse button. Other platforms
    /// lack this separate clipboard, and so we map middle-click to the normal clipboard on those
    /// platforms.
    ///
    /// **Deliberate divergence from the pin (#708, maintainer-approved, see `DECLINED.md`'s
    /// `IMPROVED` section).** The pin (`4111d08f9:app/src/settings/select.rs:144-154`) declares
    /// `middle_click_paste_enabled` as `SupportedPlatforms::OR(WINDOWS, MAC)` and never consults it
    /// on Linux/FreeBSD at all, so the only way to disable middle-click paste there was
    /// `system.linux_selection_clipboard = false` — which also disables copy-to-primary-on-select,
    /// since that is the same setting. `middle_click_paste_enabled` now applies on Linux/FreeBSD
    /// too (`supported_platforms: SupportedPlatforms::DESKTOP` above): a middle-click paste is
    /// gated on BOTH settings there (this one, and `linux_selection_clipboard` via
    /// `maybe_read_from_linux_selection_clipboard`), so either one alone can suppress it.
    ///
    /// **What this does NOT change:** copy-to-primary-on-select. `maybe_copy_on_select` and
    /// `maybe_write_to_linux_selection_clipboard` above call
    /// `linux_selection_clipboard_enabled()` directly and never read `middle_click_paste_enabled`
    /// — turning middle-click paste off does not stop text you select from being written to the
    /// primary selection, exactly as before this change.
    pub fn read_for_middle_click_paste(&self, ctx: &mut AppContext) -> Option<ClipboardContent> {
        if !self.middle_click_paste_gate() {
            return None;
        }
        if cfg!(any(target_os = "linux", target_os = "freebsd")) {
            self.maybe_read_from_linux_selection_clipboard(ctx)
        } else {
            Some(ctx.clipboard().read()).filter(|content| content.is_empty().not())
        }
    }

    /// Whether `middle_click_paste_enabled` currently permits a middle-click paste at all, on
    /// any platform — split out from `read_for_middle_click_paste` so the gating decision is
    /// testable without an `AppContext` (see `select_tests.rs`).
    fn middle_click_paste_gate(&self) -> bool {
        self.middle_click_paste_enabled
            .is_supported_on_current_platform()
            && *self.middle_click_paste_enabled.value()
    }
}

#[cfg(test)]
#[path = "select_tests.rs"]
mod tests;
