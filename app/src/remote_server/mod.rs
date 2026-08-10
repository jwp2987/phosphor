// Re-export everything from the `remote_server` crate so existing
// `crate::remote_server::*` imports in `app` continue to work.
pub use remote_server::*;

#[cfg(not(target_family = "wasm"))]
pub mod auth_context;
/// Remote codebase indexing (Delta D2). `local_fs`-gated for the same reason
/// the daemon's index manager is: it walks and stores on the host filesystem.
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
pub mod codebase_index_status;
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
pub mod codebase_index_store;
/// Client-side model of the remote codebase index. Unlike the daemon-side
/// status/store modules above it never touches the host filesystem, so it is
/// not `local_fs`-gated; it is `wasm`-gated only because every
/// `RemoteServerManager` mutation it calls is.
#[cfg(not(target_family = "wasm"))]
pub mod codebase_index_model;
#[cfg(not(target_family = "wasm"))]
pub mod server_buffer_tracker;
#[cfg(not(target_family = "wasm"))]
pub mod get_branches;
#[cfg(not(target_family = "wasm"))]
pub mod get_committed_branch_files;
#[cfg(not(target_family = "wasm"))]
pub mod diff_state_proto;
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
pub mod diff_state_tracker;
#[cfg(not(target_family = "wasm"))]
pub mod ripgrep_search;
#[cfg(not(target_family = "wasm"))]
pub mod server_model;
#[cfg(not(target_family = "wasm"))]
pub mod ssh_transport;
#[cfg(unix)]
pub mod unix;

/// Pre-handshake index limits for the daemon.
///
/// These are placeholders, not policy: the client sends the real limits on
/// `Initialize`, resolved from the same `AIRequestUsageModel` the local index
/// uses, and `ServerModel::apply_codebase_index_limits` replaces these. They
/// exist only so a daemon that is asked to index before any client has
/// completed the handshake behaves sanely instead of using `usize::MAX`.
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
const DAEMON_DEFAULT_MAX_FILES_PER_REPO: usize = 10_000;
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
const DAEMON_DEFAULT_EMBEDDING_BATCH_SIZE: usize = 32;

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

