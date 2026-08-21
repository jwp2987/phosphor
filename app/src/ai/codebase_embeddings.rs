//! The local half of the codebase index's BYOP `StoreClient`: a SQLite-backed
//! [`VectorStore`], plus the factory that assembles the whole client.
//!
//! At the pin the merkle-node registry and the embedding vectors lived on
//! Warp's server; `crates/ai` only ever held a trait. This fork stores both in
//! the app's own SQLite database — the tables come from
//! `crates/persistence/migrations/2026-08-10-000000_add_codebase_index_vectors`.
//!
//! # How writes and reads are split, and why
//!
//! Writes go through `ModelEvent`, the app's single-writer channel, exactly like
//! every other table. Reads use a separate read-only connection, as
//! `BlocklistAIHistoryModel` does. That split is the house rule
//! (`app/src/persistence/sqlite.rs:365`: "We want only one write connection to
//! exist"), and breaking it for this table would risk `SQLITE_BUSY` against the
//! writer thread.
//!
//! **The consequence, stated because it is a real behavioural difference from
//! the pin:** a write is not visible to a read until the writer thread drains
//! it. The pin's `StoreClient` calls were synchronous round-trips, so
//! write-then-read was ordered. Here it is not. Three places could notice:
//!
//! * A merkle sync writes nodes, then a *later* sync asks which nodes are
//!   known. If the write has not landed, the node is reported unknown and its
//!   subtree is walked again. Redundant work, never a wrong index.
//! * A query issued in the seconds after an index finishes may see fewer
//!   vectors than were just written, so it returns fewer candidates. It never
//!   returns a *wrong* candidate, because scoping is structural.
//! * The search index (`codebase_index_node_summaries`) is written at the end of
//!   a sync and read by the next query. If it has not landed, the query sees no
//!   summaries and falls back to opening every subtree — which is the
//!   pre-index behaviour, so it is slow rather than wrong.
//!   `LocalStoreClient::build_search_index` returns the summaries it computed so
//!   that the query which triggers a build never has to wait for its own write.
//!
//! None of them can corrupt the index: every table here is keyed by a hash that
//! covers the thing it describes, and every write is an idempotent upsert.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

use ai::index::full_source_code_embedding::local_store_client::{
    EmbeddingProvider, LocalCodebaseContextConfig, LocalStoreClient, RerankProvider, VectorStore,
};
use ai::index::full_source_code_embedding::store_client::{IntermediateNode, StoreClient};
use ai::index::full_source_code_embedding::vector_index::NodeSummary;
use ai::index::full_source_code_embedding::{
    CodebaseContextConfig, ContentHash, EmbeddingConfig, Error as IndexError, Fragment, NodeHash,
    RepoMetadata,
};
use anyhow::{Context, anyhow};
use async_trait::async_trait;
use diesel::SqliteConnection;
use warpui::{AppContext, SingletonEntity};

use crate::ai::agent_providers::embeddings::{
    EmbeddingEndpoints, HttpEmbeddingProvider, HttpRerankProvider,
    resolve_configured_embedding_model, resolve_embedding_endpoints,
};
use crate::persistence::{
    ModelEvent, codebase_index_children, codebase_index_node_summaries, codebase_index_vectors,
    database_file_path, establish_ro_connection, known_codebase_index_hashes,
};

/// Encodes a vector as little-endian `f32` bytes.
fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Decodes what [`encode_vector`] wrote, checking it against the recorded width.
///
/// A blob whose length disagrees with `dimensions` is a corrupt row; it is
/// reported rather than truncated, because a silently shortened vector would
/// still score — badly, and unfalsifiably.
fn decode_vector(bytes: &[u8], dimensions: i32) -> anyhow::Result<Vec<f32>> {
    let expected = usize::try_from(dimensions).unwrap_or(0);
    if bytes.len() != expected * 4 {
        return Err(anyhow!(
            "stored vector is {} bytes but claims {expected} dimensions",
            bytes.len()
        ));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect())
}

/// A [`VectorStore`] over the app's SQLite database.
pub struct SqliteVectorStore {
    /// `None` when persistence is unavailable (e.g. the DB could not be opened).
    /// Every method then degrades to "store nothing, know nothing", which makes
    /// the index re-sync rather than answer from a store it cannot read.
    writer: Option<SyncSender<ModelEvent>>,
    reader: Option<Arc<Mutex<SqliteConnection>>>,
}

impl SqliteVectorStore {
    pub fn new(
        writer: Option<SyncSender<ModelEvent>>,
        reader: Option<Arc<Mutex<SqliteConnection>>>,
    ) -> Self {
        Self { writer, reader }
    }

    /// Builds one from an already-obtained persistence writer.
    ///
    /// The writer is passed in rather than looked up in the context because
    /// this store is built partway through `initialize_app`, while
    /// `PersistenceWriter` is registered at the very end of it — the pin does
    /// the same (`02b53fcd8:app/src/lib.rs:2438`), and the pin's store client
    /// got away with it because it took a `server_api_provider` instead. Asking
    /// the context for the singleton here panicked on every startup, before any
    /// window opened.
    pub fn with_writer(writer: Option<SyncSender<ModelEvent>>) -> Self {
        let reader = database_file_path().to_str().and_then(|db_url| {
            establish_ro_connection(db_url)
                .inspect_err(|error| {
                    log::warn!("Codebase index vector store has no read connection: {error:#}");
                })
                .ok()
                .map(|conn| Arc::new(Mutex::new(conn)))
        });
        Self::new(writer, reader)
    }

    fn send(&self, event: ModelEvent) -> anyhow::Result<()> {
        let Some(writer) = &self.writer else {
            return Err(anyhow!("no persistence writer for the codebase index"));
        };
        writer
            .send(event)
            .context("failed to queue a codebase index write")
    }

    /// Runs `f` against the read-only connection.
    fn read<T>(
        &self,
        f: impl FnOnce(&mut SqliteConnection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let Some(reader) = &self.reader else {
            return Err(anyhow!("no read connection for the codebase index"));
        };
        let mut conn = reader
            .lock()
            .map_err(|_| anyhow!("codebase index read connection is poisoned"))?;
        f(&mut conn)
    }
}

impl VectorStore for SqliteVectorStore {
    fn record_nodes(&self, space: &str, nodes: &[IntermediateNode]) -> anyhow::Result<()> {
        if nodes.is_empty() {
            return Ok(());
        }

        let rows = nodes
            .iter()
            .map(|node| {
                let children: Vec<String> = node.children.iter().map(ToString::to_string).collect();
                let json = serde_json::to_string(&children)
                    .context("failed to encode merkle node children")?;
                Ok((node.hash.to_string(), json))
            })
            .collect::<anyhow::Result<Vec<_>>>()?;

        self.send(ModelEvent::UpsertCodebaseIndexNodes {
            embedding_space: space.to_owned(),
            nodes: rows,
        })
    }

