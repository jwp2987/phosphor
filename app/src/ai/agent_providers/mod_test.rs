//! Smoke tests for BYOP provider configuration and lookup.

use ai::LLMId;
use settings::Setting;
use warpui::{App, SingletonEntity};

use crate::ai::agent_providers::{llm_id, lookup_byop, AgentProviderSecrets};
// `tui_export` only exists under the `tui` feature; the tests that use it are
// likewise gated below, so a `--features gui` (no tui) check still compiles.
#[cfg(feature = "tui")]
use crate::tui_export::{
    tui_agent_provider_has_connected_key, tui_clear_agent_provider_api_key,
    tui_list_agent_provider_keys, tui_set_agent_provider_api_key,
};
use crate::ai::llms::{DisableReason, LLMPreferences};
use crate::auth::{AuthManager, AuthStateProvider};
use crate::network::NetworkStatus;
use crate::settings::{AISettings, AgentProvider, AgentProviderApiType, AgentProviderModel};
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspaces::user_workspaces::UserWorkspaces;

fn sample_provider(id: &str) -> AgentProvider {
    AgentProvider {
        id: id.to_owned(),
        name: "Test Ollama".to_owned(),
        kind: Default::default(),
        api_type: AgentProviderApiType::Ollama,
        base_url: "http://localhost:11434".to_owned(),
        models: vec![AgentProviderModel::from_id("llama3.2".to_owned())],
        extra_headers: Vec::new(),
        vertex_project: String::new(),
        vertex_location: String::new(),
        disabled: false,
        token_price: None,
    }
}

fn init_byop_test_app(app: &mut warpui::App) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(AgentProviderSecrets::new);
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.add_singleton_model(AuthManager::new_for_test);
    app.add_singleton_model(UserWorkspaces::default_mock);
    app.add_singleton_model(LLMPreferences::new);
    // AIExecutionProfilesModel::new reads ObjectStoreModel + the templatable MCP
    // manager, so register those first.
    app.add_singleton_model(crate::cloud_object::model::persistence::ObjectStoreModel::mock);
    app.add_singleton_model(|_| {
        crate::ai::mcp::templatable_manager::TemplatableMCPServerManager::default()
    });
    app.add_singleton_model(|ctx| {
        crate::ai::execution_profiles::profiles::AIExecutionProfilesModel::new(
            &crate::LaunchMode::new_for_unit_test(),
            ctx,
        )
    });
}

#[test]
fn smoke_build_byop_models_by_feature_exposes_configured_models() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-smoke-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .agent_providers
                    .set_value(vec![sample_provider(provider_id)], ctx);
            });
        });

        app.read(|ctx| {
            let choices: Vec<_> = LLMPreferences::as_ref(ctx)
                .get_base_llm_choices_for_agent_mode()
                .collect();
            assert_eq!(choices.len(), 1, "expected one BYOP model in picker");
            assert!(
                choices[0].disable_reason.is_none(),
                "valid provider should not be disabled"
            );
            assert_eq!(
                choices[0].id.as_str(),
                llm_id::encode(provider_id, "llama3.2").as_str()
            );
        });
    });
}

#[test]
fn smoke_build_byop_models_by_feature_uses_placeholder_when_misconfigured() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        app.read(|ctx| {
            let default = LLMPreferences::as_ref(ctx).get_default_base_model();
            assert_eq!(
                default.disable_reason,
                Some(DisableReason::Unavailable),
                "empty config should surface placeholder entry"
            );
        });
    });
}

#[test]
fn smoke_build_byop_models_by_feature_skips_empty_base_url() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut broken = sample_provider("broken");
                broken.base_url.clear();
                let _ = settings.agent_providers.set_value(vec![broken], ctx);
            });
        });

        app.read(|ctx| {
            let default = LLMPreferences::as_ref(ctx).get_default_base_model();
            assert_eq!(
                default.disable_reason,
                Some(DisableReason::Unavailable),
                "provider with empty base_url must not appear as selectable model"
            );
        });
    });
}

#[test]
fn smoke_lookup_byop_resolves_provider_and_model_without_api_key() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-lookup-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .agent_providers
                    .set_value(vec![sample_provider(provider_id)], ctx);
            });
        });

        let encoded = llm_id::encode(provider_id, "llama3.2");
        app.read(|ctx| {
            let (provider, api_key, model_id) =
                lookup_byop(ctx, &encoded).expect("lookup_byop should resolve configured model");
            assert_eq!(provider.id, provider_id);
            assert_eq!(model_id, "llama3.2");
            assert!(api_key.is_empty(), "Ollama path allows empty API key");
        });
    });
}

