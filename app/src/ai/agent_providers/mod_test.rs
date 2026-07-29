//! Smoke tests for BYOP provider configuration and lookup.

use ai::LLMId;
use settings::Setting;
use warpui::{App, SingletonEntity};

use crate::ai::agent_providers::{llm_id, lookup_byop, AgentProviderSecrets};
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