    fn known_hashes(&self, space: &str, hashes: &[NodeHash]) -> anyhow::Result<HashSet<NodeHash>> {
        if hashes.is_empty() {
            return Ok(HashSet::new());
        }

        let as_strings: Vec<String> = hashes.iter().map(ToString::to_string).collect();
        let known = self.read(|conn| {
            known_codebase_index_hashes(conn, space, &as_strings)
                .context("failed to read known codebase index hashes")
        })?;

        Ok(hashes
            .iter()
            .filter(|hash| known.contains(&hash.to_string()))
            .cloned()
            .collect())
    }

    fn record_embeddings(
        &self,
        space: &str,
        embeddings: &[(ContentHash, Vec<f32>)],
    ) -> anyhow::Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        let rows = embeddings
            .iter()
            .map(|(hash, vector)| {
                (
                    hash.to_string(),
                    i32::try_from(vector.len()).unwrap_or(i32::MAX),
                    encode_vector(vector),
                )
            })
            .collect();

        self.send(ModelEvent::UpsertCodebaseIndexEmbeddings {
            embedding_space: space.to_owned(),
            embeddings: rows,
        })
    }

    fn children_of(
        &self,
        space: &str,
        hashes: &[NodeHash],
    ) -> anyhow::Result<HashMap<NodeHash, Vec<NodeHash>>> {
        if hashes.is_empty() {
            return Ok(HashMap::new());
        }

        let as_strings: Vec<String> = hashes.iter().map(ToString::to_string).collect();
        let rows = self.read(|conn| {
            codebase_index_children(conn, space, &as_strings)
                .context("failed to read codebase index nodes")
        })?;

        let mut out = HashMap::with_capacity(rows.len());
        for (hash, json) in rows {
            let Ok(node_hash) = hash.parse::<NodeHash>() else {
                log::warn!("Skipping unparseable node hash in the codebase index: {hash}");
                continue;
            };
            let children: Vec<String> =
                serde_json::from_str(&json).context("failed to decode merkle node children")?;
            out.insert(
                node_hash,
                children
                    .into_iter()
                    .filter_map(|child| child.parse::<NodeHash>().ok())
                    .collect(),
            );
        }
        Ok(out)
    }

    fn record_node_summaries(
        &self,
        space: &str,
        summaries: &[(NodeHash, NodeSummary)],
    ) -> anyhow::Result<()> {
        if summaries.is_empty() {
            return Ok(());
        }

        let rows = summaries
            .iter()
            .map(|(hash, summary)| {
                (
                    hash.to_string(),
                    i32::try_from(summary.leaf_count).unwrap_or(i32::MAX),
                    summary.radius,
                    i32::try_from(summary.mean.len()).unwrap_or(i32::MAX),
                    encode_vector(&summary.mean),
                )
            })
            .collect();

        self.send(ModelEvent::UpsertCodebaseIndexNodeSummaries {
            embedding_space: space.to_owned(),
            summaries: rows,
        })
    }

    fn node_summaries_for(
        &self,
        space: &str,
        hashes: &[NodeHash],
    ) -> anyhow::Result<Vec<(NodeHash, NodeSummary)>> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }

        let as_strings: Vec<String> = hashes.iter().map(ToString::to_string).collect();
        let rows = self.read(|conn| {
            codebase_index_node_summaries(conn, space, &as_strings)
                .context("failed to read codebase index node summaries")
        })?;

        let mut out = Vec::with_capacity(rows.len());
        for (hash, leaf_count, radius, dimensions, bytes) in rows {
            let Ok(node_hash) = hash.parse::<NodeHash>() else {
                log::warn!("Skipping unparseable node hash in the codebase index: {hash}");
                continue;
            };
            match decode_vector(&bytes, dimensions) {
                // A corrupt summary is skipped rather than fatal: the subtree is
                // then unbounded, so it gets opened. Slower, never wrong, and
                // the next index build overwrites the row.
                Err(error) => log::warn!("Skipping corrupt node summary for {hash}: {error:#}"),
                Ok(mean) => out.push((
                    node_hash,
                    NodeSummary {
                        mean,
                        leaf_count: u32::try_from(leaf_count).unwrap_or(0),
                        radius,
                    },
                )),
            }
        }
        Ok(out)
    }

    fn vectors_for(
        &self,
        space: &str,
        hashes: &[ContentHash],
    ) -> anyhow::Result<Vec<(ContentHash, Vec<f32>)>> {
        if hashes.is_empty() {
            return Ok(Vec::new());
        }

        let as_strings: Vec<String> = hashes.iter().map(ToString::to_string).collect();
        let rows = self.read(|conn| {
            codebase_index_vectors(conn, space, &as_strings)
                .context("failed to read codebase index vectors")
        })?;

        let mut out = Vec::with_capacity(rows.len());
        for (hash, dimensions, bytes) in rows {
            let Ok(content_hash) = hash.parse::<ContentHash>() else {
                log::warn!("Skipping unparseable content hash in the vector store: {hash}");
                continue;
            };
            match decode_vector(&bytes, dimensions) {
                // A corrupt row is skipped, not fatal: the fragment simply
                // cannot be scored, and the next sync overwrites it.
                Err(error) => log::warn!("Skipping corrupt stored vector for {hash}: {error:#}"),
                Ok(vector) => out.push((content_hash, vector)),
            }
        }
        Ok(out)
    }
}

