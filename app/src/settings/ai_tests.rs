use super::*;
use crate::{
    ai::request_usage_model::{RequestLimitInfo, RequestLimitRefreshDuration},
    server_time::ServerTimestamp,
    test_util::settings::initialize_settings_for_tests,
};
use chrono::Utc;
use warpui::{App, SingletonEntity};

fn create_test_request_limit_info(
    limit: usize,
    used: usize,
    next_refresh: DateTime<Utc>,
    is_unlimited: bool,
    refresh_duration: RequestLimitRefreshDuration,
) -> RequestLimitInfo {
    RequestLimitInfo {
        limit,
        num_requests_used_since_refresh: used,
        next_refresh_time: ServerTimestamp::new(next_refresh),
        is_unlimited,
        request_limit_refresh_duration: refresh_duration,
        is_unlimited_voice: false,
        voice_request_limit: 0,
        voice_requests_used_since_last_refresh: 0,
        max_files_per_repo: 5000,
        embedding_generation_batch_size: 100,
    }
}

#[test]
fn tui_statusline_default_matches_figma() {
    let config = TuiStatuslineConfig::default();
    assert_eq!(config.order, TuiStatuslineItem::ALL);
    assert_eq!(
        config.enabled,
        vec![
            TuiStatuslineItem::Model,
            TuiStatuslineItem::WorkingDirectory,
            TuiStatuslineItem::GitBranch,
            TuiStatuslineItem::GitDiffStatus,
        ]
    );
}

#[test]
fn tui_statusline_normalization_preserves_custom_order_and_appends_missing_items() {
    let config = TuiStatuslineConfig {
        order: vec![
            TuiStatuslineItem::GitBranch,
            TuiStatuslineItem::Model,
            TuiStatuslineItem::GitBranch,
        ],
        enabled: vec![
            TuiStatuslineItem::Model,
            TuiStatuslineItem::Model,
            TuiStatuslineItem::ContextWindowUsage,
        ],
    }
    .normalized();

    assert_eq!(
        config.order,
        vec![
            TuiStatuslineItem::GitBranch,
            TuiStatuslineItem::Model,
            TuiStatuslineItem::AutoApprove,
            TuiStatuslineItem::AutoQueue,
            TuiStatuslineItem::WorkingDirectory,
            TuiStatuslineItem::GitDiffStatus,
            TuiStatuslineItem::ContextWindowUsage,
        ]
    );
    assert_eq!(
        config.enabled,
        vec![
            TuiStatuslineItem::Model,
            TuiStatuslineItem::ContextWindowUsage,
        ]
    );
}

// FocusedTerminalInfo Tests

#[test]
fn test_update_both_values_changed() {
    App::test((), |mut app| async move {
        // Create FocusedTerminalInfo with default values (false, false)
        let model_handle = app.add_model(|_| FocusedTerminalInfo::default());

        // Setup event tracking
        let (sender, receiver) = async_channel::unbounded();
        let model_handle_clone = model_handle.clone();
        model_handle.update(&mut app, move |_, ctx| {
            let sender = sender.clone();
            ctx.subscribe_to_model(
                &model_handle_clone,
                move |_, event: &FocusedTerminalInfoEvent, _| match event {
                    FocusedTerminalInfoEvent::TerminalInfoUpdated => {
                        let _ = sender.try_send(());
                    }
                },
            );
        });

        // Update both values to (true, false)
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, false, ctx);
        });

        // Verify model state
        model_handle.read(&app, |model, _| {
            assert!(model.contains_any_remote_blocks());
            assert!(!model.contains_any_restored_remote_blocks());
        });

        // Verify event was emitted exactly once
        let mut count = 0;
        while receiver.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1);
    });
}

#[test]
fn test_update_additional_value_changed() {
    App::test((), |mut app| async move {
        // Create FocusedTerminalInfo with default values (false, false)
        let model_handle = app.add_model(|_| FocusedTerminalInfo::default());

        // Setup event tracking
        let (sender, receiver) = async_channel::unbounded();
        let model_handle_clone = model_handle.clone();
        model_handle.update(&mut app, move |_, ctx| {
            let sender = sender.clone();
            ctx.subscribe_to_model(
                &model_handle_clone,
                move |_, event: &FocusedTerminalInfoEvent, _| match event {
                    FocusedTerminalInfoEvent::TerminalInfoUpdated => {
                        let _ = sender.try_send(());
                    }
                },
            );
        });

        // First update to (true, false)
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, false, ctx);
        });

        // Clear events by draining the channel
        while receiver.try_recv().is_ok() {}

        // Now update to (true, true) - only changing restored blocks
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, true, ctx);
        });

        // Verify model state
        model_handle.read(&app, |model, _| {
            assert!(model.contains_any_remote_blocks());
            assert!(model.contains_any_restored_remote_blocks());
        });

        // Verify event was emitted exactly once
        let mut count = 0;
        while receiver.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1);
    });
}

