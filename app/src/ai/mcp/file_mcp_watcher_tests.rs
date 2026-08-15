use super::{
    FileMCPConfigDiagnosticKind, FileMCPConfigParseOutcome, FileMCPWatcher, FileMCPWatcherEvent,
    parse_mcp_config_file, substitute_env_vars,
};
use crate::ai::mcp::MCPProvider;
use crate::warp_managed_paths_watcher::WarpManagedPathsWatcher;
use futures::stream::AbortHandle;
use repo_metadata::{
    RepoMetadataModel, repositories::DetectedRepositories, watcher::DirectoryWatcher,
};
use std::collections::{HashMap, HashSet};
use std::env;
use std::path::PathBuf;
use warpui::{App, Entity};
use watcher::HomeDirectoryWatcher;

fn cleanup_env_vars(vars: &[&str]) {
    for var in vars {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { env::remove_var(var) };
    }
}

/// Registers the models `FileMCPWatcher::new` depends on, plus the watcher itself, mirroring
/// `file_based_manager_tests.rs::setup_app`.
fn setup_watcher(app: &mut App) -> warpui::ModelHandle<FileMCPWatcher> {
    app.add_singleton_model(DirectoryWatcher::new);
    app.add_singleton_model(|_| DetectedRepositories::default());
    app.add_singleton_model(RepoMetadataModel::new);
    app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
    app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
    app.add_singleton_model(FileMCPWatcher::new)
}

/// Minimal test entity used solely to hold a subscription: `ctx.subscribe_to_model` is a method
/// on `ModelContext<Self>`, so observing another model's events requires *some* entity to own the
/// subscription.
struct ConfigParsedProbe;

impl Entity for ConfigParsedProbe {
    type Event = ();
}

/// Directly exercises `abort_config_parse` in isolation: seeding a tracked handle and aborting
/// it must flip the handle to aborted and drop it from `parse_abort_handles`.
#[test]
fn abort_config_parse_cancels_and_removes_inflight_task() {
    let (file_mcp_tx, _file_mcp_rx) = async_channel::unbounded();
    let config_path = PathBuf::from("/tmp/.mcp.json");
    let key = (config_path.clone(), MCPProvider::Zap);
    let (abort_handle, _abort_registration) = AbortHandle::new_pair();
    let observed_handle = abort_handle.clone();
    let mut watcher = FileMCPWatcher {
        file_mcp_tx,
        parse_abort_handles: HashMap::from([(key.clone(), abort_handle)]),
        home_provider_watchers: HashMap::new(),
        project_repo_watchers: HashSet::new(),
    };

    watcher.abort_config_parse(&config_path, MCPProvider::Zap);

    assert!(observed_handle.is_aborted());
    assert!(!watcher.parse_abort_handles.contains_key(&key));
}

/// End-to-end: starting a new parse for a config that already has one in flight must cancel the
/// old one synchronously (before the replacement even resolves), and the event the watcher
/// actually emits must reflect the *new* parse, not a stale one that got superseded.
///
/// This is the regression this cluster exists to prevent: without cancel-on-supersede, a slow
/// parse of stale content can resolve after a fast parse of fresh content and clobber it, because
/// `FileBasedMCPManager::apply_parsed_servers` applies whatever `ConfigParsed` event arrives last
/// with no ordering/versioning of its own.
#[test]
fn update_servers_from_config_file_aborts_previous_inflight_parse() {
    App::test((), |mut app| async move {
        let directory = tempfile::tempdir().expect("temporary directory should be created");
        let config_path = directory.path().join(".mcp.json");
        std::fs::write(
            &config_path,
            r#"{"mcpServers":{"new-server":{"command":"new-command"}}}"#,
        )
        .expect("config file should be written");
        let root_path = directory.path().to_path_buf();

        let watcher_handle = setup_watcher(&mut app);

        // Observe `ConfigParsed` events the watcher emits, forwarding the server count over a
        // channel so the test can `.await` the real parse completing. `FileMCPWatcher::new` also
        // scans the real host's home directory for other providers' configs (e.g. `~/.claude.json`
        // if this happens to run on a machine that has one) and those are irrelevant background
        // noise here, so filter down to events for our own `config_path` specifically.
        let (tx, rx) = async_channel::unbounded::<usize>();
        let watcher_for_subscribe = watcher_handle.clone();
        let expected_config_path = config_path.clone();
        // Bind the handle: `ModelHandle` is refcounted, so discarding it drops the probe at the
        // next effect flush, taking the subscription and `tx` with it. `rx.recv()` then fails with
        // `RecvError` (every sender gone) rather than waiting for the parse.
        let _probe = app.add_model(|ctx| {
            ctx.subscribe_to_model(&watcher_for_subscribe, move |_, event, _| {
                if let FileMCPWatcherEvent::ConfigParsed {
                    config_path: parsed_path,
                    servers,
                    ..
                } = event
                {
                    if *parsed_path == expected_config_path {
                        let _ = tx.try_send(servers.len());
                    }
                }
            });
            ConfigParsedProbe
        });

        // Seed a fake in-flight parse for the same (config_path, provider) key, then start a
        // real parse for it. The real call must abort the fake one before returning.
        let previous_handle = watcher_handle.update(&mut app, |watcher, ctx| {
            let (abort_handle, _registration) = AbortHandle::new_pair();
            let observed = abort_handle.clone();
            watcher
                .parse_abort_handles
                .insert((config_path.clone(), MCPProvider::Zap), abort_handle);

            watcher.update_servers_from_config_file(
                &config_path,
                root_path.clone(),
                MCPProvider::Zap,
                ctx,
            );

            observed
        });

        assert!(
            previous_handle.is_aborted(),
            "starting a replacement parse must abort whatever was previously in flight \
             for the same config, not let it race to completion"
        );

        let server_count = rx
            .recv()
            .await
            .expect("watcher should emit ConfigParsed for the new parse");
        assert_eq!(
            server_count, 1,
            "the event actually applied must be the new parse's result"
        );
    });
}