/// The [`StoreClient`] the app hands to `CodebaseIndexManager`.
///
/// # Why this exists at all
///
/// The manager holds one `Arc<dyn StoreClient>` for the life of the process, so
/// whatever this returns is what indexing uses until the app is restarted. The
/// user's embedding configuration, by contrast, is editable at any moment:
/// providers are added under Settings > AI and API keys are rotated in the
/// keychain. Resolving the endpoint once, at startup, made a provider
/// configured after launch invisible to indexing — the settings page reported
/// the model as live while every index attempt failed with
/// `Error::NoEmbeddingProvider` — and made a rotated key go unnoticed until the
/// next restart.
///
/// So the resolved configuration lives behind a `Mutex` and every call
/// assembles a fresh [`LocalStoreClient`] over it — three `Arc` clones and a
/// `Copy` config. This is the same shape as `DaemonStoreClient`
/// (`app/src/remote_server/codebase_index_store.rs`), which reconfigures itself
/// when a client sends new preferences; here the trigger is a settings change
/// instead of a wire message.
/// [`subscribe_to_codebase_indexing_configuration`] subscribes to `AISettings`,
/// `AgentProviderSecrets` and `CodeSettings` and calls
/// [`refresh_from_settings`][Self::refresh_from_settings] — a predicate ported
/// without its invalidation events is not ported.
///
/// Rebuilding the client per call also means a *model* change is picked up:
/// `CodebaseIndexSyncOperation::full_sync` asks
/// [`codebase_context_config`][StoreClient::codebase_context_config] which
/// model to use at the start of every sync, and that answer now comes from the
/// current settings rather than from whatever was configured at launch.
pub struct RefreshingStoreClient {
    provider: Arc<HttpEmbeddingProvider>,
    store: Arc<SqliteVectorStore>,
    configuration: Mutex<StoreClientConfiguration>,
}

/// What the last refresh resolved.
#[derive(Default)]
struct StoreClientConfiguration {
    /// The embedding model, if the user has a usable provider for one.
    ///
    /// The outer `Option` is "has a refresh happened yet", kept apart from the
    /// inner "did that refresh find anything" so the first refresh always logs
    /// what it found — including the common case of finding nothing, which is
    /// the state a fresh install is in and the one users need told about.
    resolved_model: Option<Option<EmbeddingConfig>>,
    /// The same, for the optional reranking model, held as its id so a change
    /// can be reported without keeping a second copy of the provider.
    resolved_reranker: Option<Option<&'static str>>,
    reranker: Option<Arc<dyn RerankProvider>>,
}

impl StoreClientConfiguration {
    /// The model to index with: whatever the user configured, falling back to
    /// the pin's default so the index has a well-defined storage space even
    /// before setup.
    fn embedding_config(&self) -> EmbeddingConfig {
        self.resolved_model.flatten().unwrap_or_default()
    }
}

impl RefreshingStoreClient {
    fn new(provider: Arc<HttpEmbeddingProvider>, store: Arc<SqliteVectorStore>) -> Self {
        Self {
            provider,
            store,
            configuration: Mutex::new(StoreClientConfiguration::default()),
        }
    }

    /// Re-resolves the embedding endpoint, model and reranker from the app's
    /// current settings.
    ///
    /// Call this from every event that can change the answer. The endpoint is
    /// replaced unconditionally rather than only when the *model* changes,
    /// because a rotated API key leaves the model identical and the credential
    /// different.
    pub fn refresh_from_settings(&self, app: &AppContext) {
        let endpoints = resolve_embedding_endpoints(app);
        let reranker = HttpRerankProvider::from_app(app);
        let resolved_reranker = reranker.as_ref().map(HttpRerankProvider::model_id);

        self.reconfigure(
            endpoints,
            resolved_reranker,
            reranker.map(|reranker| Arc::new(reranker) as Arc<dyn RerankProvider>),
        );
    }

    /// The settings-free half of [`refresh_from_settings`][Self::refresh_from_settings],
    /// so the refresh can be tested without an `AppContext`.
    ///
    /// `endpoints` carries both halves of the answer: which model a *new* index
    /// should use ([`EmbeddingEndpoints::preferred_model`]) and where a request
    /// for any configured model goes. They travel together because a shared
    /// endpoint with a separately-cached model is exactly the pair that can
    /// disagree — see [`EmbeddingEndpoints`].
    fn reconfigure(
        &self,
        endpoints: EmbeddingEndpoints,
        resolved_reranker: Option<&'static str>,
        reranker: Option<Arc<dyn RerankProvider>>,
    ) {
        let resolved_model = endpoints.preferred_model();
        self.provider.set_endpoints(endpoints);

        let mut configuration = self
            .configuration
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        if configuration.resolved_model != Some(resolved_model) {
            // `previous` is the model a refresh has already resolved in this
            // process, as distinct from "no refresh has happened yet"; only the
            // former means work has been paid for under the old storage key.
            let previous = configuration.resolved_model.flatten();
            match (previous, resolved_model) {
                // A model *switch*, with an index already keyed to the old one.
                //
                // `EmbeddingConfig::storage_key` is what every vector row and
                // every `known_hashes` lookup is keyed by, so the next full sync
                // finds nothing under the new key and re-embeds every indexed
                // repository from scratch — on the user's own provider quota,
                // which they are billed for.
                //
                // # Why this is only a warning, and what is still owed
                //
                // The switch is not something the user asked for: the model is
                // whichever entry of `SUPPORTED_EMBEDDING_MODELS` resolves
                // first, so *adding* a second provider can re-key an index that
                // was working. That sits badly against the argument used to
                // decline the pin's index consent banner (see `DECLINED.md`),
                // which rests on `codebase_context_enabled` being opt-in
                // precisely because indexing spends the user's money.
                //
                // A log line is not consent, and this is deliberately recorded
                // as a warning rather than left at `info`: it is the loudest
                // signal available from here without reaching into UI files.
                // The complete fix is *not* a prompt — it is to stop making the
                // choice on the user's behalf, by preferring the model the
                // vector store already holds rows for whenever that model is
                // still configured, and only then falling back to preference
                // order. That is a `SqliteVectorStore` query and a change to
                // what `preferred_model` means; it is filed rather than done
                // here because it changes indexing behaviour, not just its
                // reporting. Until it lands, the message must say what it will
                // cost and how to avoid it.
                (Some(previous), Some(config)) => log::warn!(
                    "Codebase indexing is switching from {} to {}. These are different vector \
                     spaces, so the next full sync will re-embed every indexed repository from \
                     scratch against your own provider, at your own cost. To keep indexing with \
                     {}, disable or remove {} under Settings > AI.",
                    previous.model_id(),
                    config.model_id(),
                    previous.model_id(),
                    config.model_id()
                ),
                // First model resolved in this process. Nothing has been
                // embedded under another key by this client, so there is no
                // re-embed to warn about.
                (None, Some(config)) => {
                    log::info!("Codebase indexing will embed with {}", config.model_id())
                }
                (_, None) => log::info!(
                    "No embedding provider is configured; codebase indexing will report \
                     NoEmbeddingProvider until one is added under Settings > AI"
                ),
            }
            configuration.resolved_model = Some(resolved_model);
        }

        if configuration.resolved_reranker != Some(resolved_reranker) {
            match resolved_reranker {
                Some(model_id) => {
                    log::info!("Codebase index reranking will use {model_id}")
                }
                // The pin reranked with a server-side cross-encoder. Where the
                // user's own provider sells one, use it; where it does not,
                // `LocalStoreClient` falls back to hybrid vector + lexical
                // scoring rather than requiring a model nobody configured.
                None => log::info!(
                    "No reranking model is configured; codebase search will rerank with \
                     hybrid vector + lexical scoring. Adding a rerank model (e.g. \
                     rerank-2.5) to a provider under Settings > AI enables cross-encoder \
                     reranking instead."
                ),
            }
            configuration.resolved_reranker = Some(resolved_reranker);
        }

        configuration.reranker = reranker;
    }