#[test]
fn test_update_no_change() {
    App::test((), |mut app| async move {
        // Create FocusedTerminalInfo with default values (false, false)
        let model_handle = app.add_model(|_| FocusedTerminalInfo::default());

        // Setup event tracking
        let (sender, receiver) = async_channel::unbounded();
        let model_handle_clone = model_handle.clone();
        model_handle.update(&mut app, move |_, ctx| {
            let sender = sender.clone();
            ctx.subscribe_to_model(
                &model_handle_clone,
                move |_, event: &FocusedTerminalInfoEvent, _| match event {
                    FocusedTerminalInfoEvent::TerminalInfoUpdated => {
                        let _ = sender.try_send(());
                    }
                },
            );
        });

        // First update to (true, true)
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, true, ctx);
        });

        // Clear events by draining the channel
        while receiver.try_recv().is_ok() {}

        // Update with same values (true, true)
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, true, ctx);
        });

        // Verify model state remains the same
        model_handle.read(&app, |model, _| {
            assert!(model.contains_any_remote_blocks());
            assert!(model.contains_any_restored_remote_blocks());
        });

        // Verify no event was emitted
        let mut count = 0;
        while receiver.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 0);
    });
}

#[test]
fn test_update_only_remote_toggles() {
    App::test((), |mut app| async move {
        // Create FocusedTerminalInfo with default values (false, false)
        let model_handle = app.add_model(|_| FocusedTerminalInfo::default());

        // Setup event tracking
        let (sender, receiver) = async_channel::unbounded();
        let model_handle_clone = model_handle.clone();
        model_handle.update(&mut app, move |_, ctx| {
            let sender = sender.clone();
            ctx.subscribe_to_model(
                &model_handle_clone,
                move |_, event: &FocusedTerminalInfoEvent, _| match event {
                    FocusedTerminalInfoEvent::TerminalInfoUpdated => {
                        let _ = sender.try_send(());
                    }
                },
            );
        });

        // First update to (true, true)
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, true, ctx);
        });

        // Clear events by draining the channel
        while receiver.try_recv().is_ok() {}

        // Update with (false, true) - only remote blocks changes
        model_handle.update(&mut app, |model, ctx| {
            model.update(false, true, ctx);
        });

        // Verify model state
        model_handle.read(&app, |model, _| {
            assert!(!model.contains_any_remote_blocks());
            assert!(model.contains_any_restored_remote_blocks());
        });

        // Verify event was emitted exactly once
        let mut count = 0;
        while receiver.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1);
    });
}

#[test]
fn test_update_only_restored_toggles() {
    App::test((), |mut app| async move {
        // Create FocusedTerminalInfo with default values (false, false)
        let model_handle = app.add_model(|_| FocusedTerminalInfo::default());

        // Setup event tracking
        let (sender, receiver) = async_channel::unbounded();
        let model_handle_clone = model_handle.clone();
        model_handle.update(&mut app, move |_, ctx| {
            let sender = sender.clone();
            ctx.subscribe_to_model(
                &model_handle_clone,
                move |_, event: &FocusedTerminalInfoEvent, _| match event {
                    FocusedTerminalInfoEvent::TerminalInfoUpdated => {
                        let _ = sender.try_send(());
                    }
                },
            );
        });

        // First update to (true, true)
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, true, ctx);
        });

        // Clear events by draining the channel
        while receiver.try_recv().is_ok() {}

        // Update with (true, false) - only restored blocks changes
        model_handle.update(&mut app, |model, ctx| {
            model.update(true, false, ctx);
        });

        // Verify model state
        model_handle.read(&app, |model, _| {
            assert!(model.contains_any_remote_blocks());
            assert!(!model.contains_any_restored_remote_blocks());
        });

        // Verify event was emitted exactly once
        let mut count = 0;
        while receiver.try_recv().is_ok() {
            count += 1;
        }
        assert_eq!(count, 1);
    });
}

// ToolbarCommandMap Tests

#[test]
fn test_toolbar_command_map_deserialize_from_map() {
    let json = serde_json::json!({
        "^claude": "Claude",
        "^gemini": "Gemini",
        "^codex": ""
    });
    let map: ToolbarCommandMap = serde_json::from_value(json).unwrap();
    assert_eq!(map.0.len(), 3);
    assert_eq!(map.0["^claude"], "Claude");
    assert_eq!(map.0["^gemini"], "Gemini");
    assert_eq!(map.0["^codex"], "");
}

#[test]
fn test_toolbar_command_map_deserialize_from_legacy_vec() {
    let json = serde_json::json!(["^claude", "^gemini", "^custom"]);
    let map: ToolbarCommandMap = serde_json::from_value(json).unwrap();
    assert_eq!(map.0.len(), 3);
    // Legacy vec format should assign empty agent values.
    for (_, agent) in map.0.iter() {
        assert_eq!(agent, "");
    }
    let keys: Vec<_> = map.0.keys().collect();
    assert_eq!(keys, vec!["^claude", "^gemini", "^custom"]);
}

#[test]
fn test_toolbar_command_map_from_file_value_map_format() {
    use settings_value::SettingsValue;

    let value = serde_json::json!({
        "^claude": "Claude",
        "^amp": "Amp"
    });
    let map = ToolbarCommandMap::from_file_value(&value).unwrap();
    assert_eq!(map.0.len(), 2);
    assert_eq!(map.0["^claude"], "Claude");
    assert_eq!(map.0["^amp"], "Amp");
}

