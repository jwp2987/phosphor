//! A [`StoreClient`] that keeps everything on this machine.
//!
//! # Why this exists
//!
//! At the pin, [`StoreClient`] had exactly one non-mock implementation —
//! `impl StoreClient for ServerApi` (`app/src/server/server_api/ai.rs:3332`) —
//! and every one of its seven methods was a GraphQL round-trip. The server held
//! the merkle-tree registry, produced the embeddings, stored the vectors, ran
//! the nearest-neighbour search and ran a cross-encoder reranker.
//!
//! This fork has no server. `StoreClient` is a single-implementation trait, so
//! it is precisely the seam BYOP already exists to replace, the way it was
//! replaced for chat providers. This type is that replacement:
//!
//! * **Embeddings** go to the user's own provider over HTTP
//!   ([`EmbeddingProvider`], implemented in `app/src/ai/agent_providers/embeddings.rs`).
//! * **Storage** is local ([`VectorStore`], implemented over the app's SQLite
//!   database).
//! * **Retrieval and reranking** are computed here, by cosine similarity.
//!
//! # Where this is worse than the pin, stated plainly
//!
//! 1. **Reranking quality.** The pin's `rerank_fragments` called a dedicated
//!    reranking model — a cross-encoder that reads the query and the fragment
//!    *together* and scores their interaction. [`LocalStoreClient`] reranks by
//!    cosine similarity between the query embedding and each fragment's
//!    embedding, which is a bi-encoder: query and fragment are encoded
//!    independently and never see each other. Bi-encoder ordering is
//!    consistently worse than cross-encoder ordering on the top of the list,
//!    which is exactly where a reranker earns its keep. Expect the first few
//!    results to be less well ordered than the pin's. The recall (which
//!    fragments make the shortlist at all) is unaffected — that was already a
//!    vector search at the pin.
//! 2. **Rerank costs a request.** The pin reranked with the fragments it was
//!    handed and no further embedding work. Here, a fragment that is not already
//!    in the store has to be embedded before it can be scored, and the query
//!    always costs one embedding call. Fragments already embedded during
//!    indexing are read from the store, so the common path is one query
//!    embedding per search.
//! 3. **Search is exact, not approximate.** The pin's server almost certainly
//!    used an ANN index. This walks the repo's leaves and scores every one. That
//!    is *more* accurate, but it is O(fragments) per query, so latency grows
//!    linearly with repo size where the pin's was roughly flat.
//! 4. **No cross-repo or shared cache.** `populate_merkle_tree_cache` warmed a
//!    server-side cache that other clients (and other machines) could then hit.
//!    There is no such thing locally; see the method for what it does instead.
//! 5. **`repo_metadata` is not used for scoping.** The pin passed the repo path
//!    to the server so it could scope a query to one repo. Scoping here is done
//!    structurally, by walking the merkle tree down from `root_hash`, which is
//!    strictly more precise: it cannot return a fragment that is not reachable
//!    from the root the caller asked about.
//!
//! # What this deliberately does not do
//!
//! Nothing here degrades to a silent empty result. If no provider is configured,
//! every method that needs one fails with
//! [`Error::NoEmbeddingProvider`][super::Error::NoEmbeddingProvider], which
//! names the model and says where to configure it. An unconfigured index that
//! *says so* is worth more than one that quietly answers "no matches".