    /// Assembles a client over the configuration as it stands right now.
    fn client(&self) -> LocalStoreClient {
        let (embedding_config, reranker) = {
            let configuration = self
                .configuration
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (
                configuration.embedding_config(),
                configuration.reranker.clone(),
            )
        };

        LocalStoreClient::new(
            Arc::clone(&self.provider) as Arc<dyn EmbeddingProvider>,
            Arc::clone(&self.store) as Arc<dyn VectorStore>,
            LocalCodebaseContextConfig {
                embedding_config,
                ..LocalCodebaseContextConfig::default()
            },
        )
        .with_rerank_provider(reranker)
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl StoreClient for RefreshingStoreClient {
    async fn update_intermediate_nodes(
        &self,
        embedding_config: EmbeddingConfig,
        nodes: Vec<IntermediateNode>,
    ) -> Result<HashMap<NodeHash, bool>, IndexError> {
        self.client()
            .update_intermediate_nodes(embedding_config, nodes)
            .await
    }

    async fn generate_embeddings(
        &self,
        embedding_config: EmbeddingConfig,
        fragments: Vec<Fragment>,
        root_hash: NodeHash,
        repo_metadata: RepoMetadata,
    ) -> Result<HashMap<ContentHash, bool>, IndexError> {
        self.client()
            .generate_embeddings(embedding_config, fragments, root_hash, repo_metadata)
            .await
    }

    async fn populate_merkle_tree_cache(
        &self,
        embedding_config: EmbeddingConfig,
        root_hash: NodeHash,
        repo_metadata: RepoMetadata,
    ) -> Result<bool, IndexError> {
        self.client()
            .populate_merkle_tree_cache(embedding_config, root_hash, repo_metadata)
            .await
    }

    async fn sync_merkle_tree(
        &self,
        nodes: Vec<NodeHash>,
        embedding_config: EmbeddingConfig,
    ) -> Result<HashSet<NodeHash>, IndexError> {
        self.client()
            .sync_merkle_tree(nodes, embedding_config)
            .await
    }

    async fn rerank_fragments(
        &self,
        query: String,
        fragments: Vec<Fragment>,
    ) -> Result<Vec<Fragment>, IndexError> {
        self.client().rerank_fragments(query, fragments).await
    }

    async fn get_relevant_fragments(
        &self,
        embedding_config: EmbeddingConfig,
        query: String,
        root_hash: NodeHash,
        repo_metadata: RepoMetadata,
    ) -> Result<Vec<ContentHash>, IndexError> {
        self.client()
            .get_relevant_fragments(embedding_config, query, root_hash, repo_metadata)
            .await
    }

    async fn codebase_context_config(&self) -> Result<CodebaseContextConfig, IndexError> {
        self.client().codebase_context_config().await
    }
}

/// Builds the codebase index's `StoreClient` from the user's configuration.
///
/// Returns a client whichever way the configuration falls: when no provider is
/// configured, the client still works for everything that does not need one
/// (recording nodes, answering "what do you already have"), and the paths that
/// do need one fail with `Error::NoEmbeddingProvider`. That is deliberate — a
/// `None` here would have to be handled by every caller, and the natural
/// handling would be to skip indexing silently.
///
/// The concrete type is returned, not `Arc<dyn StoreClient>`, because the
/// caller has to keep a handle it can call
/// [`RefreshingStoreClient::refresh_from_settings`] on. Coerce it for the
/// manager's config.
///
/// `persistence_writer` is threaded in from the caller because the app's
/// `PersistenceWriter` singleton is not registered yet at the point the
/// codebase index is built. See [`SqliteVectorStore::with_writer`].
pub fn build_store_client(
    app: &AppContext,
    persistence_writer: Option<SyncSender<ModelEvent>>,
) -> Arc<RefreshingStoreClient> {
    let client = Arc::new(RefreshingStoreClient::new(
        // Deliberately unconfigured. `refresh_from_settings` below resolves the
        // endpoints and overwrites whatever this holds, so seeding it from the
        // app here would be a verdict computed and discarded — and, worse, a
        // second place where "what the endpoint is" gets decided. Startup and
        // every later settings change now take exactly one code path.
        Arc::new(HttpEmbeddingProvider::new(http_client::Client::new(), None)),
        Arc::new(SqliteVectorStore::with_writer(persistence_writer)),
    ));
    client.refresh_from_settings(app);
    client
}

/// Re-resolves everything that indexes a codebase from the app's current
/// settings.
///
/// Two consumers, one trigger. The local index's store client holds the
/// embedding endpoints the app posts to; the remote-server manager holds the
/// endpoint it ships to daemons. Both are resolved from the same settings, so
/// both are refreshed from the same place — a second mechanism for the second
/// consumer would drift.
///
/// Sending `None` to a daemon is not a silent failure: it clears its endpoint
/// and reports the index as `Unavailable`, and `codebase_indexing_ready`
/// explains why.
pub fn refresh_codebase_indexing_configuration(
    store_client: &RefreshingStoreClient,
    ctx: &mut AppContext,
) {
    store_client.refresh_from_settings(&*ctx);

    #[cfg(not(target_family = "wasm"))]
    {
        use remote_server::manager::RemoteServerManager;

        let preferences = remote_client_preferences(ctx);
        RemoteServerManager::handle(ctx).update(ctx, |manager, _| {
            manager.update_client_preferences(preferences);
        });
    }
}

/// Refreshes now, and on every event that can change the answer.
///
/// # Why these three subscriptions and not fewer
///
/// * `AISettings` — `agent_providers` is where an embedding model's base URL
///   comes from, and the global AI toggle is half of
///   `UserWorkspaces::is_codebase_context_enabled`.
/// * `AgentProviderSecrets` — the API key. A rotation leaves the provider list
///   and the model identical, so nothing else fires. Without this, indexing
///   keeps using a revoked key until restart, and a daemon keeps the one it was
///   handed. This is the same pair the LLM path subscribes to
///   (`app/src/ai/llms.rs`, `LLMPreferences::new`), for the same reason. It also
///   emits on `reload_from_secure_storage`, so a key written by another process
///   — the TUI, or a second window — is picked up too.
/// * `CodeSettings` — `codebase_context_enabled` is the other half of the
///   consent predicate, and the credential gate inside
///   `remote_client_preferences` is `should_use_codebase_indexing`, which
///   reads it. Subscribing to only one of the two settings groups would leave
///   the other half of that predicate without its invalidation event: turning
///   codebase indexing off would not retract the API key already sent to
///   connected daemons, and turning it on would not deliver one until the next
///   restart — so the disclosure in
///   `settings-code-remote-indexed-folders-desc` ("sent when you turn this on")
///   would be false in both directions.
///
/// Firing more often than strictly necessary is cheap and deliberate:
/// `update_client_preferences` pushes to every connected client and no-ops when
/// nothing changed, and the refresh is two settings reads and one borrow of the
/// in-memory key map.
///
/// This lives here, and not inline in `initialize_app`, so that the wiring is
/// reachable from a test. A predicate ported without its invalidation events is
/// not ported, and a fix that is only tested one layer below the wiring is not
/// tested — deleting the subscriptions must fail something. See
/// `wiring_tests` below.
pub fn subscribe_to_codebase_indexing_configuration(
    store_client: Arc<RefreshingStoreClient>,
    ctx: &mut AppContext,
) {
    refresh_codebase_indexing_configuration(&store_client, ctx);

    let client = Arc::clone(&store_client);
    ctx.subscribe_to_model(
        &crate::settings::AISettings::handle(ctx),
        move |_, _, ctx| {
            refresh_codebase_indexing_configuration(&client, ctx);
        },
    );

    let client = Arc::clone(&store_client);
    ctx.subscribe_to_model(
        &crate::ai::agent_providers::AgentProviderSecrets::handle(ctx),
        move |_, _, ctx| {
            refresh_codebase_indexing_configuration(&client, ctx);
        },
    );

    let client = store_client;
    ctx.subscribe_to_model(
        &crate::settings::CodeSettings::handle(ctx),
        move |_, _, ctx| {
            refresh_codebase_indexing_configuration(&client, ctx);
        },
    );
}

/// The embedding model the index will use, for callers that need to know before
/// building a client.
pub fn active_embedding_config(app: &AppContext) -> EmbeddingConfig {
    resolve_configured_embedding_model(app).unwrap_or_default()
}

/// What a remote-server daemon needs to index a repository on its own host:
/// the same limits the local index obeys, plus the user's embedding endpoint.
///
/// This is where the fork's BYOP substitution shows up on the wire. The pin
/// sent the daemon a Warp bearer token and let it reach the shared store; there
/// is no shared store here, so the daemon is given the endpoint directly. It is
/// resolved on this side because only the client has the settings and the
/// keychain.
///
/// Returns `embedding_provider: None` when nothing is configured — deliberately
/// not a default, because a daemon that embedded a whole repository against a
/// model the user never chose would produce vectors under a storage key nothing
/// queries — and also whenever the user has not turned remote codebase
/// indexing on, so the user's provider API key never leaves this machine for a
/// daemon that will never be asked to index anything.
#[cfg(not(target_family = "wasm"))]
pub fn remote_client_preferences(app: &AppContext) -> remote_server::client::ClientPreferences {
    use crate::ai::AIRequestUsageModel;
    use crate::ai::agent_providers::embeddings::resolve_embedding_endpoint;
    use crate::ai::codebase_auto_indexing::{
        CodebaseAutoIndexingSurface, should_use_codebase_indexing,
    };

    let limits = AIRequestUsageModel::as_ref(app).codebase_context_limits();
    let codebase_index_limits = Some(remote_server::proto::CodebaseIndexLimits {
        max_indices_allowed: limits.max_indices_allowed.map(|value| value as u64),
        max_files_per_repo: limits.max_files_per_repo as u64,
        embedding_generation_batch_size: limits.embedding_generation_batch_size as u64,
    });

    // The endpoint carries the user's own provider API key, read out of the
    // keychain, and `Initialize` ships it to whichever host the user chose to
    // install a daemon on. Only send it when remote indexing is actually in
    // use.
    //
    // The gate is `should_use_codebase_indexing(Remote, _)` and NOT
    // `FeatureFlag::RemoteCodebaseIndexing` on its own. That flag is listed in
    // `app/Cargo.toml`'s `default` feature set, so `is_enabled()` is a constant
    // `true` in every build this repo ships: gating on it alone is the same as
    // no gate at all, and an earlier version of this function did exactly that.
    // A compile-time feature cannot express consent that the user has not given
    // yet.
    //
    // `should_use_codebase_indexing` is the fork's runtime predicate and it
    // subsumes the flag: `FullSourceCodeEmbedding` (off unless the user asks
    // for it via `ZAP_UNSTABLE_FEATURES`) AND `RemoteCodebaseIndexing` AND
    // `UserWorkspaces::is_codebase_context_enabled` — the global AI toggle AND
    // `CodeSettings::codebase_context_enabled`, whose default is `false`
    // (`app/src/settings/code.rs`). It is also the exact predicate every caller
    // that can ask a daemon to index guards on
    // (`remote_server/codebase_index_model.rs`, `ai/codebase_retrieval.rs`),
    // which is what makes the two agree: the credential travels when, and only
    // when, a request that needs it can be made.
    //
    // Because the predicate reads two settings groups, the call sites in
    // `lib.rs` must re-run it on changes to BOTH `AISettings` and
    // `CodeSettings`; `update_client_preferences` pushes the new value to
    // already-connected sessions, so withdrawing consent also retracts the key
    // rather than only stopping the next handshake.
    //
    // Sending `None` is not a silent failure: the daemon clears its endpoint
    // and reports the index as `Unavailable`, and
    // `codebase_indexing_ready` returns the "no embedding provider has been
    // configured for this host" error, so the user sees why.
    let embedding_provider = remote_embedding_provider(
        should_use_codebase_indexing(CodebaseAutoIndexingSurface::Remote, app),
        || {
            resolve_configured_embedding_model(app).and_then(|config| {
                resolve_embedding_endpoint(app, config).map(|endpoint| {
                    remote_server::proto::EmbeddingProviderConfig {
                        base_url: endpoint.base_url,
                        api_key: endpoint.api_key,
                        embedding_storage_key: config.storage_key().to_string(),
                    }
                })
            })
        },
    );

    remote_server::client::ClientPreferences {
        codebase_index_limits,
        embedding_provider,
    }
}

/// The credential gate, split out so it can be tested without an `AppContext`.
///
/// `resolve` is a closure rather than a value on purpose: when
/// `remote_indexing_in_use` is false the user's API key must not merely be
/// dropped after being read, it must never be read at all. A "compute the
/// verdict and then discard it" shape would still pull the key out of the
/// keychain on every settings change.
#[cfg(not(target_family = "wasm"))]
fn remote_embedding_provider(
    remote_indexing_in_use: bool,
    resolve: impl FnOnce() -> Option<remote_server::proto::EmbeddingProviderConfig>,
) -> Option<remote_server::proto::EmbeddingProviderConfig> {
    if !remote_indexing_in_use {
        return None;
    }
    resolve()
}

/// The endpoint and model must follow configuration changes, not freeze at
/// startup.
///
/// The defect these cover: `build_store_client` used to resolve the endpoint
/// once, inside `add_singleton_model`, and `CodebaseIndexManager` holds the
/// resulting `Arc<dyn StoreClient>` for the life of the process. Configuring a
/// provider after launch therefore left indexing answering
/// `NoEmbeddingProvider` until restart, while the settings page reported the
/// model as live. A rotated API key was ignored the same way.
///
/// They deliberately go through the `StoreClient` trait rather than poking
/// `HttpEmbeddingProvider::set_endpoints` directly — the single-endpoint setter
/// is already covered in `agent_providers/embeddings_tests.rs`, and it passed
/// while the defect was live. What was missing is that a refresh reaches *the
/// client the manager holds*, so that is what is asserted.
///
/// These stop at `reconfigure`, which is where the resolution is applied. What
/// makes `reconfigure` get *called* — the subscriptions in `initialize_app` —
/// is a separate layer with a separate failure mode, and is covered by
/// `wiring_tests` below. Neither set substitutes for the other: this one would
/// still pass with every subscription deleted.
///
/// No request leaves the machine: an `http://` endpoint that is neither loopback
/// nor on a private network is refused by `agent_providers::embeddings`' two
/// transport guards -- the credential rule when a key is present, the payload
/// rule regardless -- before the socket is opened, so "the endpoint took effect"
/// is observable offline and deterministically. The hosts below are
/// `*.example.invalid`, which is public by both rules.
#[cfg(test)]
mod endpoint_refresh_tests {
    use std::path::PathBuf;

    use futures::executor::block_on;
    use http_client::Client;
    use string_offset::ByteOffset;

    use super::*;
    use crate::ai::agent_providers::embeddings::EmbeddingEndpoint;

    fn an_unconfigured_client() -> RefreshingStoreClient {
        RefreshingStoreClient::new(
            Arc::new(HttpEmbeddingProvider::new(Client::new(), None)),
            // No writer and no reader: every store call degrades to "store
            // nothing, know nothing", which is enough because the provider is
            // consulted before the store on the path under test.
            Arc::new(SqliteVectorStore::new(None, None)),
        )
    }

    fn a_fragment() -> Fragment {
        let content = "fn main() {}".to_owned();
        let content_hash = ContentHash::from_content(&content);
        let length = content.len();
        Fragment::from_byte_range(
            content,
            content_hash,
            PathBuf::from("/repo/src/main.rs"),
            ByteOffset::from(0)..ByteOffset::from(length),
        )
    }

    /// Asks the client to embed one fragment *as a sync that cached
    /// `embedding_config` would*, and reports how it failed. It always fails:
    /// with no endpoint because there is nothing to call, and with one because
    /// that endpoint is refused before any request is made.
    fn embed_error_for(
        client: &RefreshingStoreClient,
        embedding_config: EmbeddingConfig,
    ) -> IndexError {
        let fragment = a_fragment();
        let root_hash = NodeHash::from(fragment.content_hash().clone());
        block_on(client.generate_embeddings(
            embedding_config,
            vec![fragment],
            root_hash,
            RepoMetadata { path: None },
        ))
        .expect_err("no reachable provider is configured in this test")
    }

    fn embed_error(client: &RefreshingStoreClient) -> IndexError {
        embed_error_for(client, EmbeddingConfig::default())
    }

    /// A public `http://` endpoint with a key: both transport guards refuse it
    /// before a socket is opened, so "which endpoint would this request have
    /// gone to" is observable offline and deterministically. Only the error's
    /// *host* is asserted on, so it does not matter which of the two reports.
    fn an_endpoint(host: &str) -> EmbeddingEndpoint {
        EmbeddingEndpoint {
            base_url: format!("http://{host}/v1"),
            api_key: "sk-secret".to_owned(),
        }
    }

    #[test]
    fn a_provider_configured_after_startup_reaches_the_client_the_manager_holds() {
        let client = an_unconfigured_client();

        assert!(
            matches!(embed_error(&client), IndexError::NoEmbeddingProvider { .. }),
            "with nothing configured the client must report a missing provider"
        );

        client.reconfigure(
            EmbeddingEndpoints::single(
                EmbeddingConfig::default(),
                EmbeddingEndpoint {
                    base_url: "http://embeddings.example.invalid/v1".to_owned(),
                    api_key: "sk-configured-after-launch".to_owned(),
                },
            ),
            None,
            None,
        );

        let error = embed_error(&client);
        assert!(
            !matches!(error, IndexError::NoEmbeddingProvider { .. }),
            "after the refresh the client must use the new endpoint, not keep \
             reporting the startup state; got {error}"
        );
        assert!(
            error.to_string().contains("embeddings.example.invalid"),
            "the error must name the endpoint the refresh installed; got {error}"
        );
    }

    #[test]
    fn an_endpoint_edit_replaces_the_one_resolved_at_startup() {
        // The model is unchanged across this refresh, so a client that reacted
        // only to *model* changes would keep the stale endpoint. That is the
        // shape of the original defect, and it is why `reconfigure` sets the
        // endpoint unconditionally.
        //
        // An API-key rotation travels this same `set_endpoints` call. It is not
        // asserted separately here because a key never appears in an error
        // message, so distinguishing two keys would need a real request; the
        // base URL is observable offline and exercises the identical code path.
        // `wiring_tests` does assert a key change end to end, using the
        // plaintext-bearer guard as the observable.
        let client = an_unconfigured_client();

        client.reconfigure(
            EmbeddingEndpoints::single(
                EmbeddingConfig::default(),
                an_endpoint("first.example.invalid"),
            ),
            None,
            None,
        );
        client.reconfigure(
            EmbeddingEndpoints::single(
                EmbeddingConfig::default(),
                an_endpoint("second.example.invalid"),
            ),
            None,
            None,
        );

        let error = embed_error(&client).to_string();
        assert!(
            error.contains("second.example.invalid"),
            "the second refresh must replace the first endpoint; got {error}"
        );
        assert!(
            !error.contains("first.example.invalid"),
            "the endpoint resolved earlier must not survive the refresh; got {error}"
        );
    }

    #[test]
    fn the_model_a_sync_asks_for_follows_the_configuration() {
        // `CodebaseIndexSyncOperation::full_sync` calls `codebase_context_config`
        // at the start of every sync and indexes with whatever it answers, so
        // this is how a model change reaches an already-running manager.
        let client = an_unconfigured_client();

        assert_eq!(
            block_on(client.codebase_context_config())
                .expect("the local config never fails")
                .embedding_config,
            EmbeddingConfig::default(),
            "an unconfigured client falls back to the pin's default model"
        );

        client.reconfigure(
            EmbeddingEndpoints::single(
                EmbeddingConfig::OpenAiTextSmall3_256,
                EmbeddingEndpoint {
                    base_url: "http://openai.example.invalid/v1".to_owned(),
                    api_key: String::new(),
                },
            ),
            None,
            None,
        );

        assert_eq!(
            block_on(client.codebase_context_config())
                .expect("the local config never fails")
                .embedding_config,
            EmbeddingConfig::OpenAiTextSmall3_256,
            "the next sync must use the model the user has now configured"
        );
    }

    #[test]
    fn a_newly_preferred_model_does_not_hijack_the_one_an_index_is_already_using() {
        // `CodebaseIndex` caches the model it was built with and only re-reads
        // it on a *full* sync, which is twenty minutes apart. Every incremental
        // sync and every query in that window passes the *old* model down to
        // the provider, while the refresh has already moved the preferred model
        // to whatever `SUPPORTED_EMBEDDING_MODELS` ranks first.
        //
        // With one shared endpoint slot those requests would be posted to a
        // provider that does not serve that model, and every incremental update
        // would fail for up to twenty minutes with only telemetry to show for
        // it. Per-model routing is what closes that window.
        let client = an_unconfigured_client();

        client.reconfigure(
            [
                (
                    EmbeddingConfig::Voyage3_5_512,
                    an_endpoint("voyage.example.invalid"),
                ),
                (
                    EmbeddingConfig::OpenAiTextSmall3_256,
                    an_endpoint("openai.example.invalid"),
                ),
            ]
            .into_iter()
            .collect(),
            None,
            None,
        );

        assert_eq!(
            block_on(client.codebase_context_config())
                .expect("the local config never fails")
                .embedding_config,
            EmbeddingConfig::Voyage3_5_512,
            "a new index is built with the preferred model"
        );

        let error = embed_error_for(&client, EmbeddingConfig::OpenAiTextSmall3_256).to_string();
        assert!(
            error.contains("openai.example.invalid"),
            "a sync still asking for the previous model must reach the provider that \
             serves it, not the newly-preferred one; got {error}"
        );
        assert!(
            !error.contains("voyage.example.invalid"),
            "the newly-preferred provider must not receive a model it does not serve; \
             got {error}"
        );
    }

    #[test]
    fn a_model_whose_provider_was_removed_reports_that_model_by_name() {
        // The other half of routing: when the old model genuinely stops being
        // served, the failure must be one loud, self-describing error rather
        // than a stream of HTTP 400s from a provider that was never asked about
        // it.
        let client = an_unconfigured_client();

        client.reconfigure(
            EmbeddingEndpoints::single(
                EmbeddingConfig::Voyage3_5_512,
                an_endpoint("voyage.example.invalid"),
            ),
            None,
            None,
        );

        assert!(
            matches!(
                embed_error_for(&client, EmbeddingConfig::OpenAiTextSmall3_256),
                IndexError::NoEmbeddingProvider {
                    model: "text-embedding-3-small"
                }
            ),
            "the error must name the model that is no longer configured"
        );
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod byop_key_gate_tests {
    use super::remote_embedding_provider;

    fn a_credential() -> remote_server::proto::EmbeddingProviderConfig {
        remote_server::proto::EmbeddingProviderConfig {
            base_url: "https://api.example.invalid/v1".to_string(),
            api_key: "sk-secret".to_string(),
            embedding_storage_key: "storage-key".to_string(),
        }
    }

    #[test]
    fn the_api_key_is_absent_and_unread_when_remote_indexing_is_not_in_use() {
        let mut resolved = false;

        let provider = remote_embedding_provider(false, || {
            resolved = true;
            Some(a_credential())
        });

        assert!(
            provider.is_none(),
            "no `EmbeddingProviderConfig` may be built when the runtime predicate is false; \
             `Initialize`/`UpdatePreferences` would carry the user's provider API key to the \
             remote host"
        );
        assert!(
            !resolved,
            "the keychain must not even be consulted when the predicate is false"
        );
    }

    #[test]
    fn the_api_key_is_sent_when_remote_indexing_is_in_use() {
        // The negative case above is only meaningful if the positive case is
        // reachable: a gate that always answers `None` would pass it too.
        let provider = remote_embedding_provider(true, || Some(a_credential()));

        assert_eq!(
            provider.map(|config| config.api_key),
            Some("sk-secret".to_string())
        );
    }

    #[test]
    fn an_unconfigured_provider_still_yields_none_when_indexing_is_in_use() {
        assert!(remote_embedding_provider(true, || None).is_none());
    }
}

/// The subscriptions themselves, not the helper below them.
///
/// The defect this file exists to fix was in `initialize_app`: the endpoint was
/// resolved once and never again. Tests that call
/// [`RefreshingStoreClient::reconfigure`] directly cannot catch that — the
/// equivalent tests passed while the defect was live, because `reconfigure` was
/// never the broken part. Deleting the `ctx.subscribe_to_model` calls in
/// [`subscribe_to_codebase_indexing_configuration`] has to fail something, and
/// this is that something.
///
/// Both tests install the wiring *before* the thing they change, so the initial
/// refresh inside cannot be what satisfies the assertion. Each changes exactly
/// one of the two singletons, so each subscription is pinned individually
/// rather than by whichever of them happens to fire first.
///
/// No request leaves the machine on either path. See `A_HOST` and
/// `A_KEYLESS_HOST`.
#[cfg(all(test, not(target_family = "wasm")))]
mod wiring_tests {
    use std::path::PathBuf;

    use settings::Setting as _;
    use string_offset::ByteOffset;
    use warpui::App;

    use super::*;
    use crate::ai::AIRequestUsageModel;
    use crate::ai::agent_providers::AgentProviderSecrets;
    use crate::settings::{AISettings, AgentProvider, AgentProviderModel};
    use crate::test_util::settings::initialize_settings_for_tests;
    use crate::workspaces::user_workspaces::UserWorkspaces;

    /// A public `http://` host, so `agent_providers::embeddings` refuses the
    /// request before a socket is opened. Nothing is ever sent.
    const A_HOST: &str = "embeddings.example.invalid";

    /// The same, for the test whose *failing* path would still have a keyless
    /// endpoint and so would report the payload refusal rather than the
    /// credential one. `0.0.0.0` is neither loopback nor private, so both rules
    /// apply and a connection to it could in any case only ever be refused by
    /// this machine — a regression cannot turn into real traffic.
    const A_KEYLESS_HOST: &str = "0.0.0.0:1";

    fn init_indexing_test_app(app: &mut App) {
        // Everything `refresh_codebase_indexing_configuration` reads:
        // `AISettings` and `CodeSettings` from the settings bundle, the key
        // store, and — for the remote half — the usage limits, the consent
        // predicate's workspace half, and the manager the preferences are
        // pushed into.
        initialize_settings_for_tests(app);
        app.add_singleton_model(AgentProviderSecrets::new);
        app.add_singleton_model(UserWorkspaces::default_mock);
        app.add_singleton_model(AIRequestUsageModel::new);
        app.add_singleton_model(remote_server::manager::RemoteServerManager::new);
    }

    fn an_unconfigured_client() -> Arc<RefreshingStoreClient> {
        Arc::new(RefreshingStoreClient::new(
            Arc::new(HttpEmbeddingProvider::new(
                http_client::Client::new_for_test(),
                None,
            )),
            Arc::new(SqliteVectorStore::new(None, None)),
        ))
    }

    /// A provider that lists the default embedding model at `host`, and the id
    /// its API key is stored under.
    fn a_provider_serving_the_default_model(host: &str) -> (AgentProvider, String) {
        let mut provider = AgentProvider::new_empty();
        let id = provider.id.clone();
        provider.name = "Test Embeddings".to_owned();
        provider.base_url = format!("http://{host}/v1");
        provider.models = vec![AgentProviderModel::from_id(
            EmbeddingConfig::default().model_id().to_owned(),
        )];
        (provider, id)
    }

    async fn embed_error(client: &RefreshingStoreClient) -> IndexError {
        let content = "fn main() {}".to_owned();
        let content_hash = ContentHash::from_content(&content);
        let length = content.len();
        let fragment = Fragment::from_byte_range(
            content,
            content_hash,
            PathBuf::from("/repo/src/main.rs"),
            ByteOffset::from(0)..ByteOffset::from(length),
        );
        let root_hash = NodeHash::from(fragment.content_hash().clone());

        client
            .generate_embeddings(
                EmbeddingConfig::default(),
                vec![fragment],
                root_hash,
                RepoMetadata { path: None },
            )
            .await
            .expect_err("no reachable provider is configured in this test")
    }

    #[test]
    fn a_provider_added_after_startup_reaches_the_client_the_manager_holds() {
        App::test((), |mut app| async move {
            init_indexing_test_app(&mut app);

            let (provider, provider_id) = a_provider_serving_the_default_model(A_HOST);
            // Stored before the wiring is installed, so that the moment the
            // provider appears the endpoint already carries a key and the
            // plaintext-bearer guard stops the request. The refresh under test
            // is the one driven by `AISettings`.
            app.update(|ctx| {
                AgentProviderSecrets::handle(ctx).update(ctx, |secrets, ctx| {
                    secrets.set(&provider_id, "sk-added-after-launch".to_owned(), ctx);
                });
            });

            let store_client = an_unconfigured_client();
            app.update(|ctx| {
                subscribe_to_codebase_indexing_configuration(Arc::clone(&store_client), ctx);
            });

            assert!(
                matches!(
                    embed_error(&store_client).await,
                    IndexError::NoEmbeddingProvider { .. }
                ),
                "the initial refresh must find nothing, or this test proves nothing"
            );

            app.update(|ctx| {
                AISettings::handle(ctx).update(ctx, |settings, ctx| {
                    let _ = settings.agent_providers.set_value(vec![provider], ctx);
                });
            });

            let error = embed_error(&store_client).await.to_string();
            assert!(
                error.contains(A_HOST),
                "adding a provider must reach the client through the `AISettings` \
                 subscription, not wait for a restart; got {error}"
            );
        });
    }

    #[test]
    fn a_key_rotated_after_startup_reaches_the_client_the_manager_holds() {
        App::test((), |mut app| async move {
            init_indexing_test_app(&mut app);

            let (provider, provider_id) = a_provider_serving_the_default_model(A_KEYLESS_HOST);
            // The provider exists before the wiring, so the initial refresh
            // resolves its endpoint with an empty key. The only thing that
            // changes afterwards is the key, so `AgentProviderSecrets` is the
            // only subscription that can carry it.
            app.update(|ctx| {
                AISettings::handle(ctx).update(ctx, |settings, ctx| {
                    let _ = settings.agent_providers.set_value(vec![provider], ctx);
                });
            });

            let store_client = an_unconfigured_client();
            app.update(|ctx| {
                subscribe_to_codebase_indexing_configuration(Arc::clone(&store_client), ctx);
            });

            app.update(|ctx| {
                AgentProviderSecrets::handle(ctx).update(ctx, |secrets, ctx| {
                    secrets.set(&provider_id, "sk-rotated".to_owned(), ctx);
                });
            });

            // A key never appears in an error message, so the observable is the
            // guard it trips: refusing to put a bearer token on a plaintext
            // non-loopback connection. Without the `AgentProviderSecrets`
            // subscription the endpoint still has the empty key it was resolved
            // with, the credential rule does not apply, and the error is the
            // *payload* refusal — which names the code being protected and not
            // the key, so this assertion still fails against a broken
            // subscription.
            let error = embed_error(&store_client).await.to_string();
            assert!(
                error.contains("refusing to send the API key"),
                "a rotated key must reach the client through the `AgentProviderSecrets` \
                 subscription; got {error}"
            );
        });
    }
}

#[cfg(test)]
#[path = "codebase_embeddings_tests.rs"]
mod tests;
