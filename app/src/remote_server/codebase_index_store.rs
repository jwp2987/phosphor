//! The remote-server daemon's half of the codebase index's BYOP `StoreClient`.
//!
//! # Why this exists at all
//!
//! At the pin the daemon indexed a repository on its host and pushed the
//! resulting merkle nodes and embedding vectors to Warp's *shared* store, using
//! a bearer token the client handed it. The client then queried that same
//! shared store and asked the daemon only to map the resulting content hashes
//! back to files (`GetFragmentMetadataFromHash`). "Shared" is what made that
//! split work.
//!
//! This fork has no shared store. `app::ai::codebase_embeddings` keeps vectors
//! in the app's own SQLite database, which a daemon on another machine cannot
//! reach — and the daemon has none of the app's persistence anyway: no
//! `PersistenceWriter`, no `AISettings`, no keychain. So the daemon gets:
//!
//! * its own SQLite file under its data dir ([`DaemonVectorStore`]), and
//! * the user's embedding endpoint, sent by the client on the `Initialize`
//!   handshake and refreshed by `UpdatePreferences`
//!   ([`DaemonStoreClient::configure`]).
//!
//! **The consequence, stated rather than buried:** the daemon's vectors are not
//! visible to the client's own store, so a repository indexed on a remote host
//! is searched *there*, by the daemon, not locally against a shared index. That
//! is a different topology from the pin's, and it is the only one available
//! without a shared store. Everything the client observes — statuses,
//! progress, fragment metadata — keeps the pin's shape.
//!
//! # Why a process-global provider handle
//!
//! `run_daemon_app` builds the store client before `ServerModel` exists, and
//! `ServerModel` is the thing that later learns the endpoint from the client.
//! The obvious way to bridge that is another singleton model — which is exactly
//! the shape that has already produced "Cannot get singleton model ... never
//! registered" panics in this daemon (see the registration-order comments in
//! `mod.rs`). A `OnceLock` in a single-purpose process has no registration
//! order to get wrong.

use std::collections::HashSet;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use ai::index::full_source_code_embedding::local_store_client::{
    EmbeddingProvider, LocalCodebaseContextConfig, LocalStoreClient, VectorStore,
};
use ai::index::full_source_code_embedding::store_client::{IntermediateNode, StoreClient};
use ai::index::full_source_code_embedding::vector_index::NodeSummary;
use ai::index::full_source_code_embedding::{
    CodebaseContextConfig, ContentHash, EmbeddingConfig, Error as IndexError, Fragment, NodeHash,
    RepoMetadata,
};
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use diesel::SqliteConnection;
use http_client::Client;
use std::collections::HashMap;

use crate::ai::agent_providers::embeddings::{EmbeddingEndpoint, HttpEmbeddingProvider};
use crate::persistence::{
    codebase_index_children, codebase_index_node_summaries, codebase_index_vectors,
    establish_codebase_index_connection, known_codebase_index_hashes,
    save_codebase_index_embeddings, save_codebase_index_node_summaries, save_codebase_index_nodes,
};

/// The daemon's store client, once `run_daemon_app` has built it.
///
/// `None` on every other launch mode, and on a daemon built without
/// `local_fs`. `ServerModel` reads it to push the client's embedding endpoint
/// down; nothing else touches it.
static DAEMON_STORE_CLIENT: OnceLock<Arc<DaemonStoreClient>> = OnceLock::new();

/// Records the daemon's store client so `ServerModel` can configure it when a
/// client connects. Ignored if called twice — the first daemon wins, and there
/// is only ever one.
pub fn set_daemon_store_client(client: Arc<DaemonStoreClient>) {
    let _ = DAEMON_STORE_CLIENT.set(client);
}

/// The daemon's store client, if this process is a daemon that built one.
pub fn daemon_store_client() -> Option<Arc<DaemonStoreClient>> {
    DAEMON_STORE_CLIENT.get().cloned()
}

/// Encodes a vector as little-endian `f32` bytes. Must stay byte-identical to
/// `app::ai::codebase_embeddings::encode_vector` — both write the same table
/// shape, and a daemon's database may be inspected with the same tooling.
fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

/// Decodes what [`encode_vector`] wrote, checking it against the recorded
/// width. A blob whose length disagrees with `dimensions` is reported rather
/// than truncated: a silently shortened vector would still score, badly and
/// unfalsifiably.
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

/// A [`VectorStore`] over a standalone SQLite file in the daemon's data dir.
///
/// One connection serves both reads and writes. The app splits those across a
/// writer channel and a read-only connection because it has many writers; the
/// daemon has exactly one, so the split would buy nothing and would cost the
/// app's write-then-read visibility gap.
pub struct DaemonVectorStore {
    /// `None` when the database could not be opened. Every method then degrades
    /// to "store nothing, know nothing", which makes the index re-sync rather
    /// than answer from a store it cannot read.
    conn: Option<Mutex<SqliteConnection>>,
}