#[test]
fn test_toolbar_command_map_from_file_value_legacy_array() {
    use settings_value::SettingsValue;

    // Patterns are intentionally non-alphabetical to verify insertion order is preserved.
    let value = serde_json::json!(["^zebra", "^alpha", "^middle"]);
    let map = ToolbarCommandMap::from_file_value(&value).unwrap();
    assert_eq!(map.0.len(), 3);
    assert_eq!(map.0["^zebra"], "");
    assert_eq!(map.0["^alpha"], "");
    assert_eq!(map.0["^middle"], "");
    let keys: Vec<_> = map.0.keys().collect();
    assert_eq!(keys, vec!["^zebra", "^alpha", "^middle"]);
}

#[test]
fn test_toolbar_command_map_from_file_value_invalid() {
    use settings_value::SettingsValue;

    let value = serde_json::json!(42);
    assert!(ToolbarCommandMap::from_file_value(&value).is_none());
}

#[test]
fn test_toolbar_command_map_roundtrip() {
    use settings_value::SettingsValue;

    let mut inner = IndexMap::new();
    inner.insert("^claude".to_string(), "Claude".to_string());
    inner.insert("^custom".to_string(), String::new());
    let original = ToolbarCommandMap::new(inner);

    let file_value = original.to_file_value();
    let restored = ToolbarCommandMap::from_file_value(&file_value).unwrap();
    assert_eq!(original, restored);
}

#[test]
fn test_toolbar_command_map_matched_agent() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let mut map = IndexMap::new();
        map.insert("^claude".to_string(), "Claude".to_string());
        map.insert("^gemini".to_string(), "Gemini".to_string());
        map.insert("^custom-tool".to_string(), String::new());

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            report_if_error!(settings
                .cli_agent_footer_enabled_commands
                .set_value(ToolbarCommandMap::new(map), ctx));
        });

        app.read(|ctx| {
            let agent = CompiledCommandsForCodingAgentToolbar::matched_agent(ctx, "claude chat");
            assert_eq!(agent, Some(CLIAgent::Claude));

            let agent = CompiledCommandsForCodingAgentToolbar::matched_agent(ctx, "gemini ask");
            assert_eq!(agent, Some(CLIAgent::Gemini));

            let agent =
                CompiledCommandsForCodingAgentToolbar::matched_agent(ctx, "custom-tool --flag");
            assert_eq!(agent, Some(CLIAgent::Unknown));

            let agent =
                CompiledCommandsForCodingAgentToolbar::matched_agent(ctx, "unmatched-command");
            assert_eq!(agent, None);
        });
    });
}

#[test]
fn test_should_display_quota_reset_banner_with_empty_history() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // With empty history, banner should not be displayed
            assert!(!settings.should_display_quota_reset_banner());
        });
    });
}

#[test]
fn test_should_display_quota_reset_banner_with_quota_exceeded_not_dismissed() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Set up a history with a previous cycle that had quota exceeded and banner not dismissed
        let now = Utc::now();
        let previous_end_date = now - chrono::Duration::days(15);
        let current_end_date = now + chrono::Duration::days(15);

        let previous_cycle = CycleInfo {
            end_date: previous_end_date,
            was_quota_exceeded: true,
            banner_state: BannerState { dismissed: false },
        };

        let current_cycle = CycleInfo {
            end_date: current_end_date,
            was_quota_exceeded: false,
            banner_state: BannerState::default(),
        };

        let cycle_history = vec![previous_cycle, current_cycle];

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
                .unwrap();
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Banner should be displayed when the previous cycle had quota exceeded and banner not dismissed
            assert!(settings.should_display_quota_reset_banner());
        });
    });
}

#[test]
fn test_should_display_quota_reset_banner_with_quota_exceeded_dismissed() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Set up a history with a previous cycle that had quota exceeded but banner was dismissed
        let now = Utc::now();
        let previous_end_date = now - chrono::Duration::days(15);
        let current_end_date = now + chrono::Duration::days(15);

        let previous_cycle = CycleInfo {
            end_date: previous_end_date,
            was_quota_exceeded: true,
            banner_state: BannerState { dismissed: true },
        };

        let current_cycle = CycleInfo {
            end_date: current_end_date,
            was_quota_exceeded: false,
            banner_state: BannerState::default(),
        };

        let cycle_history = vec![previous_cycle, current_cycle];

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
                .unwrap();
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Banner should not be displayed when the previous cycle had quota exceeded but banner was dismissed
            assert!(!settings.should_display_quota_reset_banner());
        });
    });
}

#[test]
fn test_should_display_quota_reset_banner_with_quota_not_exceeded() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Set up a history with a previous cycle that did not have quota exceeded
        let now = Utc::now();
        let previous_end_date = now - chrono::Duration::days(15);
        let current_end_date = now + chrono::Duration::days(15);

        let previous_cycle = CycleInfo {
            end_date: previous_end_date,
            was_quota_exceeded: false,
            banner_state: BannerState::default(),
        };

        let current_cycle = CycleInfo {
            end_date: current_end_date,
            was_quota_exceeded: false,
            banner_state: BannerState::default(),
        };

        let cycle_history = vec![previous_cycle, current_cycle];

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
                .unwrap();
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Banner should not be displayed when the previous cycle did not have quota exceeded
            assert!(!settings.should_display_quota_reset_banner());
        });
    });
}

#[test]
fn test_should_display_quota_reset_banner_with_only_one_cycle() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Set up a history with only one cycle
        let now = Utc::now();
        let current_end_date = now + chrono::Duration::days(15);

        let current_cycle = CycleInfo {
            end_date: current_end_date,
            was_quota_exceeded: true, // Even if quota is exceeded
            banner_state: BannerState::default(),
        };

        let cycle_history = vec![current_cycle];

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
                .unwrap();
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Banner should not be displayed when there's only one cycle, even if quota is exceeded
            assert!(!settings.should_display_quota_reset_banner());
        });
    });
}