#[test]
fn smoke_lookup_byop_returns_none_when_endpoint_cleared() {
    // Regression test: lookup_byop used to only check effectively_disabled(), not the
    // endpoint half of is_usable() -- a provider whose base_url was cleared while it was
    // still the active selection (e.g. mid-edit) would drop out of the picker
    // (build_byop_llm_infos) but still resolve successfully through lookup_byop with an
    // empty base_url, instead of the clean None -> InvalidApiKey path callers expect.
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-cleared-endpoint";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut provider = sample_provider(provider_id);
                provider.base_url.clear();
                let _ = settings.agent_providers.set_value(vec![provider], ctx);
            });
        });

        let encoded = llm_id::encode(provider_id, "llama3.2");
        app.read(|ctx| {
            assert!(
                lookup_byop(ctx, &encoded).is_none(),
                "a provider with no endpoint must not resolve, matching build_byop_llm_infos"
            );
        });
    });
}

#[test]
fn smoke_lookup_byop_returns_none_for_unknown_id() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        app.read(|ctx| {
            assert!(lookup_byop(ctx, &LLMId::from("byop:missing:model")).is_none());
            assert!(lookup_byop(ctx, &LLMId::from("not-byop")).is_none());
        });
    });
}

#[test]
fn smoke_disabled_provider_is_excluded_from_picker_and_lookup() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-disabled-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut disabled = sample_provider(provider_id);
                disabled.disabled = true;
                let _ = settings.agent_providers.set_value(vec![disabled], ctx);
            });
        });

        let encoded = llm_id::encode(provider_id, "llama3.2");
        app.read(|ctx| {
            let default = LLMPreferences::as_ref(ctx).get_default_base_model();
            assert_eq!(
                default.disable_reason,
                Some(DisableReason::Unavailable),
                "a disabled provider's models must not appear in the picker, same as an \
                 unconfigured one"
            );
            assert!(
                lookup_byop(ctx, &encoded).is_none(),
                "lookup_byop must refuse to resolve a disabled provider even by exact id"
            );
        });
    });
}

#[test]
fn smoke_re_enabled_provider_is_usable_again() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-toggle-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut disabled = sample_provider(provider_id);
                disabled.disabled = true;
                let _ = settings.agent_providers.set_value(vec![disabled], ctx);
            });
        });
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut providers = settings.agent_providers.value().clone();
                providers[0].disabled = false;
                let _ = settings.agent_providers.set_value(providers, ctx);
            });
        });

        let encoded = llm_id::encode(provider_id, "llama3.2");
        app.read(|ctx| {
            let (provider, _api_key, model_id) = lookup_byop(ctx, &encoded)
                .expect("re-enabling a provider should make it resolvable again");
            assert_eq!(provider.id, provider_id);
            assert_eq!(model_id, "llama3.2");
        });
    });
}

#[test]
fn smoke_disabled_model_is_excluded_from_picker_but_sibling_model_still_works() {
    // Models a large-catalog provider curated down to a subset: one model explicitly
    // disabled, one left enabled, both within an otherwise-enabled provider.
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-model-disabled-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut provider = sample_provider(provider_id);
                provider.models = vec![
                    AgentProviderModel::from_id("llama3.2".to_owned()),
                    {
                        let mut m = AgentProviderModel::from_id("llama3.2-huge".to_owned());
                        m.disabled = true;
                        m
                    },
                ];
                let _ = settings.agent_providers.set_value(vec![provider], ctx);
            });
        });

        let enabled_id = llm_id::encode(provider_id, "llama3.2");
        let disabled_id = llm_id::encode(provider_id, "llama3.2-huge");
        app.read(|ctx| {
            let choices: Vec<_> = LLMPreferences::as_ref(ctx)
                .get_base_llm_choices_for_agent_mode()
                .collect();
            assert_eq!(
                choices.len(),
                1,
                "the disabled model must not appear in the picker"
            );
            assert_eq!(choices[0].id.as_str(), enabled_id.as_str());

            assert!(
                lookup_byop(ctx, &enabled_id).is_some(),
                "the sibling enabled model must still resolve"
            );
            assert!(
                lookup_byop(ctx, &disabled_id).is_none(),
                "lookup_byop must refuse a disabled model even by exact id"
            );
        });
    });
}

