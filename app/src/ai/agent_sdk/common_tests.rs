//! Tests for the agent-SDK model-id validator and the "model list is
//! unavailable" flag it reads.
//!
//! Adapted from upstream: the pin sets the flag from `refresh_authed_models`,
//! which fetches the account's model catalogue from Warp's servers. This fork is
//! BYOP and has no such fetch — `refresh_byop_models` rebuilds the list from local
//! settings and cannot fail — so the flag is driven through
//! `update_feature_model_choices`, which is the seam that takes a
//! `Result<ModelsByFeature, _>` from whoever produced the list. The lifecycle
//! being tested (set on failure, cleared by any successful update) is the same.

use std::collections::HashMap;

use warpui::App;

use super::{classify_agent_mode_base_model_id, validate_agent_mode_base_model_id};
use crate::LaunchMode;
use crate::ai::agent_providers::AgentProviderSecrets;
use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::ai::llms::{
    AvailableLLMs, LLMContextWindow, LLMId, LLMInfo, LLMPreferences, LLMProvider, LLMUsageMetadata,
    ModelsByFeature,
};
use crate::ai::mcp::TemplatableMCPServerManager;
use crate::auth::{AuthManager, AuthStateProvider};
use crate::network::NetworkStatus;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspaces::user_workspaces::UserWorkspaces;

#[test]
fn classify_returns_list_unavailable_error_when_list_unavailable() {
    // Simulates a failed model-list update: the list is stale, but not empty
    // (e.g. a leftover "auto"), which is exactly the case that previously
    // produced the misleading "Unknown model id" error for any id not in it.
    let valid_ids = vec![LLMId::from("auto")];
    let err = classify_agent_mode_base_model_id("claude-sonnet-4-5", &valid_ids, true)
        .expect_err("unavailable list should error");
    let msg = format!("{err:#}");
    assert!(
        !msg.contains("Unknown model id"),
        "should not blame the model id when the list is unavailable: {msg}"
    );
    assert!(
        msg.contains("Could not retrieve"),
        "should surface a model-list retrieval failure error: {msg}"
    );
}

#[test]
fn classify_returns_unknown_id_error_when_list_available_and_id_genuinely_invalid() {
    // A non-empty, available list that does not contain the id still produces
    // the existing "Unknown model id" error (with suggestions).
    let valid_ids = vec![LLMId::from("auto"), LLMId::from("gpt-x")];
    let err = classify_agent_mode_base_model_id("claude-sonnet-4-5", &valid_ids, false)
        .expect_err("genuinely invalid id should error");
    let msg = format!("{err:#}");
    assert!(
        msg.contains("Unknown model id"),
        "should preserve the existing 'Unknown model id' error: {msg}"
    );
    assert!(
        msg.contains("auto") && msg.contains("gpt-x"),
        "should list the available model suggestions: {msg}"
    );
}

#[test]
fn classify_accepts_id_in_choices_even_when_list_unavailable() {
    // A locally configured provider's model that is among the choices should
    // still validate even when the list is marked unavailable: a configured
    // endpoint does not stop existing because some other update failed.
    let valid_ids = vec![LLMId::from("custom-config-key")];
    let id = classify_agent_mode_base_model_id("custom-config-key", &valid_ids, true)
        .expect("an id present in the choices should validate");
    assert_eq!(id.as_str(), "custom-config-key");
}

// -- agent_mode_models_unavailable flag lifecycle (set on a failed model-list
// result, cleared via the shared on_server_update path) --

fn llm_info(id: &str) -> LLMInfo {
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
        disable_reason: None,
        vision_supported: false,
        spec: None,
        provider: LLMProvider::Unknown,
        host_configs: HashMap::new(),
        discount_percentage: None,
        context_window: LLMContextWindow::default(),
    }
}

fn available(default_id: &str, choices: Vec<LLMInfo>) -> AvailableLLMs {
    AvailableLLMs::new(default_id.into(), choices, None).expect("choices are non-empty")
}