#[test]
fn test_update_quota_info_create_new_cycle_when_none_exists() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let now = Utc::now();
        let next_refresh = now + chrono::Duration::days(30);

        // Create a request limit info with quota not exceeded
        let request_limit_info = create_test_request_limit_info(
            100, // limit
            50,  // used
            next_refresh,
            false, // not unlimited
            RequestLimitRefreshDuration::Monthly,
        );

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            // Ensure we start with empty history
            settings
                .ai_request_quota_info
                .set_value(
                    AIRequestQuotaInfo {
                        cycle_history: vec![],
                    },
                    ctx,
                )
                .unwrap();

            // Update quota info
            settings.update_quota_info(&request_limit_info, ctx);
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Verify a new cycle was created
            let cycle_history = &settings.ai_request_quota_info.cycle_history;
            assert_eq!(cycle_history.len(), 1);

            let cycle = &cycle_history[0];
            assert_eq!(cycle.end_date, next_refresh);
            assert!(!cycle.was_quota_exceeded);
            assert!(!cycle.banner_state.dismissed);
        });
    });
}

#[test]
fn test_update_quota_info_update_existing_cycle() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let now = Utc::now();
        let cycle_end_date = now + chrono::Duration::days(30);

        // Set up an existing cycle
        let existing_cycle = CycleInfo {
            end_date: cycle_end_date,
            was_quota_exceeded: false,
            banner_state: BannerState::default(),
        };

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_request_quota_info
                .set_value(
                    AIRequestQuotaInfo {
                        cycle_history: vec![existing_cycle],
                    },
                    ctx,
                )
                .unwrap();
        });

        // Create a request limit info with updated usage
        let request_limit_info = create_test_request_limit_info(
            100, // limit
            75,  // used (increased)
            cycle_end_date,
            false, // not unlimited
            RequestLimitRefreshDuration::Monthly,
        );

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            // Update quota info
            settings.update_quota_info(&request_limit_info, ctx);
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Verify the cycle was updated
            let cycle_history = &settings.ai_request_quota_info.cycle_history;
            assert_eq!(cycle_history.len(), 1);

            let cycle = &cycle_history[0];
            assert_eq!(cycle.end_date, cycle_end_date);
            assert!(!cycle.was_quota_exceeded);
        });
    });
}

#[test]
fn test_update_quota_info_quota_exceeded() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let now = Utc::now();
        let next_refresh = now + chrono::Duration::days(30);

        // Create a request limit info with quota exceeded
        let request_limit_info = create_test_request_limit_info(
            100, // limit
            100, // used (equal to limit, should be marked as exceeded)
            next_refresh,
            false, // not unlimited
            RequestLimitRefreshDuration::Monthly,
        );

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            // Update quota info
            settings.update_quota_info(&request_limit_info, ctx);
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Verify quota exceeded is set correctly
            let cycle_history = &settings.ai_request_quota_info.cycle_history;
            let cycle = &cycle_history[0];
            assert!(cycle.was_quota_exceeded);
        });

        // Test with unlimited requests (should never be exceeded)
        let unlimited_request_limit_info = create_test_request_limit_info(
            100, // limit
            200, // used (exceeds limit)
            next_refresh,
            true, // unlimited
            RequestLimitRefreshDuration::Monthly,
        );

        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            // Update quota info
            settings.update_quota_info(&unlimited_request_limit_info, ctx);
        });

        AISettings::handle(&app).read(&app, |settings, _ctx| {
            // Verify quota exceeded is not set for unlimited plan
            let cycle_history = &settings.ai_request_quota_info.cycle_history;
            let cycle = &cycle_history[0];
            assert!(!cycle.was_quota_exceeded);
        });
    });
}

#[test]
fn test_mark_quota_banner_as_dismissed() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        let now = Utc::now();

        // Create test cycles: two expired cycles and one future cycle
        let expired_cycle_1 = CycleInfo {
            end_date: now - chrono::Duration::days(30), // 30 days ago
            was_quota_exceeded: true,
            banner_state: BannerState { dismissed: false },
        };

        let expired_cycle_2 = CycleInfo {
            end_date: now - chrono::Duration::days(15), // 15 days ago
            was_quota_exceeded: true,
            banner_state: BannerState { dismissed: false },
        };

        let future_cycle = CycleInfo {
            end_date: now + chrono::Duration::days(15), // 15 days in future
            was_quota_exceeded: false,
            banner_state: BannerState { dismissed: false },
        };

        let cycle_history = vec![expired_cycle_1, expired_cycle_2, future_cycle];

        // Set up initial state
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_request_quota_info
                .set_value(AIRequestQuotaInfo { cycle_history }, ctx)
                .unwrap();
        });

        // Mark expired cycles as dismissed
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings.mark_quota_banner_as_dismissed(ctx);
        });

        // Verify the results
        AISettings::handle(&app).read(&app, |settings, _ctx| {
            let cycle_history = &settings.ai_request_quota_info.cycle_history;
            assert_eq!(cycle_history.len(), 3);

            // First cycle (oldest expired) should be dismissed
            assert!(cycle_history[0].banner_state.dismissed);
            // Second cycle (more recent expired) should be dismissed
            assert!(cycle_history[1].banner_state.dismissed);
            // Future cycle should not be dismissed
            assert!(!cycle_history[2].banner_state.dismissed);
        });
    });
}

