//! One non-modal notice when a stored credential exists but could not be opened (#2095,
//! follow-up to #1932).
//!
//! The Linux secure-storage fallback keeps a value in an encrypted file. When that file is there
//! and cannot be opened (it was sealed with the install secret in the keyring and the keyring
//! does not answer, its key file is lost or was replaced, it is damaged, or a newer build wrote
//! it), the read still comes back as "not found" to its caller, and the user used to see empty
//! credentials with nothing to say why. Secure storage now records the event process-wide
//! (`warpui_extras::secure_storage::unreadable`), and this model turns the first one of a run
//! into one toast.
//!
//! Like the keyring notice (`keyring_notice.rs`) it waits for a window, driven by the window
//! manager's state events, not a timer, and counts as shown only when a workspace's toast stack
//! confirms it put the toast on screen. Once per launch: there is no retry to offer (the file
//! reads again by itself when its key is back), and nothing is deleted.
//!
//! The toast text is a fixed string. Nothing of a key name, a path or a value reaches it: the
//! record this model reads is a bare event.

use std::time::Duration;

use warpui::windowing::WindowManager;
use warpui::{Entity, ModelContext, SingletonEntity, WindowId};
use warpui_extras::secure_storage::unreadable;

use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// Identifies the toast, so a second request replaces the first instead of stacking.
const TOAST_OBJECT_ID: &str = "secure-storage-unreadable";

/// How long the notice stays up. It is information; the log has the details.
const TOAST_TIMEOUT: Duration = Duration::from_secs(15);

/// Pure "one notice per launch" bookkeeping, split out so it is testable without windows.
#[derive(Debug, Default)]
pub(crate) struct OncePerLaunch {
    /// An unreadable store was seen and the user has not been shown the notice yet.
    pending: bool,
    /// A workspace put the toast on screen. Final for the run.
    notified: bool,
}

impl OncePerLaunch {
    /// An unreadable store was seen (the state at startup, or the event).
    pub(crate) fn observe_unreadable(&mut self) {
        if !self.notified {
            self.pending = true;
        }
    }

    /// Whether to ask a window to show the notice. It stays due until a workspace confirms it
    /// displayed the toast, so a request to a window without one is not counted as telling the
    /// user.
    pub(crate) fn due(&self) -> bool {
        self.pending && !self.notified
    }

    /// A workspace displayed the toast. Returns `false` for a second confirmation (another
    /// window showed it already).
    pub(crate) fn confirm_shown(&mut self) -> bool {
        if !self.due() {
            return false;
        }
        self.pending = false;
        self.notified = true;
        true
    }
}

/// Singleton that shows the notice. Register once, after `ToastStack`.
pub struct UnreadableSecureStorageNotice {
    gate: OncePerLaunch,
}

impl UnreadableSecureStorageNotice {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Subscribe before reading the current state, so no event falls between the two. An
        // event already folded into the state is a repeat the gate ignores.
        let events = unreadable::subscribe();
        let _ = ctx.spawn_stream_local(
            events,
            |me: &mut Self, (), ctx| {
                me.gate.observe_unreadable();
                me.show_if_due(ctx);
            },
            |_, _| {},
        );
        ctx.subscribe_to_model(&WindowManager::handle(ctx), |me: &mut Self, _event, ctx| {
            me.show_if_due(ctx);
        });
        let mut gate = OncePerLaunch::default();
        if unreadable::seen() {
            gate.observe_unreadable();
        }
        Self { gate }
    }

    fn show_if_due(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.gate.due() {
            return;
        }
        let Some(window_id) = notice_window(ctx) else {
            // No window yet: the window manager's next state change brings us back here.
            return;
        };
        ToastStack::handle(ctx).update(ctx, move |toast_stack, ctx| {
            let toast = DismissibleToast::default(crate::t!("common-secure-storage-unreadable"))
                .with_object_id(TOAST_OBJECT_ID.to_owned())
                .with_timeout(TOAST_TIMEOUT)
                .with_on_shown(move |ctx| {
                    UnreadableSecureStorageNotice::handle(ctx).update(ctx, |notice, _| {
                        if notice.gate.confirm_shown() {
                            // The line the app tests can wait for. No key name, no path.
                            log::info!("Showing the secure-storage-unreadable notice");
                        }
                    });
                });
            toast_stack.add_ephemeral_toast(toast, window_id, ctx);
        });
    }
}

fn notice_window(ctx: &ModelContext<UnreadableSecureStorageNotice>) -> Option<WindowId> {
    let windows = ctx.windows();
    windows
        .active_window()
        .or_else(|| windows.frontmost_window_id())
}

impl Entity for UnreadableSecureStorageNotice {
    type Event = ();
}

impl SingletonEntity for UnreadableSecureStorageNotice {}

#[cfg(test)]
#[path = "secure_storage_notice_tests.rs"]
mod tests;