/// Registers everything `LLMPreferences::new` and `on_server_update` read, in
/// dependency order, and returns the preferences handle.
fn setup_llm_preferences(app: &mut App) -> warpui::ModelHandle<LLMPreferences> {
    initialize_settings_for_tests(app);
    app.add_singleton_model(AgentProviderSecrets::new);
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.add_singleton_model(AuthManager::new_for_test);
    app.add_singleton_model(UserWorkspaces::default_mock);
    let llm_preferences = app.add_singleton_model(LLMPreferences::new);
    // `AIExecutionProfilesModel::new` reads the object store and the templatable
    // MCP manager, so register those first. The object store is named inline
    // rather than imported, matching `ai/agent_providers/mod_test.rs`: it lives
    // under `cloud_object`, and `script/check_cloud_boundary` tracks new `use`
    // lines into that tree. Nothing cloud is exercised — `mock` is a local stub
    // that only exists so the profiles model can be constructed.
    app.add_singleton_model(crate::cloud_object::model::persistence::ObjectStoreModel::mock);
    app.add_singleton_model(|_| TemplatableMCPServerManager::default());
    app.add_singleton_model(|ctx| {
        AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
    });
    llm_preferences
}

#[test]
fn update_feature_model_choices_clears_unavailable_flag_after_failed_fetch() {
    App::test((), |mut app| async move {
        let llm_preferences = setup_llm_preferences(&mut app);

        // Simulate a failed model-list update, which is what marks the
        // agent-mode list unavailable.
        llm_preferences.update(&mut app, |preferences, ctx| {
            preferences.update_feature_model_choices(
                Err(anyhow::anyhow!("model list update failed")),
                ctx,
            );
        });
        assert!(
            llm_preferences.read(&app, |preferences, _| {
                preferences.agent_mode_models_unavailable()
            }),
            "flag should be set after a failed model-list update"
        );

        // While the flag is set, a genuinely-invalid id is reported as a
        // model-list availability error rather than "Unknown model id".
        llm_preferences.read(&app, |_, app| {
            let err = validate_agent_mode_base_model_id("claude-sonnet-4-5", app)
                .expect_err("unavailable list should error");
            let msg = format!("{err:#}");
            assert!(
                !msg.contains("Unknown model id"),
                "should not blame the model id while the list is unavailable: {msg}"
            );
            assert!(
                msg.contains("Could not retrieve"),
                "should surface a model-list retrieval failure error: {msg}"
            );
        });

        // A later successful model-list update arrives through
        // `update_feature_model_choices(Ok(..))`, which goes straight to
        // `on_server_update` and previously bypassed the flag clear.
        let models = ModelsByFeature {
            agent_mode: available("auto", vec![llm_info("auto"), llm_info("gpt-x")]),
            coding: available("auto", vec![llm_info("auto")]),
            cli_agent: Some(available(
                "cli-agent-auto",
                vec![llm_info("cli-agent-auto")],
            )),
            computer_use: None,
        };
        llm_preferences.update(&mut app, |preferences, ctx| {
            preferences.update_feature_model_choices(Ok(models), ctx);
        });

        // The successful update must have cleared the unavailable flag ...
        assert!(
            !llm_preferences.read(&app, |preferences, _| {
                preferences.agent_mode_models_unavailable()
            }),
            "a successful model-list update through update_feature_model_choices must clear the unavailable flag"
        );

        // ... so a genuinely-invalid id is now reported as "Unknown model id"
        // rather than "model list unavailable".
        llm_preferences.read(&app, |_, app| {
            let err = validate_agent_mode_base_model_id("claude-sonnet-4-5", app)
                .expect_err("genuinely invalid id should still error");
            let msg = format!("{err:#}");
            assert!(
                msg.contains("Unknown model id"),
                "after the list is available again, a genuinely-invalid id should report 'Unknown model id': {msg}"
            );
        });
    });
}