use std::collections::{HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::store_client::{IntermediateNode, StoreClient};
use super::{
    CodebaseContextConfig, ContentHash, EmbeddingConfig, Error, Fragment, NodeHash, RepoMetadata,
};

/// How many candidate fragments a query returns by default.
///
/// At the pin this cap lived on the server and arrived with the query results.
/// The value is chosen to match the pin's observed behaviour: enough candidates
/// that the reranker has something to work with, few enough that reading and
/// reranking them stays interactive.
pub const DEFAULT_MAX_RELEVANT_FRAGMENTS: usize = 50;

/// How often a synced index re-embeds, when no other cadence is configured.
///
/// The pin fetched this from `codebaseContextConfig` alongside the embedding
/// model. Five minutes is the value the pin's own `MockStoreClient` used, and it
/// is the one behaviour of the mock worth keeping: it is short enough that edits
/// show up in search within a coffee break and long enough that a busy repo does
/// not spend the user's embedding quota on churn.
pub const DEFAULT_EMBEDDING_CADENCE: Duration = Duration::from_secs(300);

/// Generates embedding vectors for text, using whatever provider the user has
/// configured.
///
/// Implemented in `app/` rather than here, because the provider list, the API
/// keys and the HTTP client all live there — the same layering
/// `AgentProviderSecrets` and `chat_stream` already use.
#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait EmbeddingProvider: 'static + Send + Sync {
    /// Embeds `texts`, returning one vector per input, in the same order.
    ///
    /// Implementations must return exactly `texts.len()` vectors or an error;
    /// a short return is a protocol violation, not a partial success, because
    /// the caller pairs results with inputs positionally.
    async fn embed(
        &self,
        embedding_config: EmbeddingConfig,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, Error>;
}

/// Local, durable storage for the merkle-node registry and the embedding
/// vectors.
///
/// Deliberately blocking rather than `async`: the app's SQLite layer is
/// synchronous (one writer thread, short-lived read-only connections), and every
/// call into this trait already happens inside a background sync-queue task or a
/// spawned future, never on the UI thread.
///
/// `space` is [`EmbeddingConfig::storage_key`], and every method is scoped by it
/// so that vectors produced by different models can never be compared with each
/// other. Changing the model does not corrupt the store; it just leaves the old
/// rows unreachable.
pub trait VectorStore: 'static + Send + Sync {
    /// Records intermediate nodes and their child lists, replacing any existing
    /// entry for the same hash.
    fn record_nodes(&self, space: &str, nodes: &[IntermediateNode]) -> anyhow::Result<()>;

    /// Returns the subset of `hashes` this store already knows about — either as
    /// an intermediate node or as an embedded leaf.
    ///
    /// This is the local answer to "what does the server already have", which is
    /// the whole of `sync_merkle_tree`.
    fn known_hashes(&self, space: &str, hashes: &[NodeHash]) -> anyhow::Result<HashSet<NodeHash>>;

    /// Stores one vector per content hash, replacing any existing vector.
    fn record_embeddings(
        &self,
        space: &str,
        embeddings: &[(ContentHash, Vec<f32>)],
    ) -> anyhow::Result<()>;

    /// Every embedded leaf reachable from `root`, by walking the recorded child
    /// lists.
    ///
    /// This is what scopes a query to one repository: a fragment that is not
    /// reachable from the root the caller asked about cannot be returned.
    fn leaves_under(&self, space: &str, root: &NodeHash) -> anyhow::Result<Vec<ContentHash>>;

    /// The stored vectors for `hashes`. Hashes with no stored vector are
    /// omitted rather than reported — the caller treats "no vector" as "cannot
    /// be scored", which is a normal state during a sync.
    fn vectors_for(
        &self,
        space: &str,
        hashes: &[ContentHash],
    ) -> anyhow::Result<Vec<(ContentHash, Vec<f32>)>>;
}

/// Cosine similarity of two vectors, in `[-1.0, 1.0]`.
///
/// Returns `0.0` for mismatched lengths or a zero-magnitude vector rather than
/// `NaN`, so a malformed row can never poison a sort.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f64;
    let mut norm_a = 0.0f64;
    let mut norm_b = 0.0f64;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += f64::from(*x) * f64::from(*y);
        norm_a += f64::from(*x) * f64::from(*x);
        norm_b += f64::from(*y) * f64::from(*y);
    }

    if norm_a <= 0.0 || norm_b <= 0.0 {
        return 0.0;
    }

    (dot / (norm_a.sqrt() * norm_b.sqrt())) as f32
}