#[test]
fn test_substitute_env_vars_success() {
    let test_vars = ["FOO", "BAZ", "REPEATED"];

    // Setup environment variables
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::set_var("FOO", "bar") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::set_var("BAZ", "qux") };
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::set_var("REPEATED", "value") };

    // Test 1: Single variable substitution
    let input = r#"{"key": "${FOO}"}"#;
    let result = substitute_env_vars(input).expect("Single variable substitution should succeed");
    assert_eq!(
        result, r#"{"key": "bar"}"#,
        "Single variable FOO should be replaced with 'bar'"
    );

    // Test 2: Multiple different variables
    let input = r#"{"key": "${FOO}", "other": "${BAZ}"}"#;
    let result = substitute_env_vars(input).expect("Multiple variable substitution should succeed");
    assert_eq!(
        result, r#"{"key": "bar", "other": "qux"}"#,
        "Multiple variables FOO and BAZ should be replaced"
    );

    // Test 3: Multiple occurrences of same variable
    let input = r#"{"a": "${REPEATED}", "b": "${REPEATED}", "c": "prefix_${REPEATED}_suffix"}"#;
    let result = substitute_env_vars(input).expect("Repeated variable substitution should succeed");
    assert_eq!(
        result, r#"{"a": "value", "b": "value", "c": "prefix_value_suffix"}"#,
        "All occurrences of REPEATED should be replaced with 'value', including within context"
    );

    // Cleanup
    cleanup_env_vars(&test_vars);
}

#[test]
fn test_substitute_env_vars_missing_or_empty() {
    // Test 1: Missing variable
    // Ensure MISSING_VAR is not set
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::remove_var("MISSING_VAR") };

    let input = r#"{"key": "${MISSING_VAR}"}"#;
    let result = substitute_env_vars(input);
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Missing or empty environment variable: MISSING_VAR"),
        "Error message should mention MISSING_VAR, got: {err_msg}"
    );

    // Test 2: Empty variable
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { env::set_var("EMPTY_VAR", "") };

    let input = r#"{"key": "${EMPTY_VAR}"}"#;
    let result = substitute_env_vars(input);
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Missing or empty environment variable: EMPTY_VAR"),
        "Error message should mention EMPTY_VAR, got: {err_msg}"
    );

    // Cleanup
    cleanup_env_vars(&["EMPTY_VAR"]);
}

#[tokio::test]
async fn parse_outcomes_distinguish_missing_invalid_and_valid_configs() {
    let directory = tempfile::tempdir().expect("temporary directory should be created");
    let path = directory.path().join(".mcp.json");

    assert!(matches!(
        parse_mcp_config_file(&path, MCPProvider::Zap).await,
        FileMCPConfigParseOutcome::Missing
    ));

    std::fs::write(&path, "{invalid").expect("invalid config should be written");
    match parse_mcp_config_file(&path, MCPProvider::Zap).await {
        FileMCPConfigParseOutcome::Error(diagnostic) => {
            assert_eq!(diagnostic.kind, FileMCPConfigDiagnosticKind::Parse);
        }
        _ => panic!("invalid JSON should produce a parse diagnostic"),
    }

    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("ZAP_MCP_TEST_MISSING") };
    std::fs::write(
        &path,
        r#"{"mcpServers":{"test":{"command":"${ZAP_MCP_TEST_MISSING}"}}}"#,
    )
    .expect("missing-env config should be written");
    match parse_mcp_config_file(&path, MCPProvider::Zap).await {
        FileMCPConfigParseOutcome::Error(diagnostic) => {
            assert_eq!(
                diagnostic.kind,
                FileMCPConfigDiagnosticKind::MissingEnvironmentVariable
            );
        }
        _ => panic!("missing env should produce a diagnostic"),
    }

    std::fs::write(
        &path,
        r#"{"mcpServers":{"test":{"command":"test-command"}}}"#,
    )
    .expect("valid config should be written");
    match parse_mcp_config_file(&path, MCPProvider::Zap).await {
        FileMCPConfigParseOutcome::Parsed(servers) => assert_eq!(servers.len(), 1),
        _ => panic!("valid config should produce one server"),
    }
}