/// The directory this daemon keeps its codebase-index snapshots and vector
/// database in.
///
/// The pin's equivalent is `daemon_codebase_index_snapshot_storage`
/// (`02b53fcd8:app/src/lib.rs:364`), which read the identity key out of
/// `LaunchMode::RemoteServerDaemon { identity_key }`. This fork's
/// `LaunchMode::RemoteServerDaemon` is a unit variant and never reaches
/// `initialize_app` at all — the daemon runs its own bootstrap in
/// [`run_daemon_app`] — so the identity key is taken from the argument that
/// already carries it, and this lives next to the daemon rather than in
/// `lib.rs`.
#[cfg(all(not(target_family = "wasm"), feature = "local_fs"))]
pub fn daemon_codebase_index_data_dir(identity_key: &str) -> std::path::PathBuf {
    let data_dir = ::remote_server::setup::remote_server_daemon_data_dir(identity_key);
    std::path::PathBuf::from(shellexpand::tilde(&data_dir).into_owned())
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
    #[cfg_attr(not(feature = "local_fs"), allow(unused_variables))] identity_key: String,
    server_model_init: impl FnOnce(&mut warpui::ModelContext<server_model::ServerModel>) -> server_model::ServerModel
        + 'static,
) -> anyhow::Result<()> {
    use warpui::platform::app::AppCallbacks;
    use warpui::platform::AppBuilder;
    use warpui::SingletonEntity;

    AppBuilder::new_headless(AppCallbacks::default(), Box::new(()), None).run(move |ctx| {
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
        // SkillManager backs the daemon's own `RemoteAgentContextSnapshot` producer
        // (#353): `ServerModel::new` reads its home skills via
        // `SkillManager::as_ref(ctx).home_skills()`. Must be registered before
        // ServerModel, and its own `SkillWatcher` depends on `WarpManagedPathsWatcher`
        // and (when the daemon host has a home directory) `HomeDirectoryWatcher` —
        // mirroring their non-daemon registration in `app/src/lib.rs`. The daemon
        // previously registered neither SkillManager nor these two watchers, so a
        // daemon's `SkillManager::new()` would have panicked the moment `SkillWatcher`
        // tried to subscribe to an unregistered `WarpManagedPathsWatcher`.
        ctx.add_singleton_model(crate::warp_managed_paths_watcher::WarpManagedPathsWatcher::new);
        if let Some(home_dir) = dirs::home_dir() {
            ctx.add_singleton_model(|ctx| {
                watcher::HomeDirectoryWatcher::new(home_dir, ctx)
            });
        } else {
            log::info!("Home directory not found; skipping HomeDirectoryWatcher registration");
        }
        ctx.add_singleton_model(crate::ai::skills::SkillManager::new);
        // ProjectContextModel backs the daemon's own `RemoteAgentContextSnapshot.
        // global_rules` producer (#575): `ServerModel::new`'s snapshot builder reads
        // this host's global rules via `ProjectContextModel::as_ref(ctx).global_rules()`.
        // Must be registered before ServerModel. Like SkillManager above, indexing
        // global rules depends on `HomeDirectoryWatcher` (registered above, when this
        // host has a home directory) — `index_global_rules` itself no-ops safely when
        // `dirs::home_dir()` is `None`, matching the guard already used for
        // `HomeDirectoryWatcher` above, so it's safe to call unconditionally here. The
        // daemon has no persisted project rules of its own (no local SQLite store), so
        // `new_from_persisted` is seeded with an empty list, matching the non-daemon
        // (app) path's shape but with nothing to hydrate.
        ctx.add_singleton_model(|ctx| {
            ::ai::project_context::model::ProjectContextModel::new_from_persisted(
                Vec::new(),
                ctx,
            )
        });
        ::ai::project_context::model::ProjectContextModel::handle(ctx)
            .update(ctx, |me, ctx| me.index_global_rules(ctx));
        // Codebase indexing on this host (Delta D2). Registered before
        // `ServerModel`, which subscribes to `CodebaseIndexManagerEvent` in its
        // own `new()` and would panic on an unregistered singleton — the same
        // failure mode `GlobalBufferModel` and `WarpManagedPathsWatcher` hit
        // above. `ServerModel` additionally checks `has_singleton_model`, so
        // getting this order wrong degrades rather than crashes.
        //
        // Differences from the pin's `initialize_app` registration
        // (`02b53fcd8:app/src/lib.rs:2380`):
        //
        // * `defer_persisted_index_restore()` is unconditional here. The pin
        //   applied it only for `LaunchMode::RemoteServerDaemon`, which is
        //   exactly what this is.
        // * There is no persisted index list to restore from: the daemon has no
        //   `PersistedWorkspace` and no app database. Indices are rebuilt from
        //   the on-disk snapshots instead, which is what `SnapshotStorage` is
        //   for.
        // * The store client is the daemon's own — see
        //   `codebase_index_store.rs` — not the app's, and not the pin's
        //   `ServerApi`.
        // * Limits come from the client on `Initialize`, so the values here are
        //   only the pre-handshake defaults; `apply_codebase_index_limits`
        //   replaces them.
        #[cfg(feature = "local_fs")]
        {
            use ::ai::index::full_source_code_embedding::manager::{
                CodebaseIndexManager, CodebaseIndexManagerConfig,
            };
            use ::ai::index::full_source_code_embedding::SnapshotStorage;

            let data_dir = daemon_codebase_index_data_dir(&identity_key);
            let store_client = codebase_index_store::build_daemon_store_client(&data_dir);
            let snapshot_storage =
                SnapshotStorage::from_dir(data_dir.join("cache").join("codebase_index_snapshots"));
            if snapshot_storage.is_none() {
                log::warn!(
                    "Daemon could not open a codebase index snapshot directory under {}; \
                     indices will be rebuilt from scratch on every restart",
                    data_dir.display()
                );
            }
            ctx.add_singleton_model(move |ctx| {
                let indexing_enabled =
                    warp_core::features::FeatureFlag::RemoteCodebaseIndexing.is_enabled();
                let config = CodebaseIndexManagerConfig::new(
                    Vec::new(),
                    None,
                    DAEMON_DEFAULT_MAX_FILES_PER_REPO,
                    DAEMON_DEFAULT_EMBEDDING_BATCH_SIZE,
                    store_client
                        as std::sync::Arc<
                            dyn ::ai::index::full_source_code_embedding::store_client::StoreClient,
                        >,
                    indexing_enabled,
                )
                .defer_persisted_index_restore();
                CodebaseIndexManager::new_with_snapshot_storage(config, snapshot_storage, ctx)
            });
        }
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
