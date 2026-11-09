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
//! manager's own state events rather than a timer. It counts as shown only when a workspace's
//! toast stack confirms it put the toast on screen (`DismissibleToast::with_on_shown`). The toast
//! goes after 10 s, or at once when the keyring answers again. Clicking it is the explicit user
//! action that lets the keyring be tried again.

use std::time::Duration;

use warpui::windowing::WindowManager;
use warpui::{
    Entity, GetSingletonModelHandle, ModelContext, SingletonEntity, UpdateModel, WindowId,
};
use warpui_extras::secure_storage::availability::{self, Availability};

use crate::settings::network_secrets::ProxyCredentials;
use crate::view_components::DismissibleToast;
use crate::workspace::ToastStack;

/// Identifies the toast, so a later notice replaces an earlier one instead of stacking.
const TOAST_OBJECT_ID: &str = "keyring-unavailable";

/// How long the notice stays up. It is information, not a question; clicking it retries.
const TOAST_TIMEOUT: Duration = Duration::from_secs(10);

/// Pure "one notice per outage" bookkeeping, split out so it is testable without windows.
#[derive(Debug, Default)]
pub(crate) struct NoticeGate {
    /// An outage the user has not been shown yet.
    pending: bool,
    /// This outage has been shown (a workspace put the toast on screen); repeats are ignored.
    notified: bool,
}

impl NoticeGate {
    /// Feeds one availability state (the current one at startup, then each transition).
    /// Returns `true` when a notice already on screen has become stale and should go.
    pub(crate) fn observe(&mut self, availability: Availability) -> bool {
        match availability {
            Availability::Unavailable => {
                if !self.notified {
                    self.pending = true;
                }
                false
            }
            // The keyring answered, or the user asked to retry: a later outage is news again.
            Availability::Available | Availability::Unknown => {
                let shown = self.notified;
                self.pending = false;
                self.notified = false;
                shown
            }
        }
    }

    /// Whether to ask a window to show the notice. It stays due until a workspace confirms it
    /// displayed the toast ([`Self::confirm_shown`]), so a request to a window without one
    /// (or before it subscribed) is not counted as telling the user.
    pub(crate) fn due(&self) -> bool {
        self.pending && !self.notified
    }

    /// A workspace displayed the toast. Returns `false` for a stale confirmation (the outage
    /// ended meanwhile, or another window already showed it).
    pub(crate) fn confirm_shown(&mut self) -> bool {
        if !self.due() {
            return false;
        }
        self.pending = false;
        self.notified = true;
        true
    }
}

/// Singleton that shows the keyring notice. Register once, after `ToastStack`.
pub struct KeyringNotice {
    gate: NoticeGate,
    /// The window whose workspace displayed the current notice.
    shown_in: Option<WindowId>,
}

impl KeyringNotice {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        // Subscribe before reading the current state, so no transition falls between the two.
        // A transition already folded into the current state is ignored by the gate when its
        // event arrives.
        let changes = availability::subscribe();
        let _ = ctx.spawn_stream_local(
            changes,
            |me: &mut Self, change, ctx| me.on_change(change, ctx),
            |_, _| {},
        );
        ctx.subscribe_to_model(&WindowManager::handle(ctx), |me: &mut Self, _event, ctx| {
            me.show_if_due(ctx);
        });
        let mut gate = NoticeGate::default();
        gate.observe(availability::current());
        Self {
            gate,
            shown_in: None,
        }
    }

    fn on_change(&mut self, change: Availability, ctx: &mut ModelContext<Self>) {
        if self.gate.observe(change) {
            // The keyring is back (or being retried): the notice is no longer true.
            if let Some(window_id) = self.shown_in.take() {
                ToastStack::handle(ctx).update(ctx, move |toast_stack, ctx| {
                    toast_stack.remove_toast_by_identifier(
                        TOAST_OBJECT_ID.to_owned(),
                        window_id,
                        ctx,
                    );
                });
            }
        }
        self.show_if_due(ctx);
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
            let toast = DismissibleToast::default(crate::t!("common-keyring-unavailable"))
                .with_object_id(TOAST_OBJECT_ID.to_owned())
                .with_timeout(TOAST_TIMEOUT)
                .with_on_body_click(|ctx| {
                    retry_keyring(ctx);
                })
                .with_on_shown(move |ctx| {
                    KeyringNotice::handle(ctx).update(ctx, |notice, _| {
                        if notice.gate.confirm_shown() {
                            notice.shown_in = Some(window_id);
                            log::info!("Showing the keyring-unavailable notice");
                        }
                    });
                });
            toast_stack.add_ephemeral_toast(toast, window_id, ctx);
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

impl SingletonEntity for KeyringNotice {}

#[cfg(test)]
#[path = "keyring_notice_tests.rs"]
mod tests;
