// Re-export everything from the `remote_server` crate so existing
// `crate::remote_server::*` imports in `app` continue to work.
pub use remote_server::*;

#[cfg(not(target_family = "wasm"))]
pub mod auth_context;
#[cfg(not(target_family = "wasm"))]
pub mod server_buffer_tracker;
#[cfg(not(target_family = "wasm"))]
pub mod get_branches;
#[cfg(not(target_family = "wasm"))]
pub mod get_committed_branch_files;
#[cfg(not(target_family = "wasm"))]
pub mod diff_state_proto;
#[cfg(not(target_family = "wasm"))]
pub mod ripgrep_search;
#[cfg(not(target_family = "wasm"))]
pub mod server_model;
#[cfg(not(target_family = "wasm"))]
pub mod ssh_transport;
#[cfg(unix)]
pub mod unix;

/// Run the `remote-server-proxy` subcommand.
#[cfg(unix)]
pub fn run_proxy(identity_key: String) -> anyhow::Result<()> {
    unix::proxy::run(&identity_key)
}

#[cfg(not(unix))]
pub fn run_proxy(_identity_key: String) -> anyhow::Result<()> {
    anyhow::bail!("remote-server-proxy is not supported on this platform")
}

/// Run the `remote-server-daemon` subcommand.
#[cfg(unix)]
pub fn run_daemon(identity_key: String) -> anyhow::Result<()> {
    unix::run_daemon(identity_key)
}

#[cfg(not(unix))]
pub fn run_daemon(_identity_key: String) -> anyhow::Result<()> {
    anyhow::bail!("remote-server-daemon is not supported on this platform")
}

/// Start the WarpUI headless app with all daemon singleton models.
///
/// This is the platform-agnostic core of every `run_daemon` implementation.
/// Platform-specific code (Unix sockets, Windows named pipes, …) binds a
/// listener and calls this function with the appropriate `ServerModel`
/// constructor — everything else (DirectoryWatcher, DetectedRepositories,
/// RepoMetadataModel, FileModel) is shared.
///
/// # Example
/// ```ignore
/// // In unix/mod.rs:
/// super::run_daemon_app(move |ctx| ServerModel::new(unix_listener, ctx))
/// ```
#[cfg(not(target_family = "wasm"))]
pub(super) fn run_daemon_app(
    server_model_init: impl FnOnce(&mut warpui::ModelContext<server_model::ServerModel>) -> server_model::ServerModel
        + 'static,
) -> anyhow::Result<()> {
    use warpui::platform::app::AppCallbacks;
    use warpui::platform::AppBuilder;
    use warpui::SingletonEntity;

    AppBuilder::new_headless(AppCallbacks::default(), Box::new(()), None).run(|ctx| {
        // Rotate log files from the previous daemon invocation in the background.
        ctx.background_executor()
            .spawn(warp_logging::rotate_log_files())
            .detach();
        use repo_metadata::repositories::DetectedRepositories;
        use repo_metadata::watcher::DirectoryWatcher;
        use repo_metadata::RepoMetadataModel;

        // Order matters: DetectedRepositories must be registered before
        // RepoMetadataModel because LocalRepoMetadataModel::new()
        // subscribes to DetectedRepositories::handle(ctx).
        ctx.add_singleton_model(DirectoryWatcher::new);
        // Register the skill-provider directories as force-included paths so
        // the gitignore-pruning watch descend filter still watches gitignored
        // skill directories (e.g. `.agents/skills`) for `Repository`
        // subscribers (LSP, MCP), mirroring the non-daemon registration in
        // `app/src/lib.rs`. Registered before any repository begins watching
        // so it gates descent on the very first registration. See #170.
        DirectoryWatcher::handle(ctx).update(ctx, |watcher, _| {
            watcher.register_force_included_paths(
                ::ai::skills::SKILL_PROVIDER_DEFINITIONS
                    .iter()
                    .map(|provider| provider.skills_path.clone()),
            );
        });
        ctx.add_singleton_model(|_ctx| DetectedRepositories::default());
        ctx.add_singleton_model(|ctx| {
            let model = RepoMetadataModel::new_with_incremental_updates(ctx);

            // Force-include project skill-provider directories even when
            // gitignored, and register them as standing-query targets so the
            // skill watcher's `StandingQueryResultsUpdated` subscription
            // sees skill files as soon as they're discovered, without
            // waiting on a full `RepositoryUpdated` tree rebuild. Mirrors
            // the registration `app/src/lib.rs` performs for the non-daemon
            // (app) path — the daemon previously skipped both calls, so
            // remote project-skill discovery silently had nothing to watch.
            // See #170.
            model.register_force_included_paths(
                ::ai::skills::SKILL_PROVIDER_DEFINITIONS
                    .iter()
                    .map(|provider| provider.skills_path.clone()),
                ctx,
            );
            model.set_project_skill_provider_paths(
                ::ai::skills::SKILL_PROVIDER_DEFINITIONS
                    .iter()
                    .map(|provider| provider.skills_path.clone()),
                ctx,
            );

            model
        });
        ctx.add_singleton_model(warp_files::FileModel::new);
        // GlobalBufferModel must be registered before ServerModel: the
        // server-side buffer-sync handling (server_model.rs /
        // server_buffer_tracker.rs) accesses it via
        // `GlobalBufferModel::handle(ctx)`, and failing to register it would
        // panic at daemon startup with "Cannot get singleton model ... never
        // registered". It subscribes to FileModel in its own `new()`, so it
        // must come after FileModel.
        ctx.add_singleton_model(crate::code::global_buffer_model::GlobalBufferModel::new);
        ctx.add_singleton_model(server_model_init);
    })?;
    Ok(())
}

// Zap Wave 6-1: the `wire_auth_token_rotation` function has been physically
// removed — it used to subscribe to the server API token rotation event and
// forward it to `RemoteServerManager::rotate_auth_token`. After Wave 3-1
// removed the auth subsystem, that event had zero emit points, so Wave 6-1
// removed the event, this subscription function, and the call site in
// `lib.rs` together. The `RemoteServerManager::rotate_auth_token` function
// body itself is kept for now.
