//! Tests for `NotificationsModel`.
//!
//! Ported from the pin's `app/src/ai/agent_management/agent_management_model_tests.rs`,
//! trimmed to the singletons this fork's `NotificationsModel` actually needs
//! (the cloud-side `ActiveAgentViewsModel` / `QueuedQueryModel` wiring that the
//! pin's `setup_app` registers does not exist here).

use settings::Setting as _;
use warp_core::features::FeatureFlag;
use warpui::{App, EntityId, ModelHandle, SingletonEntity};

use super::*;
use crate::BlocklistAIHistoryModel;
use crate::ai::agent::conversation::AIConversationId;
use crate::notifications::item::NotificationFilter;
use crate::settings::AISettings;
use crate::terminal::cli_agent_sessions::CLIAgentSessionsModel;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspace::WorkspaceRegistry;

fn setup_app(app: &mut App) -> ModelHandle<NotificationsModel> {
    initialize_settings_for_tests(app);
    // `add_notification` resolves the notification's git branch through the
    // workspace registry, so the singleton has to exist even with no workspaces.
    app.add_singleton_model(|_| WorkspaceRegistry::new());
    // `NotificationsModel::new` subscribes to both of these while constructing,
    // so they must be registered first.
    app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
    app.add_singleton_model(|_| CLIAgentSessionsModel::new());
    app.add_singleton_model(NotificationsModel::new)
}

/// `show_agent_notifications` is a display setting, not a recording setting.
/// With it off the mailbox chip, the mailbox panel and the toast stack are all
/// suppressed, but the notification must still be recorded so unread state —
/// notably the vertical-tab unread dot, which reads
/// `has_unread_for_terminal_view` — keeps working. Regression test for the
/// `a530563eb` defect, where `add_notification` early-returned and dropped the
/// item outright. The pin has no such check in `add_notification`.
#[test]
fn add_notification_tracks_unread_activity_when_in_app_notifications_are_hidden() {
    App::test((), |mut app| async move {
        let _guard = FeatureFlag::HOANotifications.override_enabled(true);
        let notifications = setup_app(&mut app);

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .show_agent_notifications
                .set_value(false, ctx)
                .unwrap();
        });

        let conversation_id = AIConversationId::new();
        let terminal_view_id = EntityId::new();
        notifications.update(&mut app, |model, ctx| {
            model.add_notification(
                "Agent task".to_owned(),
                "Task completed.".to_owned(),
                NotificationCategory::Complete,
                NotificationSourceAgent::Oz,
                NotificationOrigin::Conversation(conversation_id),
                terminal_view_id,
                vec![],
                ctx,
            );
        });

        notifications.read(&app, |model, _| {
            assert_eq!(
                model
                    .notifications()
                    .filtered_count(NotificationFilter::All),
                1,
                "the notification must be recorded even when in-app display is off",
            );
            assert!(
                model
                    .notifications()
                    .has_unread_for_terminal_view(terminal_view_id),
                "unread state must survive so the vertical-tab dot still appears",
            );
        });
    });
}
