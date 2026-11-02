use crate::settings::{CustomSecretRegex, PrivacySettings, PrivacySettingsChangedEvent};
use crate::terminal::model::set_user_and_enterprise_secret_regexes;
use crate::terminal::safe_mode_settings::{SafeModeSettings, get_secret_obfuscation_mode};
use crate::workspaces::user_workspaces::UserWorkspaces;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

/// Pushes the effective secret-redaction setting into the BYOP outbound-redaction switch
/// (#1469), which has no `AppContext` at the wire sites.
fn sync_outbound_redaction(app: &AppContext) {
    crate::ai::agent_providers::outbound_redaction::set_enabled(
        get_secret_obfuscation_mode(app).should_redact_secret(),
    );
}

/// Dummy singleton model that is used to update the current set of custom regexes within the
/// terminal model. We do this via a singleton model since we only want to do this once any time
/// the custom secret regex list changes, which must be done independent of any view.
pub struct CustomSecretRegexUpdater;

impl CustomSecretRegexUpdater {
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let updater = CustomSecretRegexUpdater;
        // Initialize with current custom regexes (will be empty until safe mode is enabled)
        updater.update_custom_secret_regex_list(ctx);
        sync_outbound_redaction(ctx);
        // Any change to the safe-mode setting, the privacy settings (enterprise force-enable) or
        // the workspace (enterprise redaction policy) re-evaluates the switch.
        let safe_mode_settings = SafeModeSettings::handle(ctx);
        ctx.subscribe_to_model(&safe_mode_settings, |_, _, ctx| {
            sync_outbound_redaction(ctx)
        });
        let user_workspaces = UserWorkspaces::handle(ctx);
        ctx.subscribe_to_model(&user_workspaces, |_, _, ctx| sync_outbound_redaction(ctx));

        let privacy_settings = PrivacySettings::handle(ctx);
        ctx.subscribe_to_model(&privacy_settings, |me, evt, ctx| {
            sync_outbound_redaction(ctx);
            if let PrivacySettingsChangedEvent::CustomSecretRegexList { .. } = evt {
                me.update_custom_secret_regex_list(ctx);
            }
        });
        updater
    }

    fn update_custom_secret_regex_list(&self, ctx: &mut ModelContext<Self>) {
        let privacy_settings = PrivacySettings::as_ref(ctx);

        // Get enterprise and user secrets separately
        let enterprise_secrets = privacy_settings
            .enterprise_secret_regex_list
            .iter()
            .map(CustomSecretRegex::pattern);

        let user_secrets = privacy_settings
            .user_secret_regex_list
            .iter()
            .map(CustomSecretRegex::pattern);

        set_user_and_enterprise_secret_regexes(user_secrets, enterprise_secrets);

        // Zap (Wave1-S4): the original telemetry-side `update_telemetry_secrets_regex`
        // call was removed entirely along with `server/telemetry/secret_redaction.rs`.
        // Safe mode's visual blurring is already fully covered by
        // `set_user_and_enterprise_secret_regexes`; the telemetry-side
        // defense-in-depth redact no longer serves a purpose since there's no
        // outbound path left for it to protect.
    }
}

impl Entity for CustomSecretRegexUpdater {
    type Event = ();
}

impl SingletonEntity for CustomSecretRegexUpdater {}