// ─────────────────────────────────────────────────────────────────────────────
// TUI `/api-keys` menu support (crate::tui_export::tui_*_agent_provider_*)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(feature = "tui")]
#[test]
fn tui_list_agent_provider_keys_reflects_no_key_then_key_set_then_cleared() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-tui-keys-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .agent_providers
                    .set_value(vec![sample_provider(provider_id)], ctx);
            });
        });

        // No key yet.
        app.read(|ctx| {
            let providers = tui_list_agent_provider_keys(ctx);
            assert_eq!(providers.len(), 1);
            assert_eq!(providers[0].provider_id, provider_id);
            assert_eq!(providers[0].display_name, "Test Ollama");
            assert!(!providers[0].has_key, "no key set yet");
        });

        // Setting a key persists it via AgentProviderSecrets and is immediately reflected.
        app.update(|ctx| {
            tui_set_agent_provider_api_key(ctx, provider_id, "sk-test-123".to_owned());
        });
        app.read(|ctx| {
            let providers = tui_list_agent_provider_keys(ctx);
            assert!(providers[0].has_key, "key should now be connected");
            assert_eq!(
                AgentProviderSecrets::as_ref(ctx).get(provider_id),
                Some("sk-test-123"),
                "key must be readable straight from AgentProviderSecrets, the same store the \
                 GUI settings AI page writes to"
            );
        });

        // Clearing removes it again.
        app.update(|ctx| {
            tui_clear_agent_provider_api_key(ctx, provider_id);
        });
        app.read(|ctx| {
            let providers = tui_list_agent_provider_keys(ctx);
            assert!(!providers[0].has_key, "key should be cleared");
            assert_eq!(AgentProviderSecrets::as_ref(ctx).get(provider_id), None);
        });
    });
}

#[cfg(feature = "tui")]
#[test]
fn tui_list_agent_provider_keys_falls_back_to_id_for_unnamed_provider() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-tui-keys-unnamed";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut provider = sample_provider(provider_id);
                provider.name.clear();
                let _ = settings.agent_providers.set_value(vec![provider], ctx);
            });
        });

        app.read(|ctx| {
            let providers = tui_list_agent_provider_keys(ctx);
            assert_eq!(providers[0].display_name, provider_id);
        });
    });
}

#[cfg(feature = "tui")]
#[test]
fn tui_agent_provider_has_connected_key_requires_both_usable_and_keyed() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);

        let provider_id = "provider-tui-indicator-1";
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let _ = settings
                    .agent_providers
                    .set_value(vec![sample_provider(provider_id)], ctx);
            });
        });
        let model_id = llm_id::encode(provider_id, "llama3.2");

        // Usable provider, no key yet -> not connected.
        app.read(|ctx| {
            assert!(!tui_agent_provider_has_connected_key(ctx, &model_id));
        });

        // Usable provider with a key -> connected. This is the model picker's
        // "(key connected)" indicator.
        app.update(|ctx| {
            tui_set_agent_provider_api_key(ctx, provider_id, "sk-test-456".to_owned());
        });
        app.read(|ctx| {
            assert!(tui_agent_provider_has_connected_key(ctx, &model_id));
        });

        // Disabling the provider (still keyed) must flip the indicator back off --
        // effectively_disabled() providers don't show up in the model picker at all, so a
        // stale "connected" reading here would be misleading busywork for a hidden model.
        app.update(|ctx| {
            AISettings::handle(ctx).update(ctx, |settings, ctx| {
                let mut providers = settings.agent_providers.value().clone();
                providers[0].disabled = true;
                let _ = settings.agent_providers.set_value(providers, ctx);
            });
        });
        app.read(|ctx| {
            assert!(!tui_agent_provider_has_connected_key(ctx, &model_id));
        });
    });
}

#[cfg(feature = "tui")]
#[test]
fn tui_agent_provider_has_connected_key_false_for_non_byop_id() {
    App::test((), |mut app| async move {
        init_byop_test_app(&mut app);
        app.read(|ctx| {
            assert!(!tui_agent_provider_has_connected_key(
                ctx,
                &LLMId::from("claude-4-sonnet")
            ));
        });
    });
}
