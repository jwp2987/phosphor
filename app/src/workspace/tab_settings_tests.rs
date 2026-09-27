use super::*;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspace::header_toolbar_item::HeaderToolbarItemKind;
use settings::Setting;
use warpui::{App, SingletonEntity};

#[test]
fn use_latest_user_prompt_as_conversation_title_in_tab_names_defaults_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.use_latest_user_prompt_as_conversation_title_in_tab_names);
        });
    });
}

#[test]
fn use_latest_user_prompt_as_conversation_title_in_tab_names_uses_vertical_tabs_path() {
    assert_eq!(
        UseLatestUserPromptAsConversationTitleInTabNames::toml_path(),
        Some("appearance.vertical_tabs.use_latest_prompt_as_title")
    );
    assert_eq!(
        UseLatestUserPromptAsConversationTitleInTabNames::hierarchy(),
        Some("appearance.vertical_tabs")
    );
    assert_eq!(
        UseLatestUserPromptAsConversationTitleInTabNames::toml_key(),
        "use_latest_prompt_as_title"
    );
}

#[test]
fn enable_tab_groups_defaults_to_true() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(
                *settings.enable_tab_groups,
                "tab groups ship on; the setting only exists so a user can turn them off"
            );
        });
    });
}

#[test]
fn enable_tab_groups_uses_appearance_tabs_path() {
    assert_eq!(
        EnableTabGroups::toml_path(),
        Some("appearance.tabs.enable_tab_groups")
    );
    assert_eq!(EnableTabGroups::hierarchy(), Some("appearance.tabs"));
    assert_eq!(EnableTabGroups::toml_key(), "enable_tab_groups");
}

#[test]
fn show_vertical_tab_panel_in_restored_windows_defaults_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.show_vertical_tab_panel_in_restored_windows);
        });
    });
}

#[test]
fn show_vertical_tab_panel_in_restored_windows_uses_vertical_tabs_path() {
    assert_eq!(
        ShowVerticalTabPanelInRestoredWindows::toml_path(),
        Some("appearance.vertical_tabs.show_panel_in_restored_windows")
    );
    assert_eq!(
        ShowVerticalTabPanelInRestoredWindows::hierarchy(),
        Some("appearance.vertical_tabs")
    );
    assert_eq!(
        ShowVerticalTabPanelInRestoredWindows::toml_key(),
        "show_panel_in_restored_windows"
    );
}

#[test]
fn hide_title_bar_search_bar_in_vertical_tabs_defaults_to_false() {
    App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);

        TabSettings::handle(&app).read(&app, |settings, _ctx| {
            assert!(!*settings.hide_title_bar_search_bar_in_vertical_tabs);
        });
    });
}

#[test]
fn hide_title_bar_search_bar_in_vertical_tabs_uses_vertical_tabs_path() {
    assert_eq!(
        HideTitleBarSearchBarInVerticalTabs::toml_path(),
        Some("appearance.vertical_tabs.hide_title_bar_search_bar")
    );
    assert_eq!(
        HideTitleBarSearchBarInVerticalTabs::hierarchy(),
        Some("appearance.vertical_tabs")
    );
    assert_eq!(
        HideTitleBarSearchBarInVerticalTabs::toml_key(),
        "hide_title_bar_search_bar"
    );
}

#[test]
fn header_toolbar_chip_selection_default_contains_code_review() {
    let config = HeaderToolbarChipSelection::Default;
    assert!(config.contains_item(&HeaderToolbarItemKind::CodeReview));
}

#[test]
fn header_toolbar_chip_selection_custom_without_code_review_reports_absent() {
    let config = HeaderToolbarChipSelection::Custom {
        left: vec![
            HeaderToolbarItemKind::TabsPanel,
            HeaderToolbarItemKind::ToolsPanel,
        ],
        right: vec![HeaderToolbarItemKind::NotificationsMailbox],
    };
    assert!(!config.contains_item(&HeaderToolbarItemKind::CodeReview));
    assert!(config.contains_item(&HeaderToolbarItemKind::TabsPanel));
    assert!(config.contains_item(&HeaderToolbarItemKind::ToolsPanel));
    assert!(config.contains_item(&HeaderToolbarItemKind::NotificationsMailbox));
    assert!(!config.contains_item(&HeaderToolbarItemKind::AgentManagement));
}

#[test]
fn header_toolbar_chip_selection_custom_with_code_review_on_left_reports_present() {
    let config = HeaderToolbarChipSelection::Custom {
        left: vec![HeaderToolbarItemKind::CodeReview],
        right: vec![],
    };
    assert!(config.contains_item(&HeaderToolbarItemKind::CodeReview));
}

#[test]
fn host_footer_color_rule_eq_compares_pattern_only() {
    let production = HostFooterColorRule {
        pattern: Regex::new("^prod-").unwrap(),
        color: AnsiColorIdentifier::Red,
        name: Some("Production".to_string()),
    };
    let same_pattern_different_color_and_name = HostFooterColorRule {
        pattern: Regex::new("^prod-").unwrap(),
        color: AnsiColorIdentifier::Blue,
        name: None,
    };
    let different_pattern = HostFooterColorRule {
        pattern: Regex::new("^staging-").unwrap(),
        color: AnsiColorIdentifier::Red,
        name: Some("Production".to_string()),
    };

    // Two rules with the same pattern are the same rule for duplicate-detection purposes
    // (`settings_view::appearance_page::commit_host_footer_color_rule` rejects adding a second
    // one), regardless of color or display name: a duplicate pattern can never match, since
    // rules are tried first-to-last and the first match wins, so it would be silently dead
    // configuration if accepted.
    assert_eq!(production, same_pattern_different_color_and_name);
    assert_ne!(production, different_pattern);
}

#[test]
fn header_toolbar_chip_selection_custom_empty_reports_all_absent() {
    let config = HeaderToolbarChipSelection::Custom {
        left: vec![],
        right: vec![],
    };
    for item in HeaderToolbarItemKind::all_items() {
        assert!(!config.contains_item(&item));
    }
}