#[test]
fn extra_headers_backward_compat() {
    let toml_str = r#"
        id = "test-id"
        name = "Test Provider"
        base_url = "https://api.example.com/v1"
    "#;
    let provider: AgentProvider = toml::from_str(toml_str).expect("should deserialize");
    assert!(
        provider.extra_headers.is_empty(),
        "extra_headers should default to empty vec"
    );
}

#[test]
fn extra_headers_skip_when_empty() {
    let provider = AgentProvider {
        id: "test-id".to_string(),
        name: "Test".to_string(),
        kind: AgentProviderKind::default(),
        api_type: AgentProviderApiType::default(),
        base_url: "https://api.example.com/v1".to_string(),
        models: Vec::new(),
        extra_headers: Vec::new(),
        vertex_project: String::new(),
        vertex_location: String::new(),
        disabled: false,
        token_price: None,
    };
    let serialized = toml::to_string(&provider).expect("should serialize");
    assert!(
        !serialized.contains("extra_headers"),
        "empty extra_headers should not appear in TOML"
    );
}

#[test]
fn extra_headers_round_trip() {
    let mut provider = AgentProvider::new_empty();
    provider.extra_headers = vec![
        ("x-portkey-provider".to_string(), "openai".to_string()),
        ("x-custom".to_string(), "value".to_string()),
    ];
    let serialized = toml::to_string(&provider).expect("should serialize");
    let deserialized: AgentProvider = toml::from_str(&serialized).expect("should deserialize");
    assert_eq!(provider.extra_headers, deserialized.extra_headers);
}

#[test]
fn new_empty_provider_is_not_explicitly_disabled_but_reads_as_effectively_disabled() {
    // The raw flag starts false (the provider itself was never explicitly turned off) --
    // only `effectively_disabled()` (used for the Settings UI grouping) treats it as off,
    // because it has no models yet. This is what makes a freshly configured provider
    // graduate to "enabled" automatically the moment it gets a model, with no separate
    // "Enable" click needed.
    let provider = AgentProvider::new_empty();
    assert!(!provider.disabled);
    assert!(provider.effectively_disabled());
    assert!(!provider.is_usable());
}

#[test]
fn effectively_disabled_auto_graduates_once_models_are_added() {
    let mut provider = AgentProvider::new_empty();
    assert!(
        provider.effectively_disabled(),
        "no models yet -> effectively disabled"
    );

    provider.base_url = "https://api.example.com/v1".to_string();
    provider.models = vec![AgentProviderModel::from_id("some-model".to_string())];
    assert!(
        !provider.effectively_disabled(),
        "adding a model and an endpoint should graduate it back to enabled automatically"
    );

    // But an explicit disable always wins, even with models and an endpoint configured.
    provider.disabled = true;
    assert!(provider.effectively_disabled());
}

#[test]
fn effectively_disabled_when_endpoint_is_missing() {
    // Regression test: `effectively_disabled()` (the Settings UI's active/greyed-out
    // predicate) used to only check `disabled` and per-model state, not the endpoint --
    // letting a provider with a model but no base_url/vertex_project render as active in
    // Settings while `is_usable()` (the actual model-picker gate) silently excluded it.
    let mut provider = AgentProvider::new_empty();
    provider.models = vec![AgentProviderModel::from_id("some-model".to_string())];
    assert!(
        provider.effectively_disabled(),
        "a model with no endpoint must still read as effectively disabled"
    );
    assert_eq!(
        provider.effectively_disabled(),
        !provider.is_usable(),
        "effectively_disabled() and is_usable() must never disagree"
    );

    provider.base_url = "https://api.example.com/v1".to_string();
    assert!(
        !provider.effectively_disabled(),
        "filling in the endpoint should graduate it back to enabled"
    );
    assert_eq!(provider.effectively_disabled(), !provider.is_usable());
}

#[test]
fn effectively_disabled_when_every_individual_model_is_disabled() {
    // A non-empty models list where every entry is individually disabled (e.g. via the
    // "Disable shown" bulk action with no search filter) has nothing to serve, same as an
    // empty list -- it must not look "active" in Settings while contributing zero models
    // to the picker.
    let mut provider = AgentProvider::new_empty();
    provider.base_url = "https://api.example.com/v1".to_string();
    let mut model_a = AgentProviderModel::from_id("model-a".to_string());
    let mut model_b = AgentProviderModel::from_id("model-b".to_string());
    model_a.disabled = true;
    model_b.disabled = true;
    provider.models = vec![model_a, model_b];

    assert!(
        provider.effectively_disabled(),
        "every model disabled -> provider is effectively disabled too"
    );

    // Re-enabling just one model should be enough to bring the provider back.
    provider.models[0].disabled = false;
    assert!(
        !provider.effectively_disabled(),
        "at least one enabled model -> provider is usable again"
    );
}

#[test]
fn is_usable_requires_endpoint_and_model_and_not_disabled() {
    let mut provider = AgentProvider::new_empty();
    provider.disabled = false;
    assert!(!provider.is_usable(), "no endpoint, no models yet");

    provider.base_url = "https://api.example.com/v1".to_string();
    assert!(!provider.is_usable(), "still no models");

    provider.models = vec![AgentProviderModel::from_id("some-model".to_string())];
    assert!(
        provider.is_usable(),
        "endpoint + model + not disabled = usable"
    );

    provider.disabled = true;
    assert!(!provider.is_usable(), "explicitly disabled wins");
}

