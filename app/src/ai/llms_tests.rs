use warpui::{App, SingletonEntity};

use super::*;
use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::auth::AuthStateProvider;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::cloud_object::update_manager::UpdateManager;
use crate::network::NetworkStatus;
use crate::settings::{AISettings, PrivacySettings};
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspaces::user_workspaces::UserWorkspaces;
use crate::LaunchMode;

#[test]
fn llm_info_deserializes_without_base_model_name() {
    let raw = r#"{
            "display_name": "gpt-4o",
            "id": "gpt-4o",
            "usage_metadata": {
                "request_multiplier": 1,
                "credit_multiplier": null
            },
            "description": null,
            "disable_reason": null,
            "vision_supported": false,
            "spec": null,
            "provider": "Unknown"
        }"#;

    let info: LLMInfo = serde_json::from_str(raw).expect("should deserialize");
    assert_eq!(info.display_name, "gpt-4o");
    assert_eq!(info.base_model_name, "gpt-4o");
}

#[test]
fn llm_info_deserializes_host_configs_as_vec() {
    // Wire format from server: host_configs is a Vec
    let raw = r#"{
            "display_name": "gpt-4o",
            "id": "gpt-4o",
            "usage_metadata": { "request_multiplier": 1, "credit_multiplier": null },
            "provider": "OpenAI",
            "host_configs": [
                { "enabled": true, "model_routing_host": "DirectApi" },
                { "enabled": false, "model_routing_host": "AwsBedrock" }
            ]
        }"#;

    let info: LLMInfo = serde_json::from_str(raw).expect("should deserialize vec format");
    assert_eq!(info.display_name, "gpt-4o");
    assert_eq!(info.host_configs.len(), 2);
    assert!(
        info.host_configs
            .get(&LLMModelHost::DirectApi)
            .unwrap()
            .enabled
    );
    assert!(
        !info
            .host_configs
            .get(&LLMModelHost::AwsBedrock)
            .unwrap()
            .enabled
    );
}

#[test]
fn llm_info_round_trip_serializes_and_deserializes() {
    // Start with wire format (Vec)
    let wire_json = r#"{
            "display_name": "claude-3",
            "base_model_name": "claude-3",
            "id": "claude-3",
            "usage_metadata": { "request_multiplier": 2, "credit_multiplier": 1.5 },
            "description": "A powerful model",
            "vision_supported": true,
            "provider": "Anthropic",
            "host_configs": [
                { "enabled": true, "model_routing_host": "DirectApi" }
            ]
        }"#;

    // Deserialize from wire format
    let info: LLMInfo = serde_json::from_str(wire_json).expect("should deserialize");

    // Serialize (produces HashMap format)
    let serialized = serde_json::to_string(&info).expect("should serialize");

    // Deserialize again (from HashMap format)
    let round_tripped: LLMInfo =
        serde_json::from_str(&serialized).expect("should deserialize after round trip");

    assert_eq!(info, round_tripped);
}

fn server_llm(id: &str, disable_reason: Option<DisableReason>) -> LLMInfo {
    LLMInfo {
        display_name: id.to_string(),
        base_model_name: id.to_string(),
        id: id.into(),
        reasoning_level: None,
        usage_metadata: LLMUsageMetadata {
            request_multiplier: 1,
            credit_multiplier: None,
        },
        description: None,
        disable_reason,
        vision_supported: false,
        spec: None,
        provider: LLMProvider::Unknown,
        host_configs: HashMap::new(),
        discount_percentage: None,
        context_window: LLMContextWindow::default(),
    }
}

fn available(default_id: &str, choices: Vec<LLMInfo>) -> AvailableLLMs {
    AvailableLLMs {
        default_id: default_id.into(),
        choices,
        preferred_codex_model_id: None,
    }
}

#[test]
fn deserialized_available_llms_with_missing_default_does_not_panic() {
    // `AvailableLLMs::new()` guarantees `default_id` is one of `choices`, but
    // deserialization (e.g. a stale persisted cache or a server payload)
    // bypasses `new()`. Build such a struct, round-trip it through serde, and
    // confirm `default_llm_info()` falls back to the first choice instead of
    // panicking ("Default LLM ID must be present in choices").
    let original = available(
        "missing-default",
        vec![server_llm("gpt-x", None), server_llm("gpt-y", None)],
    );
    let json = serde_json::to_string(&original).expect("should serialize");
    let deserialized: AvailableLLMs = serde_json::from_str(&json).expect("should deserialize");

    assert_eq!(deserialized.default_id.as_str(), "missing-default");
    assert_eq!(deserialized.default_llm_info().id.as_str(), "gpt-x");
}

fn agent_llm(id: &str, display_name: &str) -> LLMInfo {
    LLMInfo {
        display_name: display_name.to_owned(),
        base_model_name: display_name.to_owned(),
        id: id.into(),
        reasoning_level: None,
        usage_metadata: LLMUsageMetadata {
            request_multiplier: 1,
            credit_multiplier: None,
        },
        description: None,
        disable_reason: None,
        vision_supported: false,
        spec: None,
        provider: LLMProvider::Unknown,
        host_configs: HashMap::new(),
        discount_percentage: None,
        context_window: LLMContextWindow::default(),
    }
}

