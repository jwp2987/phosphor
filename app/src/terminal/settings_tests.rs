use settings_value::SettingsValue;

use super::*;

#[test]
fn osc52_default_is_write_only_read_denied() {
    let default = Osc52ClipboardAccess::default();
    assert_eq!(default, Osc52ClipboardAccess::WriteOnly);
    assert!(default.allows_write());
    assert!(!default.allows_read());
}

#[test]
fn osc52_explicit_deny_is_kept() {
    // An explicit `deny` in settings.toml parses to Deny, not the new default.
    let deny = Osc52ClipboardAccess::from_file_value(&serde_json::json!("deny")).unwrap();
    assert_eq!(deny, Osc52ClipboardAccess::Deny);
    assert!(!deny.allows_write());
}

#[test]
fn osc52_deny_blocks_read_and_write() {
    let access = Osc52ClipboardAccess::Deny;
    assert!(!access.allows_read());
    assert!(!access.allows_write());
}

#[test]
fn osc52_write_only_allows_write_but_not_read() {
    let access = Osc52ClipboardAccess::WriteOnly;
    assert!(access.allows_write());
    assert!(!access.allows_read());
}

#[test]
fn osc52_read_write_allows_both() {
    let access = Osc52ClipboardAccess::ReadWrite;
    assert!(access.allows_read());
    assert!(access.allows_write());
}

#[test]
fn osc52_deserializes_all_variants_from_settings_value() {
    let deny = Osc52ClipboardAccess::from_file_value(&serde_json::json!("deny")).unwrap();
    assert_eq!(deny, Osc52ClipboardAccess::Deny);

    let write_only =
        Osc52ClipboardAccess::from_file_value(&serde_json::json!("write_only")).unwrap();
    assert_eq!(write_only, Osc52ClipboardAccess::WriteOnly);

    let read_write =
        Osc52ClipboardAccess::from_file_value(&serde_json::json!("read_write")).unwrap();
    assert_eq!(read_write, Osc52ClipboardAccess::ReadWrite);
}

#[test]
fn osc52_rejects_unknown_variant() {
    assert!(Osc52ClipboardAccess::from_file_value(&serde_json::json!("allow_all")).is_none());
}

#[test]
fn osc52_write_notice_fires_once() {
    use crate::test_util::settings::initialize_settings_for_tests;
    warpui::App::test((), |mut app| async move {
        initialize_settings_for_tests(&mut app);
        app.update(|ctx| {
            assert!(TerminalSettings::take_osc52_write_notice(ctx));
            assert!(!TerminalSettings::take_osc52_write_notice(ctx));
            assert!(!TerminalSettings::take_osc52_write_notice(ctx));
        });
    });
}

/// `hop=osc52_clipboard op=<write|read> access=<setting> allowed=<bool>`: the exact detail the
/// real-app case `osc52_write_read` parses (`cases_r14_state.OSC52_HOP_RE`), for every setting.
#[test]
fn osc52_hop_detail_names_the_request_the_setting_and_the_outcome() {
    let default = Osc52ClipboardAccess::default();
    assert_eq!(
        default.hop_detail(true),
        "op=write access=write_only allowed=true"
    );
    assert_eq!(
        default.hop_detail(false),
        "op=read access=write_only allowed=false"
    );
    assert_eq!(
        Osc52ClipboardAccess::Deny.hop_detail(true),
        "op=write access=deny allowed=false"
    );
    assert_eq!(
        Osc52ClipboardAccess::Deny.hop_detail(false),
        "op=read access=deny allowed=false"
    );
    assert_eq!(
        Osc52ClipboardAccess::ReadWrite.hop_detail(true),
        "op=write access=read_write allowed=true"
    );
    assert_eq!(
        Osc52ClipboardAccess::ReadWrite.hop_detail(false),
        "op=read access=read_write allowed=true"
    );
}