#[test]
fn is_usable_for_vertex_checks_project_not_base_url() {
    let mut provider = AgentProvider::new_empty();
    provider.disabled = false;
    provider.api_type = AgentProviderApiType::Vertex;
    provider.models = vec![AgentProviderModel::from_id("gemini-2.5-pro".to_string())];
    assert!(
        !provider.is_usable(),
        "Vertex has no base_url; needs vertex_project instead"
    );

    provider.vertex_project = "my-gcp-project".to_string();
    assert!(provider.is_usable());
}

#[test]
fn disabled_field_round_trip_and_omitted_when_false() {
    let mut provider = AgentProvider::new_empty();
    provider.disabled = false;
    let serialized = toml::to_string(&provider).expect("should serialize");
    assert!(
        !serialized.contains("disabled"),
        "disabled = false should not appear in TOML (matches is_false skip_serializing_if)"
    );

    provider.disabled = true;
    let serialized = toml::to_string(&provider).expect("should serialize");
    let deserialized: AgentProvider = toml::from_str(&serialized).expect("should deserialize");
    assert!(deserialized.disabled);
}

#[test]
fn model_from_id_starts_enabled() {
    // Unlike a freshly created provider, a freshly added/fetched model starts enabled --
    // most providers have a handful of models and the common case shouldn't require manual
    // enabling. Curating a huge catalog down is an opt-in bulk action, not the default.
    let model = AgentProviderModel::from_id("gpt-5".to_string());
    assert!(!model.disabled);
}

#[test]
fn model_disabled_field_round_trip_and_omitted_when_false() {
    let mut model = AgentProviderModel::from_id("gpt-5".to_string());
    let serialized = toml::to_string(&model).expect("should serialize");
    assert!(
        !serialized.contains("disabled"),
        "disabled = false should not appear in TOML"
    );

    model.disabled = true;
    let serialized = toml::to_string(&model).expect("should serialize");
    let deserialized: AgentProviderModel = toml::from_str(&serialized).expect("should deserialize");
    assert!(deserialized.disabled);
}

#[test]
fn model_legacy_plain_string_format_still_deserializes() {
    // Backward compat: `models = ["deepseek-chat"]` (pre-struct format) must still parse, now
    // that the struct's custom Deserialize impl has a new `disabled` field in Either::Full.
    let toml_str = r#"models = ["deepseek-chat"]"#;
    #[derive(serde::Deserialize)]
    struct Wrapper {
        models: Vec<AgentProviderModel>,
    }
    let parsed: Wrapper = toml::from_str(toml_str).expect("legacy plain-string format");
    assert_eq!(parsed.models.len(), 1);
    assert_eq!(parsed.models[0].id, "deepseek-chat");
    assert!(!parsed.models[0].disabled);
}

// --- `/cost` token prices (fork-authored: Warp has no client-side price table) ---

#[test]
fn model_token_price_round_trips_through_toml() {
    let mut model = AgentProviderModel::from_id("claude-sonnet-4-5".to_string());
    model.token_price = Some(TokenPrice {
        input_usd_per_million_tokens: 3.0,
        output_usd_per_million_tokens: 15.0,
        cache_read_usd_per_million_tokens: Some(0.3),
        cache_write_usd_per_million_tokens: Some(3.75),
    });
    let serialized = toml::to_string(&model).expect("should serialize");
    let deserialized: AgentProviderModel = toml::from_str(&serialized).expect("should deserialize");
    assert_eq!(deserialized.token_price, model.token_price);
}

#[test]
fn model_without_a_token_price_stays_unpriced_and_writes_nothing() {
    // Absence must survive the round trip: a `token_price` that materializes as zeros would
    // make `/cost` report `$0.00` for a model nobody has priced.
    let model = AgentProviderModel::from_id("gpt-5".to_string());
    let serialized = toml::to_string(&model).expect("should serialize");
    assert!(
        !serialized.contains("token_price"),
        "an unpriced model should not write a price block: {serialized}"
    );
    let deserialized: AgentProviderModel = toml::from_str(&serialized).expect("should deserialize");
    assert_eq!(deserialized.token_price, None);
}

#[test]
fn provider_token_price_round_trips_and_is_omitted_when_unset() {
    let mut provider = AgentProvider::new_empty();
    provider.base_url = "https://api.example.com/v1".to_string();
    let serialized = toml::to_string(&provider).expect("should serialize");
    assert!(!serialized.contains("token_price"), "{serialized}");

    provider.token_price = Some(TokenPrice {
        input_usd_per_million_tokens: 1.25,
        output_usd_per_million_tokens: 10.0,
        cache_read_usd_per_million_tokens: None,
        cache_write_usd_per_million_tokens: None,
    });
    let serialized = toml::to_string(&provider).expect("should serialize");
    let deserialized: AgentProvider = toml::from_str(&serialized).expect("should deserialize");
    assert_eq!(deserialized.token_price, provider.token_price);
}