/// Preferences whose agent-mode models are an `"auto"` default plus one concrete model and one
/// "custom" endpoint.
///
/// Adapted: upstream seeds the custom endpoint through `LLMPreferences::custom_llms`, a field this
/// BYOP-only fork does not have — here every pickable model, custom endpoints included, lives in
/// `models_by_feature.agent_mode` (see `agent_providers::build_byop_models_by_feature`). The
/// struct is built literally rather than via `LLMPreferences::new` so the test's model list is not
/// overwritten by a settings-driven rebuild.
fn preferences_for_profile_model_tests(custom_model_id: &LLMId) -> LLMPreferences {
    let agent_mode = AvailableLLMs::new(
        "auto".into(),
        vec![
            agent_llm("auto", "auto (cost-efficient)"),
            agent_llm("claude-opus", "Opus"),
            agent_llm(custom_model_id.as_str(), "Custom Endpoint"),
        ],
        None,
    )
    .expect("choices are non-empty");
    LLMPreferences {
        models_by_feature: ModelsByFeature {
            agent_mode,
            ..Default::default()
        },
        last_update: None,
        base_llm_for_terminal_view: HashMap::new(),
        reasoning_effort_per_terminal: HashMap::new(),
        last_used_reasoning: HashMap::new(),
    }
}

fn install_profile_model_singletons(app: &mut App) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(|_| AuthStateProvider::new_logged_out_for_test());
    app.add_singleton_model(|_| NetworkStatus::new());
    // `AIExecutionProfilesModel::new` reads the object store (and its update manager) while
    // resolving the default profile, so both have to be registered before it is constructed.
    app.add_singleton_model(UpdateManager::mock);
    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(|_| TemplatableMCPServerManager::default());
    app.add_singleton_model(PrivacySettings::mock);
    app.add_singleton_model(UserWorkspaces::default_mock);
}

/// Picking the model that the active profile already defaults to must drop the per-terminal
/// override rather than pinning the same id twice — otherwise a later profile switch would be
/// shadowed by a stale session override.
#[test]
fn selecting_a_custom_profile_default_clears_the_session_override() {
    App::test((), |mut app| async move {
        install_profile_model_singletons(&mut app);
        let profiles = app.add_singleton_model(|ctx| {
            AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
        });
        let custom_model_id = LLMId::from("custom-endpoint");
        let preferences =
            app.add_singleton_model(|_| preferences_for_profile_model_tests(&custom_model_id));
        let surface_id = EntityId::new();
        let profile_id =
            profiles.read(&app, |profiles, ctx| *profiles.active_profile(Some(surface_id), ctx).id());
        profiles.update(&mut app, |profiles, ctx| {
            profiles.set_base_model(profile_id, Some(custom_model_id.clone()), ctx);
        });
        preferences.update(&mut app, |preferences, ctx| {
            // Upstream seeds the session override via `set_agent_mode_llm_override`, which this
            // fork does not have; the field is seeded directly instead (the test module is a child
            // of `llms.rs`, so the private field is in scope).
            preferences
                .base_llm_for_terminal_view
                .insert(surface_id, LLMId::from("claude-opus"));
            preferences.update_preferred_agent_mode_llm(&custom_model_id, surface_id, ctx);
        });

        preferences.read(&app, |preferences, _| {
            assert_eq!(
                preferences.base_llm_for_terminal_view.get(&surface_id),
                None,
                "selecting the profile's own default must clear the session override"
            );
        });

        profiles.update(&mut app, |profiles, ctx| {
            profiles.set_base_model(profile_id, Some(LLMId::from("auto")), ctx);
        });
        // Fork behaviour, asserted deliberately: `get_preferred_base_model` consults the BYOP
        // picker's `byop_last_used_model_id` *before* the profile default ("the strongest signal
        // of user intent"), and `update_preferred_agent_mode_llm` above wrote the custom id there.
        // Upstream has no such setting, so its version of this test sees "auto" immediately.
        preferences.read(&app, |preferences, ctx| {
            assert_eq!(
                preferences
                    .get_active_base_model(ctx, Some(surface_id))
                    .id
                    .as_str(),
                custom_model_id.as_str()
            );
        });

        // With the last-used memory cleared, the profile default is what remains — the upstream
        // assertion, and the reason clearing the override above matters.
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                settings
                    .byop_last_used_model_id
                    .set_value(String::new(), ctx)
                    .expect("clearing the last-used model id should succeed");
            });
        });
        preferences.read(&app, |preferences, ctx| {
            assert_eq!(
                preferences
                    .get_active_base_model(ctx, Some(surface_id))
                    .id
                    .as_str(),
                "auto"
            );
        });
    });
}