/// Sorts `scored` by descending score, breaking ties deterministically.
///
/// `sort_by` with a partial comparison would be non-deterministic on equal
/// scores, which makes search results flap between identical queries. The tie
/// break is on the caller-supplied key so the order is stable across runs.
fn sort_by_score_desc<T>(scored: &mut [(T, f32)], key: impl Fn(&T) -> String) {
    scored.sort_by(|(left_item, left), (right_item, right)| {
        right
            .partial_cmp(left)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| key(left_item).cmp(&key(right_item)))
    });
}

/// The knobs the pin fetched from the server, derived locally instead.
#[derive(Clone, Copy, Debug)]
pub struct LocalCodebaseContextConfig {
    pub embedding_config: EmbeddingConfig,
    pub embedding_cadence: Duration,
    pub max_relevant_fragments: usize,
}

impl Default for LocalCodebaseContextConfig {
    fn default() -> Self {
        Self {
            embedding_config: EmbeddingConfig::default(),
            embedding_cadence: DEFAULT_EMBEDDING_CADENCE,
            max_relevant_fragments: DEFAULT_MAX_RELEVANT_FRAGMENTS,
        }
    }
}

/// A [`StoreClient`] backed by a user-configured embedding provider and a local
/// vector store. See the module docs for how it differs from the pin.
pub struct LocalStoreClient {
    provider: Arc<dyn EmbeddingProvider>,
    store: Arc<dyn VectorStore>,
    config: LocalCodebaseContextConfig,
}

impl LocalStoreClient {
    pub fn new(
        provider: Arc<dyn EmbeddingProvider>,
        store: Arc<dyn VectorStore>,
        config: LocalCodebaseContextConfig,
    ) -> Self {
        Self {
            provider,
            store,
            config,
        }
    }

