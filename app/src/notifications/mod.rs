//! Notification center (mailbox + toast).
//!
//! Rebuilt after 002ce467's cloud-removal mistakenly deleted it, keeping only
//! the cloud-unrelated local paths:
//! - The app's own BYOP agent (Oz) completion/error notifications
//! - Third-party CLI agent (Claude Code / Codex / DeepSeek, etc.) status notifications
//!
//! Module layout:
//! - `item`          the data model (`NotificationItem` / `NotificationItems`, etc.)
//! - `item_rendering` per-notification UI (shared by mailbox and toast)
//! - `model`         the singleton `NotificationsModel` (subscribes to the
//!                   history / cli session models, produces notifications)
//! - `view`          `NotificationMailboxView` (the mailbox's main panel)
//! - `toast_stack`   `AgentNotificationToastStack` (bottom-right toast)
//! - `telemetry`     notification-center-related telemetry events (`NotificationsTelemetryEvent`)

pub(crate) mod item;
pub(crate) mod item_rendering;
pub mod model;
pub(crate) mod telemetry;
pub mod toast_stack;
pub mod view;

pub(crate) use item::{
    NotificationCategory, NotificationFilter, NotificationId, NotificationItem, NotificationItems,
    NotificationSourceAgent,
};
pub use toast_stack::AgentNotificationToastStack;
pub use view::{NotificationMailboxView, NotificationMailboxViewEvent};

pub fn init(app: &mut warpui::AppContext) {
    NotificationMailboxView::init(app);
}
