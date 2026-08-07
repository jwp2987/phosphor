use super::*;

// Ported from Warp's `app/src/lib_tests.rs` at the pinned oracle (`02b53fcd8`,
// release `2026.07.29.09.05` stable — see `ORACLE.md`), which has 4 `#[test]`s.
// 2 ported / 2 skipped (design divergence, no issue filed):
//
//   - `tui_uses_distinct_secure_storage_service_name` and
//     `app_keeps_default_secure_storage_service_name` test
//     `LaunchMode::secure_storage_service_name`, which namespaces the TUI's OS
//     keychain under a distinct `.tui` service suffix from the GUI's. This fork
//     does not have that method, and deliberately so: commit `fcf5aaf56`
//     ("fix(tui): share the GUI's app identity so BYOP models/config load")
//     found that a separate TUI identity pointed the TUI at an empty
//     config/secrets store, so `/model` couldn't see the GUI's BYOP providers
//     or their API keys. The fix was to give the TUI binary the *same*
//     `AppId` as the GUI (`crates/warp_tui/src/bin/oss.rs`), so both share one
//     keychain namespace. Porting these two tests would assert the exact
//     behavior that commit deliberately removed. Skipped, no issue.
//
// `app_and_tui_accept_api_keys` and `launch_modes_select_expected_logging_frontend`
// are ported below, adapted only in *shape*: this fork's `LaunchMode::Tui` carries
// `api_key` directly (no `TuiEntryPoint::Interactive` wrapper — the fork's TUI has
// no separate `CliCommand` entrypoint), and `LaunchMode::RemoteServerDaemon` is a
// unit variant (no `identity_key` field, since remote-daemon identity is derived
// elsewhere in this fork). `api_key_from_launch_mode` was extracted out of the
// inline match in `initialize_app` so it's unit-testable; the dogfood-channel gate
// that match applies on top of it is untouched.

#[test]
fn app_and_tui_accept_api_keys() {
    let app = LaunchMode::App {
        args: Default::default(),
        api_key: Some("app-api-key".to_owned()),
    };
    let tui = LaunchMode::Tui {
        mount: Box::new(|_| {}),
        api_key: Some("tui-api-key".to_owned()),
    };

    assert_eq!(
        api_key_from_launch_mode(&app).as_deref(),
        Some("app-api-key")
    );
    assert_eq!(
        api_key_from_launch_mode(&tui).as_deref(),
        Some("tui-api-key")
    );
}

#[test]
fn launch_modes_select_expected_logging_frontend() {
    let tui = LaunchMode::Tui {
        mount: Box::new(|_| {}),
        api_key: None,
    };
    let app = LaunchMode::App {
        args: Default::default(),
        api_key: None,
    };
    let test = LaunchMode::Test {
        driver: Box::new(None),
        is_integration_test: false,
    };

    assert_eq!(tui.log_frontend(), LogFrontend::Tui);
    assert_eq!(app.log_frontend(), LogFrontend::Gui);
    assert_eq!(test.log_frontend(), LogFrontend::Gui);
    assert_eq!(
        LaunchMode::RemoteServerProxy.log_frontend(),
        LogFrontend::Cli
    );
    assert_eq!(
        LaunchMode::RemoteServerDaemon.log_frontend(),
        LogFrontend::Cli
    );
}