impl DaemonVectorStore {
    /// Opens (or creates) the store at `path`.
    ///
    /// Never fails: a database that cannot be opened is logged and degrades as
    /// described on [`Self::conn`]. Failing here would take down the whole
    /// daemon over a feature the user may not be using.
    pub fn open(path: &Path) -> Self {
        let conn = path
            .to_str()
            .ok_or_else(|| anyhow!("codebase index database path is not valid UTF-8"))
            .and_then(|url| establish_codebase_index_connection(url).map_err(Into::into))
            .inspect_err(|error| {
                log::warn!("Daemon codebase index store is unavailable: {error:#}");
            })
            .ok()
            .map(Mutex::new);
        Self { conn }
    }

    fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut SqliteConnection) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        let Some(conn) = &self.conn else {
            return Err(anyhow!("the daemon has no codebase index database"));
        };
        let mut conn = conn
            .lock()
            .map_err(|_| anyhow!("the daemon's codebase index connection is poisoned"))?;
        f(&mut conn)
    }
}

impl VectorStore for DaemonVectorStore {
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

        self.with_conn(|conn| {
            save_codebase_index_nodes(conn, space.to_owned(), rows)
                .context("failed to record codebase index nodes")
        })
    }

    fn known_hashes(&self, space: &str, hashes: &[NodeHash]) -> anyhow::Result<HashSet<NodeHash>> {
        if hashes.is_empty() {
            return Ok(HashSet::new());
        }
        let as_strings: Vec<String> = hashes.iter().map(ToString::to_string).collect();
        let known = self.with_conn(|conn| {
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

        self.with_conn(|conn| {
            save_codebase_index_embeddings(conn, space.to_owned(), rows)
                .context("failed to record codebase index embeddings")
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
        let rows = self.with_conn(|conn| {
            codebase_index_children(conn, space, &as_strings)
                .context("failed to read codebase index nodes")
        })?;

        let mut out = HashMap::with_capacity(rows.len());
        for (hash, json) in rows {
            let Ok(node_hash) = hash.parse::<NodeHash>() else {
                log::warn!("Skipping unparseable node hash in the daemon codebase index: {hash}");
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

        self.with_conn(|conn| {
            save_codebase_index_node_summaries(conn, space.to_owned(), rows)
                .context("failed to record codebase index node summaries")
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
        let rows = self.with_conn(|conn| {
            codebase_index_node_summaries(conn, space, &as_strings)
                .context("failed to read codebase index node summaries")
        })?;

        let mut out = Vec::with_capacity(rows.len());
        for (hash, leaf_count, radius, dimensions, bytes) in rows {
            let Ok(node_hash) = hash.parse::<NodeHash>() else {
                log::warn!("Skipping unparseable node hash in the daemon codebase index: {hash}");
                continue;
            };
            match decode_vector(&bytes, dimensions) {
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
        let rows = self.with_conn(|conn| {
            codebase_index_vectors(conn, space, &as_strings)
                .context("failed to read codebase index vectors")
        })?;

        let mut out = Vec::with_capacity(rows.len());
        for (hash, dimensions, bytes) in rows {
            let Ok(content_hash) = hash.parse::<ContentHash>() else {
                log::warn!("Skipping unparseable content hash in the daemon vector store: {hash}");
                continue;
            };
            match decode_vector(&bytes, dimensions) {
                Err(error) => log::warn!("Skipping corrupt stored vector for {hash}: {error:#}"),
                Ok(vector) => out.push((content_hash, vector)),
            }
        }
        Ok(out)
    }
}

/// The daemon's [`StoreClient`]: a [`DaemonVectorStore`] plus an
/// [`HttpEmbeddingProvider`] whose endpoint arrives from the client after the
/// process has already started.
///
/// Delegates every call to a freshly-assembled [`LocalStoreClient`] — three
/// `Arc` clones and a `Copy` config — so that reconfiguring the model at
/// runtime does not require rebuilding the `CodebaseIndexManager`, which holds
/// one `Arc<dyn StoreClient>` for the life of the process.
pub struct DaemonStoreClient {
    provider: Arc<HttpEmbeddingProvider>,
    store: Arc<DaemonVectorStore>,
    /// The model the client told us to use. `None` until an `Initialize` (or
    /// `UpdatePreferences`) carrying an `EmbeddingProviderConfig` arrives.
    ///
    /// Deliberately not defaulted: defaulting would let the daemon embed a
    /// whole repository against a model the user never configured, and the
    /// vectors would be keyed under that model's storage key. `None` surfaces
    /// as `Error::NoEmbeddingProvider`, which the caller reports as
    /// `Unavailable`.
    config: Mutex<Option<EmbeddingConfig>>,
}

impl DaemonStoreClient {
    pub fn new(provider: Arc<HttpEmbeddingProvider>, store: Arc<DaemonVectorStore>) -> Self {
        Self {
            provider,
            store,
            config: Mutex::new(None),
        }
    }

    /// Points the daemon at the user's embedding endpoint.
    ///
    /// `None` clears it, which is what a client with no configured provider
    /// sends; the daemon then reports indexing as unavailable rather than
    /// producing vectors nobody can compare against.
    pub fn configure(&self, endpoint: Option<EmbeddingEndpoint>, config: Option<EmbeddingConfig>) {
        self.provider.set_endpoint(endpoint);
        if let Ok(mut slot) = self.config.lock() {
            *slot = config;
        }
    }

    /// Whether the daemon currently has an embedding model to index with.
    pub fn is_configured(&self) -> bool {
        self.config
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false)
    }

    /// The model the daemon was told to use, if any.
    pub fn configured_model(&self) -> Option<EmbeddingConfig> {
        self.config.lock().ok().and_then(|slot| *slot)
    }

    fn client(&self) -> Result<LocalStoreClient, IndexError> {
        let embedding_config = self
            .configured_model()
            .ok_or(IndexError::NoEmbeddingProvider {
                model: "<none configured by the client>",
            })?;
        Ok(LocalStoreClient::new(
            Arc::clone(&self.provider) as Arc<dyn EmbeddingProvider>,
            Arc::clone(&self.store) as Arc<dyn VectorStore>,
            LocalCodebaseContextConfig {
                embedding_config,
                ..LocalCodebaseContextConfig::default()
            },
        ))
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl StoreClient for DaemonStoreClient {
    async fn update_intermediate_nodes(
        &self,
        embedding_config: EmbeddingConfig,
        nodes: Vec<IntermediateNode>,
    ) -> Result<HashMap<NodeHash, bool>, IndexError> {
        let client = self.client()?;
        client
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
        let client = self.client()?;
        client
            .generate_embeddings(embedding_config, fragments, root_hash, repo_metadata)
            .await
    }

    async fn populate_merkle_tree_cache(
        &self,
        embedding_config: EmbeddingConfig,
        root_hash: NodeHash,
        repo_metadata: RepoMetadata,
    ) -> Result<bool, IndexError> {
        let client = self.client()?;
        client
            .populate_merkle_tree_cache(embedding_config, root_hash, repo_metadata)
            .await
    }

    async fn sync_merkle_tree(
        &self,
        nodes: Vec<NodeHash>,
        embedding_config: EmbeddingConfig,
    ) -> Result<HashSet<NodeHash>, IndexError> {
        let client = self.client()?;
        client.sync_merkle_tree(nodes, embedding_config).await
    }

    async fn rerank_fragments(
        &self,
        query: String,
        fragments: Vec<Fragment>,
    ) -> Result<Vec<Fragment>, IndexError> {
        let client = self.client()?;
        client.rerank_fragments(query, fragments).await
    }

    async fn get_relevant_fragments(
        &self,
        embedding_config: EmbeddingConfig,
        query: String,
        root_hash: NodeHash,
        repo_metadata: RepoMetadata,
    ) -> Result<Vec<ContentHash>, IndexError> {
        let client = self.client()?;
        client
            .get_relevant_fragments(embedding_config, query, root_hash, repo_metadata)
            .await
    }

    async fn codebase_context_config(&self) -> Result<CodebaseContextConfig, IndexError> {
        let client = self.client()?;
        client.codebase_context_config().await
    }
}

/// Assembles the daemon's store client and registers it for `ServerModel` to
/// configure later.
pub fn build_daemon_store_client(data_dir: &Path) -> Arc<DaemonStoreClient> {
    let db_path = data_dir.join("cache").join("codebase_index.sqlite");
    if let Some(parent) = db_path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        log::warn!(
            "Could not create the daemon codebase index directory {}: {error:#}",
            parent.display()
        );
    }

    // No endpoint yet: it arrives on the first client's `Initialize`.
    let provider = Arc::new(HttpEmbeddingProvider::new(Client::new(), None));
    let store = Arc::new(DaemonVectorStore::open(&db_path));
    let client = Arc::new(DaemonStoreClient::new(provider, store));
    set_daemon_store_client(Arc::clone(&client));
    client
}
