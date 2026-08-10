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
    LocalCodebaseContextConfig, LocalStoreClient, RerankProvider, VectorStore,
};
use ai::index::full_source_code_embedding::store_client::{IntermediateNode, StoreClient};
use ai::index::full_source_code_embedding::vector_index::NodeSummary;
use ai::index::full_source_code_embedding::{ContentHash, EmbeddingConfig, NodeHash};
use anyhow::{Context, anyhow};
use diesel::SqliteConnection;
use warpui::{AppContext, SingletonEntity};

use crate::ai::agent_providers::embeddings::{
    HttpEmbeddingProvider, HttpRerankProvider, resolve_configured_embedding_model,
};
use crate::persistence::{
    ModelEvent, PersistenceWriter, codebase_index_children, codebase_index_node_summaries,
    codebase_index_vectors, database_file_path, establish_ro_connection,
    known_codebase_index_hashes,
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

    /// Builds one from the app's persistence singleton.
    pub fn from_app(app: &AppContext) -> Self {
        let writer = PersistenceWriter::as_ref(app).sender();
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

/// Builds the codebase index's `StoreClient` from the user's configuration.
///
/// Returns a client whichever way the configuration falls: when no provider is
/// configured, the client still works for everything that does not need one
/// (recording nodes, answering "what do you already have"), and the paths that
/// do need one fail with `Error::NoEmbeddingProvider`. That is deliberate — a
/// `None` here would have to be handled by every caller, and the natural
/// handling would be to skip indexing silently.
pub fn build_store_client(app: &AppContext) -> Arc<dyn StoreClient> {
    // Whatever the user actually configured, falling back to the pin's default
    // so the index has a well-defined storage space even before setup.
    let embedding_config = resolve_configured_embedding_model(app).unwrap_or_default();

    if resolve_configured_embedding_model(app).is_none() {
        log::info!(
            "No embedding provider is configured; codebase indexing will report \
             NoEmbeddingProvider until one is added under Settings > AI"
        );
    }

    let provider = Arc::new(HttpEmbeddingProvider::from_app(app, embedding_config));
    let store = Arc::new(SqliteVectorStore::from_app(app));

    // The pin reranked with a server-side cross-encoder. Where the user's own
    // provider sells one, use it; where it does not, `LocalStoreClient` falls
    // back to hybrid vector + lexical scoring rather than requiring a model
    // nobody configured.
    let reranker = HttpRerankProvider::from_app(app).map(|reranker| {
        log::info!("Codebase index reranking will use {}", reranker.model_id());
        Arc::new(reranker) as Arc<dyn RerankProvider>
    });
    if reranker.is_none() {
        log::info!(
            "No reranking model is configured; codebase search will rerank with \
             hybrid vector + lexical scoring. Adding a rerank model (e.g. \
             rerank-2.5) to a provider under Settings > AI enables cross-encoder \
             reranking instead."
        );
    }

    Arc::new(
        LocalStoreClient::new(
            provider,
            store,
            LocalCodebaseContextConfig {
                embedding_config,
                ..LocalCodebaseContextConfig::default()
            },
        )
        .with_rerank_provider(reranker),
    )
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
/// queries.
#[cfg(not(target_family = "wasm"))]
pub fn remote_client_preferences(app: &AppContext) -> remote_server::client::ClientPreferences {
    use crate::ai::agent_providers::embeddings::resolve_embedding_endpoint;
    use crate::ai::AIRequestUsageModel;

    let limits = AIRequestUsageModel::as_ref(app).codebase_context_limits();
    let codebase_index_limits = Some(remote_server::proto::CodebaseIndexLimits {
        max_indices_allowed: limits.max_indices_allowed.map(|value| value as u64),
        max_files_per_repo: limits.max_files_per_repo as u64,
        embedding_generation_batch_size: limits.embedding_generation_batch_size as u64,
    });

    let embedding_provider = resolve_configured_embedding_model(app).and_then(|config| {
        resolve_embedding_endpoint(app, config).map(|endpoint| {
            remote_server::proto::EmbeddingProviderConfig {
                base_url: endpoint.base_url,
                api_key: endpoint.api_key,
                embedding_storage_key: config.storage_key().to_string(),
            }
        })
    });

    remote_server::client::ClientPreferences {
        codebase_index_limits,
        embedding_provider,
    }
}

#[cfg(test)]
#[path = "codebase_embeddings_tests.rs"]
mod tests;