#[test]
fn cache_rates_fall_back_to_the_input_rate_only_when_absent() {
    let with_cache_rate = TokenPrice {
        input_usd_per_million_tokens: 3.0,
        output_usd_per_million_tokens: 15.0,
        cache_read_usd_per_million_tokens: Some(0.3),
        cache_write_usd_per_million_tokens: None,
    };
    assert_eq!(with_cache_rate.cache_read_rate(), (0.3, true));
    assert_eq!(with_cache_rate.cache_write_rate(), (3.0, false));
}

#[test]
fn from_input_output_treats_two_empty_fields_as_no_price() {
    assert_eq!(TokenPrice::from_input_output(None, None), None);
    // One field entered is still a price: the other simply reads as zero, which the user can
    // see and correct, rather than the whole price silently disappearing.
    let only_input = TokenPrice::from_input_output(Some(3.0), None).expect("should be a price");
    assert_eq!(only_input.input_usd_per_million_tokens, 3.0);
    assert_eq!(only_input.output_usd_per_million_tokens, 0.0);
    // An explicit zero is a real answer, not an empty field.
    let free = TokenPrice::from_input_output(Some(0.0), Some(0.0)).expect("should be a price");
    assert_eq!(free.input_usd_per_million_tokens, 0.0);
}

#[test]
fn vertex_endpoint_url_uses_regional_host_for_a_location() {
    assert_eq!(
        vertex_endpoint_url("my-proj", "us-east5"),
        "https://us-east5-aiplatform.googleapis.com/v1/projects/my-proj/locations/us-east5/"
    );
}

#[test]
fn vertex_endpoint_url_falls_back_to_global() {
    let global = "https://aiplatform.googleapis.com/v1/projects/my-proj/locations/global/";
    assert_eq!(vertex_endpoint_url("my-proj", ""), global);
    assert_eq!(vertex_endpoint_url("my-proj", "global"), global);
}

#[test]
fn vertex_endpoint_url_normalizes_location_case() {
    // GCP consoles often display region ids capitalized; they must still route
    // to a valid lowercase host, not `US-EAST5-aiplatform...`.
    assert_eq!(
        vertex_endpoint_url("my-proj", "US-EAST5"),
        "https://us-east5-aiplatform.googleapis.com/v1/projects/my-proj/locations/us-east5/"
    );
    assert_eq!(
        vertex_endpoint_url("my-proj", "Global"),
        "https://aiplatform.googleapis.com/v1/projects/my-proj/locations/global/"
    );
}

#[test]
fn vertex_endpoint_url_with_empty_project_still_has_no_stray_double_slash_when_project_present() {
    // Regression guard for the malformed-URL bug: a non-empty project must never collapse
    // into an empty `projects//` segment. This asserts the well-formed shape stays well-formed
    // (the actual defense against an *empty* project reaching this function at all is
    // `AgentProvider::validation_error`, exercised at save time).
    let url = vertex_endpoint_url("my-proj", "global");
    assert!(
        !url.contains("projects//"),
        "a non-empty project must not produce an empty projects// segment: {url}"
    );
    assert!(url.contains("/projects/my-proj/"));
}

#[test]
fn vertex_endpoint_url_with_empty_project_produces_the_malformed_segment_the_bug_was_about() {
    // Documents the exact defect `AgentProvider::validation_error` exists to prevent from ever
    // reaching this function: `vertex_endpoint_url` itself has no project-emptiness guard, so an
    // empty project interpolates straight into the URL path as an empty segment.
    let url = vertex_endpoint_url("", "global");
    assert_eq!(
        url, "https://aiplatform.googleapis.com/v1/projects//locations/global/",
        "empty project produces the malformed projects// segment -- callers must validate \
         before reaching here"
    );
}

#[test]
fn validation_error_flags_vertex_provider_with_empty_project() {
    let mut provider = AgentProvider::new_empty();
    provider.api_type = AgentProviderApiType::Vertex;
    assert!(
        provider.validation_error().is_some(),
        "Vertex provider with an empty project must fail validation"
    );

    // Whitespace-only counts as empty too (matches the `.trim()` applied when saving).
    provider.vertex_project = "   ".to_string();
    assert!(provider.validation_error().is_some());

    provider.vertex_project = "my-gcp-project".to_string();
    assert!(
        provider.validation_error().is_none(),
        "a non-empty project should pass validation"
    );
}

#[test]
fn validation_error_ignores_empty_vertex_project_for_non_vertex_providers() {
    // vertex_project is meaningless (and always empty) for non-Vertex api types -- validation
    // must not flag it there, matching the existing "harmless when not Vertex" treatment
    // documented on `AgentProvider::vertex_project`.
    let provider = AgentProvider::new_empty();
    assert_eq!(provider.api_type, AgentProviderApiType::OpenAi);
    assert!(provider.validation_error().is_none());
}

#[test]
fn vertex_model_family_routes_claude_to_anthropic_else_gemini() {
    assert_eq!(vertex_model_family("claude-sonnet-4-6"), AgentProviderApiType::Anthropic);
    assert_eq!(vertex_model_family("Claude-Opus"), AgentProviderApiType::Anthropic);
    assert_eq!(vertex_model_family("gemini-2.5-flash"), AgentProviderApiType::Gemini);
    assert_eq!(vertex_model_family("llama-3"), AgentProviderApiType::Gemini);
}

