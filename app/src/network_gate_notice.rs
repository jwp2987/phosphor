//! The toast that says the network gate refused a request, with an "Allow this origin" link.
//!
//! It listens to [`NetworkGateLog`] (denials only) and shows an ephemeral toast in the active
//! window naming the purpose and the host. The link carries a
//! `WorkspaceAction::AllowNetworkOrigin` that appends the origin to Settings > Network >
//! Allowed origins. There is no Allow link when the denial has no host to allow (an
//! unparseable URL, or the redirect-hop cap).
//!
//! No clock: a repeat denial of the same purpose and host carries the same toast object id, so
//! the toast stack replaces the earlier toast rather than stacking another.

use warpui::{Entity, ModelContext, SingletonEntity, WindowId};

use crate::network_gate_log::{NetworkGateLog, NetworkGateLogEvent, allow_text};
use crate::view_components::{DismissibleToast, ToastLink};
use crate::workspace::{ToastStack, WorkspaceAction};
use http_client::gate::GateEvent;

/// The toast's replace-key: one toast per (purpose, host), however often it is denied.
pub(crate) fn toast_object_id(event: &GateEvent) -> String {
    format!(
        "network-gate-denied-{}-{:016x}",
        event.purpose, event.host_hash
    )
}

/// The toast text for a denial.
pub(crate) fn toast_text(event: &GateEvent) -> String {
    match allow_text(event) {
        Some(target) => crate::t!(
            "settings-network-gate-denied-toast",
            purpose = event.purpose,
            host = target
        ),
        None => crate::t!(
            "settings-network-gate-denied-toast-no-host",
            purpose = event.purpose
        ),
    }
}

pub struct NetworkGateNotice;

impl NetworkGateNotice {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        ctx.subscribe_to_model(
            &NetworkGateLog::handle(ctx),
            |_me: &mut Self, event, ctx| {
                if let NetworkGateLogEvent::Denied(denied) = event {
                    Self::show(denied, ctx);
                }
            },
        );
        Self
    }

    fn show(event: &GateEvent, ctx: &mut ModelContext<Self>) {
        let windows = ctx.windows();
        let Some(window_id) = windows
            .active_window()
            .or_else(|| windows.frontmost_window_id())
        else {
            // No window: the denial is still in the Settings > Network log.
            return;
        };
        Self::add_toast(event, window_id, ctx);
    }

    fn add_toast(event: &GateEvent, window_id: WindowId, ctx: &mut ModelContext<Self>) {
        let object_id = toast_object_id(event);
        let mut toast =
            DismissibleToast::error(toast_text(event)).with_object_id(object_id.clone());
        if let Some(origin) = allow_text(event) {
            toast = toast.with_link(
                ToastLink::new(crate::t!("settings-network-gate-allow-link")).with_onclick_action(
                    WorkspaceAction::AllowNetworkOrigin {
                        origin,
                        toast_object_id: object_id,
                    },
                ),
            );
        }
        ToastStack::handle(ctx).update(ctx, move |toast_stack, ctx| {
            toast_stack.add_ephemeral_toast(toast, window_id, ctx);
        });
    }
}

impl Entity for NetworkGateNotice {
    type Event = ();
}

impl SingletonEntity for NetworkGateNotice {}

#[cfg(test)]
#[path = "network_gate_notice_tests.rs"]
mod tests;