    /// Embeds a single string and returns its vector.
    async fn embed_one(
        &self,
        embedding_config: EmbeddingConfig,
        text: String,
    ) -> Result<Vec<f32>, Error> {
        let mut vectors = self.provider.embed(embedding_config, vec![text]).await?;
        if vectors.len() != 1 {
            return Err(Error::Other(anyhow::anyhow!(
                "embedding provider returned {} vectors for a single input",
                vectors.len()
            )));
        }
        Ok(vectors.remove(0))
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl StoreClient for LocalStoreClient {
    /// Records the child lists of intermediate nodes.
    ///
    /// The pin sent these to the server so it could later walk a tree it did not
    /// have the filesystem for. The local store needs them for exactly the same
    /// reason: `get_relevant_fragments` is scoped by walking down from a root
    /// hash, and without the child lists there is no tree to walk.
    ///
    /// Every node either records successfully or the whole batch fails, so the
    /// returned map is uniformly `true`. Per-node failure was a property of the
    /// remote API (one node could be rejected while its siblings were accepted);
    /// a local transaction has no such state.
    async fn update_intermediate_nodes(
        &self,
        embedding_config: EmbeddingConfig,
        nodes: Vec<IntermediateNode>,
    ) -> Result<HashMap<NodeHash, bool>, Error> {
        let space = embedding_config.storage_key();
        self.store
            .record_nodes(space, &nodes)
            .map_err(Error::VectorStore)?;

        Ok(nodes.into_iter().map(|node| (node.hash, true)).collect())
    }

    /// Embeds each fragment with the user's provider and stores the vectors.
    ///
    /// Unlike the pin, a fragment cannot fail on its own: the provider is asked
    /// for the whole batch, so either all of these vectors come back or none do.
    /// The pin's per-fragment status map is still honoured — it is filled with
    /// `true` on success — and the caller's failure path (`FailedToGenerateEmbeddings`)
    /// is reached by returning `Err`, which it already handles.
    ///
    /// `root_hash` and `repo_metadata` are accepted and unused: at the pin they
    /// told the server which repository the fragments belonged to, and the local
    /// store derives that structurally from the merkle tree instead.
    async fn generate_embeddings(
        &self,
        embedding_config: EmbeddingConfig,
        fragments: Vec<Fragment>,
        _root_hash: NodeHash,
        _repo_metadata: RepoMetadata,
    ) -> Result<HashMap<ContentHash, bool>, Error> {
        if fragments.is_empty() {
            return Ok(HashMap::new());
        }

        let texts: Vec<String> = fragments
            .iter()
            .map(|fragment| fragment.content().to_owned())
            .collect();

        let vectors = self.provider.embed(embedding_config, texts).await?;
        if vectors.len() != fragments.len() {
            return Err(Error::Other(anyhow::anyhow!(
                "embedding provider returned {} vectors for {} fragments",
                vectors.len(),
                fragments.len()
            )));
        }

        let rows: Vec<(ContentHash, Vec<f32>)> = fragments
            .iter()
            .map(|fragment| fragment.content_hash().clone())
            .zip(vectors)
            .collect();

        self.store
            .record_embeddings(embedding_config.storage_key(), &rows)
            .map_err(Error::VectorStore)?;

        Ok(rows.into_iter().map(|(hash, _)| (hash, true)).collect())
    }

    /// A no-op that reports success.
    ///
    /// This is the one method with genuinely nothing to do locally, and that is
    /// a property of the design rather than an omission. At the pin it asked the
    /// server to warm a cache keyed by root hash so that the *next* client to
    /// ask about this tree — possibly on another machine — would not pay for the
    /// walk. Here the store and the querier are the same process reading the
    /// same tables: writing the nodes (`update_intermediate_nodes`) has already
    /// done everything a warm cache would have done, so there is no work left
    /// and no behaviour lost.
    ///
    /// It is not stubbed into silence: `Ok(true)` is the truthful answer, the
    /// caller treats this result as advisory (`cache_population_error` is
    /// attached to telemetry, never to correctness), and returning an error here
    /// would report a failure that did not happen.
    async fn populate_merkle_tree_cache(
        &self,
        _embedding_config: EmbeddingConfig,
        _root_hash: NodeHash,
        _repo_metadata: RepoMetadata,
    ) -> Result<bool, Error> {
        Ok(true)
    }

    /// Returns the subset of `nodes` the local store has never seen.
    ///
    /// The caller uses this to walk the tree top-down: a node it gets back is
    /// dirty, so its children are checked next; a node it does not get back is
    /// clean, and the whole subtree beneath it is skipped. That contract is
    /// unchanged from the pin — only "does the server have it" becomes "does the
    /// local store have it".
    async fn sync_merkle_tree(
        &self,
        nodes: Vec<NodeHash>,
        embedding_config: EmbeddingConfig,
    ) -> Result<HashSet<NodeHash>, Error> {
        if nodes.is_empty() {
            return Ok(HashSet::new());
        }

        let known = self
            .store
            .known_hashes(embedding_config.storage_key(), &nodes)
            .map_err(Error::VectorStore)?;

        Ok(nodes
            .into_iter()
            .filter(|hash| !known.contains(hash))
            .collect())
    }

    /// Reorders `fragments` by cosine similarity to `query`.
    ///
    /// This is the biggest quality difference from the pin, which used a
    /// cross-encoder reranking model here. See the module docs.
    ///
    /// Fragments already embedded during indexing are scored from the store.
    /// Any fragment with no stored vector is embedded on the spot, so a rerank
    /// never silently drops a fragment for lack of a vector.
    async fn rerank_fragments(
        &self,
        query: String,
        fragments: Vec<Fragment>,
    ) -> Result<Vec<Fragment>, Error> {
        if fragments.len() < 2 {
            return Ok(fragments);
        }

        let embedding_config = self.config.embedding_config;
        let space = embedding_config.storage_key();

        let hashes: Vec<ContentHash> = fragments
            .iter()
            .map(|fragment| fragment.content_hash().clone())
            .collect();

        let mut vectors: HashMap<ContentHash, Vec<f32>> = self
            .store
            .vectors_for(space, &hashes)
            .map_err(Error::VectorStore)?
            .into_iter()
            .collect();

        // Anything the store does not have yet gets embedded now, in one batch.
        let missing: Vec<usize> = (0..fragments.len())
            .filter(|index| !vectors.contains_key(fragments[*index].content_hash()))
            .collect();
        if !missing.is_empty() {
            let texts: Vec<String> = missing
                .iter()
                .map(|index| fragments[*index].content().to_owned())
                .collect();
            let fresh = self.provider.embed(embedding_config, texts).await?;
            if fresh.len() != missing.len() {
                return Err(Error::Other(anyhow::anyhow!(
                    "embedding provider returned {} vectors for {} fragments",
                    fresh.len(),
                    missing.len()
                )));
            }
            let rows: Vec<(ContentHash, Vec<f32>)> = missing
                .iter()
                .map(|index| fragments[*index].content_hash().clone())
                .zip(fresh)
                .collect();
            // Best effort: a rerank that cannot write its cache is still a valid
            // rerank, so a store failure here is logged rather than fatal.
            if let Err(error) = self.store.record_embeddings(space, &rows) {
                log::warn!("Failed to cache rerank embeddings: {error:#}");
            }
            vectors.extend(rows);
        }

        let query_vector = self.embed_one(embedding_config, query).await?;

        let mut scored: Vec<(Fragment, f32)> = fragments
            .into_iter()
            .map(|fragment| {
                let score = vectors
                    .get(fragment.content_hash())
                    .map(|vector| cosine_similarity(&query_vector, vector))
                    .unwrap_or(0.0);
                (fragment, score)
            })
            .collect();

        sort_by_score_desc(&mut scored, |fragment| fragment.content_hash().to_string());

        Ok(scored.into_iter().map(|(fragment, _)| fragment).collect())
    }

    /// Finds the most similar fragments in the tree rooted at `root_hash`.
    ///
    /// Scoping is structural rather than by `repo_metadata`: the store walks the
    /// recorded child lists down from `root_hash`, so a fragment that is not
    /// reachable from that root cannot be returned. This is why
    /// `update_intermediate_nodes` has to persist the child lists.
    ///
    /// The scan is exact, not approximate — every reachable leaf with a stored
    /// vector is scored. See the module docs for the latency consequence.
    async fn get_relevant_fragments(
        &self,
        embedding_config: EmbeddingConfig,
        query: String,
        root_hash: NodeHash,
        _repo_metadata: RepoMetadata,
    ) -> Result<Vec<ContentHash>, Error> {
        let space = embedding_config.storage_key();

        let leaves = self
            .store
            .leaves_under(space, &root_hash)
            .map_err(Error::VectorStore)?;
        if leaves.is_empty() {
            return Ok(Vec::new());
        }

        let query_vector = self.embed_one(embedding_config, query).await?;

        let mut scored: Vec<(ContentHash, f32)> = self
            .store
            .vectors_for(space, &leaves)
            .map_err(Error::VectorStore)?
            .into_iter()
            .map(|(hash, vector)| {
                let score = cosine_similarity(&query_vector, &vector);
                (hash, score)
            })
            .collect();

        sort_by_score_desc(&mut scored, |hash| hash.to_string());
        scored.truncate(self.config.max_relevant_fragments);

        Ok(scored.into_iter().map(|(hash, _)| hash).collect())
    }

    /// The locally derived equivalent of the pin's `codebaseContextConfig`
    /// query. See [`LocalCodebaseContextConfig`].
    async fn codebase_context_config(&self) -> Result<CodebaseContextConfig, Error> {
        Ok(CodebaseContextConfig {
            embedding_config: self.config.embedding_config,
            embedding_cadence: self.config.embedding_cadence,
        })
    }
}

/// Converts a merkle node hash to a content hash.
///
/// The two types wrap the same `MerkleHash` and the tree stores a leaf's content
/// hash in its parent's child list as a [`NodeHash`], so walking down from a
/// root produces `NodeHash` values that are really content hashes. The pin never
/// needed this conversion because the walk happened on the server; a local
/// implementation of [`VectorStore::leaves_under`] does.
pub fn node_hash_to_content_hash(hash: &NodeHash) -> Result<ContentHash, Error> {
    ContentHash::from_str(&hash.to_string())
}

#[cfg(test)]
#[path = "local_store_client_tests.rs"]
mod tests;