#[test]
fn prompt_submission_mode_defaults_match_upstream() {
    // The queued-prompts feature relies on these defaults: a new prompt interrupts
    // the in-flight response, but a prompt submitted during a long-running command
    // queues until the command completes.
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.read(|ctx| {
            assert_eq!(
                AISettings::as_ref(ctx).default_prompt_submission_mode,
                PromptSubmissionMode::Interrupt,
            );
            assert_eq!(
                AISettings::as_ref(ctx).long_running_command_submission_mode,
                LongRunningCommandSubmissionMode::QueueUntilCommandCompletes,
            );
        });
    });
}

#[test]
fn prompt_submission_mode_set_value_round_trips() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            report_if_error!(
                settings
                    .default_prompt_submission_mode
                    .set_value(PromptSubmissionMode::Queue, ctx)
            );
            report_if_error!(
                settings
                    .long_running_command_submission_mode
                    .set_value(LongRunningCommandSubmissionMode::SendImmediately, ctx)
            );
        });
        app.read(|ctx| {
            assert_eq!(
                AISettings::as_ref(ctx).default_prompt_submission_mode,
                PromptSubmissionMode::Queue,
            );
            assert_eq!(
                AISettings::as_ref(ctx).long_running_command_submission_mode,
                LongRunningCommandSubmissionMode::SendImmediately,
            );
        });
    });
}

#[test]
fn submission_mode_file_value_uses_snake_case() {
    // The settings-file (toml/JSON) wire form is produced by the `SettingsValue`
    // derive, which converts enum variants to snake_case (it bypasses serde).
    // The model and settings page round-trip through these string keys, so lock
    // them down against the real persistence path.
    use settings_value::SettingsValue;

    assert_eq!(
        PromptSubmissionMode::Interrupt.to_file_value(),
        serde_json::json!("interrupt")
    );
    assert_eq!(
        PromptSubmissionMode::Queue.to_file_value(),
        serde_json::json!("queue")
    );
    assert_eq!(
        LongRunningCommandSubmissionMode::SendImmediately.to_file_value(),
        serde_json::json!("send_immediately")
    );
    assert_eq!(
        LongRunningCommandSubmissionMode::QueueUntilCommandCompletes.to_file_value(),
        serde_json::json!("queue_until_command_completes")
    );

    // And the file value parses back to the same variant.
    assert_eq!(
        PromptSubmissionMode::from_file_value(&serde_json::json!("queue")),
        Some(PromptSubmissionMode::Queue)
    );
    assert_eq!(
        LongRunningCommandSubmissionMode::from_file_value(&serde_json::json!(
            "send_immediately"
        )),
        Some(LongRunningCommandSubmissionMode::SendImmediately)
    );
}

#[test]
fn ai_autodetection_defaults_to_opt_in() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        AISettings::handle(&app).read(&app, |settings, ctx| {
            // NLD is opt-in: a fresh user who never touched the setting has it off.
            // This fails before the default flip (default was `true`) and passes after.
            assert!(!*settings.ai_autodetection_enabled_internal.value());
            // AI is enabled by default, so the getter reflects the opt-in setting
            // rather than a disabled-AI state.
            assert!(settings.is_any_ai_enabled(ctx));
            assert!(!settings.is_ai_autodetection_enabled(ctx));
        });
    });
}

#[test]
fn ai_autodetection_setting_can_be_toggled_on_and_off() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        // Mirrors what `/enable-natural-language-detection` does in the TUI.
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_autodetection_enabled_internal
                .set_value(true, ctx)
                .unwrap();
        });
        AISettings::handle(&app).read(&app, |settings, ctx| {
            assert!(*settings.ai_autodetection_enabled_internal.value());
            assert!(settings.is_ai_autodetection_enabled(ctx));
        });

        // Mirrors what `/disable-natural-language-detection` does in the TUI.
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .ai_autodetection_enabled_internal
                .set_value(false, ctx)
                .unwrap();
        });
        AISettings::handle(&app).read(&app, |settings, ctx| {
            assert!(!*settings.ai_autodetection_enabled_internal.value());
            assert!(!settings.is_ai_autodetection_enabled(ctx));
        });
    });
}

#[test]
fn orchestration_is_enabled_when_ai_is_enabled() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        AISettings::handle(&app).read(&app, |settings, ctx| {
            assert!(settings.is_orchestration_enabled(ctx));
        });
    });
}

// Ported from Warp's `app/src/ai/blocklist/block_tests.rs` at the pinned
// oracle (`02b53fcd8`, Warp `2026.07.29.09.05` stable — see `ORACLE.md`),
// which exercises the setting from outside `app/src/settings/`. Placed here
// instead since the field itself lives in this file's scope; the sibling
// speedbump settings this one is modeled on
// (`should_show_agent_mode_autoread_files_speedbump` et al.) have no
// coverage in this file either, but this one is otherwise untested since
// `app/src/ai/blocklist` is out of scope for this change.
#[test]
fn should_show_agent_mode_ask_user_question_speedbump_defaults_to_true() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        AISettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(*settings.should_show_agent_mode_ask_user_question_speedbump);
        });
    });
}

#[test]
fn should_show_agent_mode_ask_user_question_speedbump_round_trips_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        AISettings::handle(&app).update(&mut app, |settings, ctx| {
            settings
                .should_show_agent_mode_ask_user_question_speedbump
                .set_value(false, ctx)
                .unwrap();
        });
        AISettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.should_show_agent_mode_ask_user_question_speedbump);
        });
    });
}
