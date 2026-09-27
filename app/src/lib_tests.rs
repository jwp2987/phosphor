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

/// `LaunchMode::CommandLine` carries its API key inside `GlobalOptions` rather than in a variant
/// field of its own, so it needs its own case: the CLI's `--api-key` must reach the same
/// key-extraction path the GUI and TUI use.
///
/// Adapted: upstream asserts on `LaunchMode::auth_initialization()` returning
/// `AuthInitialization::PendingApiKey`, which is part of the dropped cloud auth machinery. This
/// fork extracts the key with `api_key_from_launch_mode` instead, so the assertion is on that.
#[test]
fn command_line_api_key_requires_validation() {
    let command_line = LaunchMode::CommandLine {
        command: CliCommand::Whoami,
        global_options: GlobalOptions {
            api_key: Some("cli-api-key".to_owned()),
            ..Default::default()
        },
        debug: false,
        is_sandboxed: false,
        computer_use_override: None,
    };

    assert_eq!(
        api_key_from_launch_mode(&command_line).as_deref(),
        Some("cli-api-key")
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

// jwp2987/phosphor#680: `on_will_terminate` shuts language servers down through
// `begin_language_servers_shutdown_for_app_exit` (the pin calls
// `LspManagerModel::terminate` from the same hook). The hook's real steps reach for
// a dozen app singletons, so these drive the server steps directly and
// `run_will_terminate_steps` is tested for order with fake steps.

#[test]
fn will_terminate_lsp_step_terminates_every_language_server() {
    App::test((), |mut app| async move {
        app.update(lsp::init);

        let lsp_shutdown = app
            .update(begin_language_servers_shutdown_for_app_exit)
            .expect("a registered LSP manager is shut down on exit");
        assert_eq!(lsp_shutdown.wait(APP_EXIT_SHUTDOWN_GRACE), 0);

        app.read(|ctx| {
            assert!(
                lsp::LspManagerModel::as_ref(ctx).terminated_for_app_exit(),
                "quitting must run LspManagerModel's app-exit termination"
            );
        });
    });
}

#[test]
fn will_terminate_lsp_step_is_a_noop_without_an_lsp_manager() {
    // The remote-server daemon shares these callbacks but never calls
    // `lsp::init`; `LspManagerModel::handle` would panic there.
    App::test((), |mut app| async move {
        assert!(
            app.update(begin_language_servers_shutdown_for_app_exit)
                .is_none()
        );

        app.read(|ctx| assert!(!ctx.has_singleton_model::<lsp::LspManagerModel>()));
    });
}

/// Records the order `run_will_terminate_steps` calls the steps in.
#[derive(Default)]
struct RecordedSteps(Vec<&'static str>);

impl WillTerminateSteps for RecordedSteps {
    fn begin_server_shutdown(&mut self) {
        self.0.push("begin_server_shutdown");
    }
    fn flush_notebooks(&mut self) {
        self.0.push("flush_notebooks");
    }
    fn terminate_persistence_writer(&mut self) {
        self.0.push("terminate_persistence_writer");
    }
    fn tear_down_terminal_server(&mut self) {
        self.0.push("tear_down_terminal_server");
    }
    fn tear_down_app_services(&mut self) {
        self.0.push("tear_down_app_services");
    }
    fn finish_server_shutdown(&mut self) {
        self.0.push("finish_server_shutdown");
    }
    fn relaunch_for_autoupdate(&mut self) {
        self.0.push("relaunch_for_autoupdate");
    }
    fn tear_down_diagnostics(&mut self) {
        self.0.push("tear_down_diagnostics");
    }
}

impl RecordedSteps {
    fn position(&self, step: &str) -> usize {
        self.0
            .iter()
            .position(|recorded| *recorded == step)
            .unwrap_or_else(|| panic!("{step} never ran"))
    }
}

#[test]
fn will_terminate_runs_every_step_once_in_order() {
    let mut steps = RecordedSteps::default();

    run_will_terminate_steps(&mut steps);

    assert_eq!(
        steps.0,
        vec![
            "begin_server_shutdown",
            "flush_notebooks",
            "terminate_persistence_writer",
            "tear_down_terminal_server",
            "tear_down_app_services",
            "finish_server_shutdown",
            "relaunch_for_autoupdate",
            "tear_down_diagnostics",
        ]
    );
}

#[test]
fn will_terminate_starts_server_shutdown_before_the_writer_join() {
    // The writer join is unbounded; server shutdowns must progress during it rather
    // than queue behind it and eat the SIGTERM deadline.
    let mut steps = RecordedSteps::default();
    run_will_terminate_steps(&mut steps);
    assert!(
        steps.position("begin_server_shutdown") < steps.position("terminate_persistence_writer")
    );
}

#[test]
fn will_terminate_waits_for_servers_only_after_releasing_the_instance() {
    // A relaunch during the ~1s wait must not be routed to this hidden, exiting
    // instance: the single-instance name and terminal server go first.
    let mut steps = RecordedSteps::default();
    run_will_terminate_steps(&mut steps);
    let wait = steps.position("finish_server_shutdown");
    assert!(steps.position("tear_down_app_services") < wait);
    assert!(steps.position("tear_down_terminal_server") < wait);
}

#[test]
fn will_terminate_tears_down_the_terminal_server_after_the_writer() {
    // Otherwise the shells' exits are persisted as sessions that ended.
    let mut steps = RecordedSteps::default();
    run_will_terminate_steps(&mut steps);
    assert!(
        steps.position("terminate_persistence_writer")
            < steps.position("tear_down_terminal_server")
    );
    assert!(steps.position("tear_down_app_services") < steps.position("relaunch_for_autoupdate"));
}

// jwp2987/phosphor#687: the same hook stops MCP servers, against one deadline shared
// with the language servers. The manager's own tests (`templatable_manager::native`)
// drive real in-memory sessions; these cover the app-side wiring.

#[test]
fn will_terminate_server_step_is_a_noop_without_any_manager() {
    // The remote-server daemon registers neither manager; `handle` would panic.
    App::test((), |mut app| async move {
        let start = instant::Instant::now();

        app.update(shut_down_servers_for_app_exit);

        assert!(start.elapsed() < APP_EXIT_SHUTDOWN_GRACE);
        assert!(
            app.update(begin_mcp_servers_shutdown_for_app_exit)
                .is_none()
        );
        app.read(|ctx| {
            assert!(!ctx.has_singleton_model::<TemplatableMCPServerManager>());
            assert!(!ctx.has_singleton_model::<lsp::LspManagerModel>());
        });
    });
}

#[test]
fn will_terminate_server_step_stops_lsp_and_mcp_within_the_shared_grace() {
    App::test((), |mut app| async move {
        app.update(lsp::init);
        app.add_singleton_model(|_| TemplatableMCPServerManager::default());
        let start = instant::Instant::now();

        app.update(shut_down_servers_for_app_exit);

        assert!(
            start.elapsed() < APP_EXIT_SHUTDOWN_GRACE,
            "with nothing running, quitting must not wait out the grace"
        );
        app.read(|ctx| {
            assert!(lsp::LspManagerModel::as_ref(ctx).terminated_for_app_exit());
        });
        let mcp_shutdown = app
            .update(begin_mcp_servers_shutdown_for_app_exit)
            .expect("a registered MCP manager is shut down on exit");
        assert_eq!(mcp_shutdown.pending(), 0);
    });
}

/// `6696954c6`: stable-promotion of `CtrlCCancelsThirdPartyHarness`. The flag's
/// only enable path used to be `DOGFOOD_FLAGS`
/// (`crates/warp_features/src/lib.rs`), which reaches no binary this fork
/// ships -- see the doc comment on that list. Promoted here the way upstream's
/// own `RELEASE_FLAGS` doc comment prescribes: a `default` Cargo feature
/// bridged in `extra_flags`, not a `RELEASE_FLAGS` entry.
#[test]
fn ctrl_c_cancels_third_party_harness_has_a_default_enable_path() {
    assert!(
        enabled_features().contains(&FeatureFlag::CtrlCCancelsThirdPartyHarness),
        "the ctrl_c_cancels_third_party_harness Cargo feature (in app/Cargo.toml's \
         default) must bridge to the flag in extra_flags -- this test runs with \
         default features on, so an absent flag here means the bridge is missing \
         or the Cargo feature fell out of `default`"
    );
}
