//! One non-modal notice when the OS keyring cannot be used (#1805).
//!
//! Secure storage records a keyring outage process-wide
//! (`warpui_extras::secure_storage::availability`): the first failed or unanswered keyring call
//! marks it unavailable, and no later lookup in the run contacts the keyring again. This model
//! turns that one transition into one toast. The app stays fully usable; only stored credentials
//! may be missing (on Linux they still come from the owner-only fallback file when they were
//! saved there).
//!
//! The outage is usually detected before any window exists (the secure-storage worker starts
//! reading during `initialize_app`), so the notice waits for a window, driven by the window
//! manager's own state events rather than a timer. Clicking the toast is the explicit user action
//! that lets the keyring be tried again.

use warpui::windowing::WindowManager;
use warpui::{
    Entity, GetSingletonModelHandle, ModelContext, SingletonEntity as _, UpdateModel, WindowId,
};
use warpui_extras::secure_storage::availability::{self, Availability};

use crate::settings::network_secrets::ProxyCredentials;
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// Identifies the toast, so a later notice replaces an earlier one instead of stacking.
const TOAST_OBJECT_ID: &str = "keyring-unavailable";

/// Pure "one notice per outage" bookkeeping, split out so it is testable without windows.
#[derive(Debug, Default)]
pub(crate) struct NoticeGate {
    /// An outage has not been shown yet (waiting for a window).
    pending: bool,
    /// This outage has been shown; repeats of it are ignored.
    notified: bool,
}

impl NoticeGate {
    /// Feeds one availability state (the current one at startup, then each transition).
    pub(crate) fn observe(&mut self, availability: Availability) {
        match availability {
            Availability::Unavailable => {
                if !self.notified {
                    self.pending = true;
                }
            }
            // The keyring answered, or the user asked to retry: a later outage is news again.
            Availability::Available | Availability::Unknown => {
                self.pending = false;
                self.notified = false;
            }
        }
    }

    /// Whether to show the notice now that a window is available; at most once per outage.
    pub(crate) fn take(&mut self) -> bool {
        if self.pending {
            self.pending = false;
            self.notified = true;
            true
        } else {
            false
        }
    }
}

/// Singleton that shows the keyring notice. Register once, after `ToastStack`.
pub struct KeyringNotice {
    gate: NoticeGate,
}

impl KeyringNotice {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Subscribe before reading the current state, so no transition falls between the two.
        // A transition already folded into the current state is ignored by the gate when its
        // event arrives.
        let changes = availability::subscribe();
        let _ = ctx.spawn_stream_local(
            changes,
            |me: &mut Self, change, ctx| {
                me.gate.observe(change);
                me.show_if_due(ctx);
            },
            |_, _| {},
        );
        ctx.subscribe_to_model(&WindowManager::handle(ctx), |me: &mut Self, _event, ctx| {
            me.show_if_due(ctx);
        });
        let mut gate = NoticeGate::default();
        gate.observe(availability::current());
        Self { gate }
    }

    fn show_if_due(&mut self, ctx: &mut ModelContext<Self>) {
        if !self.gate.pending {
            return;
        }
        let Some(window_id) = notice_window(ctx) else {
            // No window yet: the window manager's next state change brings us back here.
            return;
        };
        if !self.gate.take() {
            return;
        }
        log::info!("Showing the keyring-unavailable notice");
        ToastStack::handle(ctx).update(ctx, move |toast_stack, ctx| {
            let toast = DismissibleToast::default(crate::t!("common-keyring-unavailable"))
                .with_object_id(TOAST_OBJECT_ID.to_owned())
                .with_on_body_click(|ctx| {
                    retry_keyring(ctx);
                });
            toast_stack.add_persistent_toast(toast, window_id, ctx);
        });
    }
}

fn notice_window(ctx: &ModelContext<KeyringNotice>) -> Option<WindowId> {
    let windows = ctx.windows();
    windows
        .active_window()
        .or_else(|| windows.frontmost_window_id())
}

/// The explicit user retry: the next keyring lookup contacts the keyring again, and a proxy
/// password whose read failed is read again. Other stores re-read on their own next use (a
/// failed `AgentProviderSecrets` load is retried by its next waiter); stores that loaded from
/// the fallback are deliberately not re-read, so a value saved during the outage is not
/// replaced mid-session by an older keyring copy.
pub(crate) fn retry_keyring<C>(ctx: &mut C)
where
    C: GetSingletonModelHandle + UpdateModel,
{
    if !availability::retry() {
        return;
    }
    log::info!("Retrying the OS keyring at the user's request");
    ProxyCredentials::handle(ctx).update(ctx, |creds, ctx| creds.retry_if_unavailable(ctx));
}

impl Entity for KeyringNotice {
    type Event = ();
}

impl warpui::SingletonEntity for KeyringNotice {}

#[cfg(test)]
#[path = "keyring_notice_tests.rs"]
mod tests;
