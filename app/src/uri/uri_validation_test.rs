use super::*;

// Ported from Warp's `app/src/uri/uri_tests.rs` at the pinned oracle
// (`02b53fcd8`, release `2026.07.29.09.05` stable — see `ORACLE.md`), which has
// 65 `#[test]`s. `app/src/uri/uri_test.rs` (this fork's `mod.rs`-adjacent test
// file) already carries most of them under matching names; this file adds the
// one remaining portable test that couldn't go there because `uri_test.rs` is
// under separate, active repair this round (do not add to it — see
// `ROUND4-BRIEF.md`).
//
// Classification of the tests absent from `uri_test.rs` by name:
//
//   - `test_action_auto_handoff_to_cloud_parse_{alias_path,default_trigger,sleep_trigger}`,
//     `test_action_cloud_agent_setup_parse`, `test_action_create_environment_parse{,_no_repos}`,
//     `test_action_focus_cloud_mode_parse`, `test_action_new_cloud_agent_conversation_parse`,
//     `test_app_web_link_rewrites_to_new_cloud_agent_conversation` (8): cloud.
//     `Action::CreateEnvironment`, `Action::FocusCloudMode`, `AutoHandoffToCloud`,
//     `CloudAgentSetup`, and `new_cloud_agent_conversation` all name Warp cloud
//     agent conversations / dev environments; this fork's `Action` enum in
//     `app/src/uri/mod.rs` has none of these variants. Skipped, no issue.
//   - `test_open_file_ipynb_opens_in_editor_when_disabled`,
//     `test_open_file_ipynb_routes_to_notebook_when_enabled` (2): feature gap,
//     already tracked. `classify_open_file_action` in `app/src/uri/mod.rs`
//     explicitly documents that Jupyter notebook routing
//     (`FeatureFlag::JupyterNotebookRendering`) is "deliberately NOT ported"
//     pending #240. Skipped here too — no new issue needed.
//   - `test_settings_section_for_simple_subpage` (1): feature gap, newly filed
//     as #414. `settings_section_for_simple_subpage` doesn't exist in this
//     fork's `app/src/uri/mod.rs` at all, so `warp://settings/appearance` /
//     `warp://settings/warp_agent` deep links resolve to nothing today. Not
//     ported verbatim: the pin's test also asserts
//     `SettingsSection::BillingAndUsage` and `SettingsSection::OzCloudAPIKeys`,
//     both removed from this fork's `SettingsSection` enum ("Zap Wave 3-1"),
//     so porting it as-is would mean weakening the pinned assertions to fit —
//     against policy. See #414 for the fork-scoped version to add.
//   - `validate_custom_uri_errors_do_not_leak_query_string` (1): portable, and
//     ported below verbatim. `validate_custom_uri` exists in this fork's
//     `app/src/uri/mod.rs` with the same `UriHost`/`Auth` shape as the pin.

#[test]
fn validate_custom_uri_errors_do_not_leak_query_string() {
    // Unexpected scheme.
    let url = Url::parse("https://auth/desktop_redirect?refresh_token=LEAKED").unwrap();
    let err = validate_custom_uri(&url).unwrap_err();
    let msg = format!("{err:?}");
    assert!(!msg.contains("refresh_token"), "{msg}");
    assert!(!msg.contains("LEAKED"), "{msg}");

    // Unexpected host.
    let url = Url::parse(&format!(
        "{}://unknown_host/desktop_redirect?refresh_token=LEAKED",
        ChannelState::url_scheme()
    ))
    .unwrap();
    let err = validate_custom_uri(&url).unwrap_err();
    let msg = format!("{err:?}");
    assert!(!msg.contains("refresh_token"), "{msg}");
    assert!(!msg.contains("LEAKED"), "{msg}");

    // Unexpected path for a host that doesn't allow arbitrary paths.
    let url = Url::parse(&format!(
        "{}://auth/not_the_redirect?refresh_token=LEAKED",
        ChannelState::url_scheme()
    ))
    .unwrap();
    let err = validate_custom_uri(&url).unwrap_err();
    let msg = format!("{err:?}");
    assert!(!msg.contains("refresh_token"), "{msg}");
    assert!(!msg.contains("LEAKED"), "{msg}");
}
