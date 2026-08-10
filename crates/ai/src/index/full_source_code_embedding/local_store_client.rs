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
//! * **Retrieval** descends the merkle tree with a pruning bound
//!   ([`vector_index`][super::vector_index]), so a query does not have to look
//!   at the whole repository.
//! * **Reranking** uses the user's provider's reranking model where there is
//!   one, and a hybrid of vector and lexical scoring where there is not.
//!
//! # How this differs from the pin, stated plainly
//!
//! 1. **Reranking is the user's provider's, or hybrid.** The pin called a
//!    dedicated cross-encoder — a model that reads query and fragment *together*
//!    and scores their interaction. A provider that offers one is used the same
//!    way ([`RerankProvider`]). A provider that does not leaves a bi-encoder,
//!    which encodes query and fragment separately and never lets them see each
//!    other; that is measurably worse at the top of the list, which is the only
//!    part of a reranked list anyone reads. So the fallback fuses the bi-encoder
//!    with BM25 over the same fragments — see [`lexical`][super::lexical] for
//!    why lexical matching earns its place specifically in code search. It is
//!    not a cross-encoder and is not claimed to be one; it is what recovers most
//!    of the gap without a model.
//! 2. **Rerank cost.** With a provider-side reranker, a rerank is one request
//!    and no embedding work at all. Without one, a fragment that is not already
//!    in the store has to be embedded before it can be scored, and the query
//!    always costs one embedding call; fragments embedded during indexing are
//!    read from the store, so the common path is one query embedding per search.
//! 3. **Search is exact and pruned.** The pin's server almost certainly used an
//!    approximate index, which trades recall for latency. This trades neither:
//!    the merkle tree doubles as a ball tree, and a subtree is skipped only when
//!    its *best possible* score cannot reach the current k-th best. Results are
//!    identical to scoring every leaf. What varies is how many leaves have to be
//!    read to get them — see [`vector_index`][super::vector_index] for when that
//!    degrades to all of them.
//! 4. **No cross-repo or shared cache.** `populate_merkle_tree_cache` warmed a
//!    server-side cache that other clients (and other machines) could then hit.
//!    There is no such thing locally; it now builds this machine's search index
//!    instead.
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

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use super::store_client::{IntermediateNode, StoreClient};
use super::vector_index::{ByScore, NodeSummary, unit};
use super::{
    CodebaseContextConfig, ContentHash, EmbeddingConfig, Error, Fragment, NodeHash, RepoMetadata,
    lexical,
};

pub use super::vector_index::cosine_similarity;

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

/// Hard cap on how many merkle nodes one walk — a search descent or an index
/// build — will visit.
///
/// A merkle tree cannot cycle, but a half-written or hand-edited table could,
/// and a query is not the place to discover that by hanging. The cap is far
/// above any real repository's node count.
pub const MAX_WALKED_NODES: usize = 2_000_000;

/// How many frontier nodes a search expands per round trip to the store.
///
/// Best-first search wants to expand exactly one node at a time; a database
/// wants to be asked for many rows at once. This is the compromise: pop up to
/// this many nodes that all still beat the pruning threshold, and resolve them
/// with one query each for children, summaries and vectors.
///
/// It never affects the *result*, only how much work produced it — but it
/// affects that a great deal, and not in the direction one would guess. A batch
/// pops several nodes before any of them has contributed a result, so the
/// pruning threshold is stale for the whole batch, and a large batch simply
/// expands nodes that a threshold one round fresher would have discarded. On the
/// synthetic corpora the tests use, raising this from 8 to 64 took a query over
/// 8,192 fragments from ~70 vectors read to 512, and made a 512-fragment corpus
/// degenerate into a full scan. Eight keeps the descent inside a dozen round
/// trips while leaving the threshold useful.
const FRONTIER_BATCH: usize = 8;

/// The constant in reciprocal rank fusion, `1 / (k + rank)`.
///
/// 60 is the value from Cormack, Clarke & Buettcher (2009), and the one every
/// later hybrid-retrieval system has inherited. It is large enough that the gap
/// between rank 1 and rank 2 does not swamp the other ranker's opinion, which is
/// the entire point of fusing rather than picking.
const RRF_K: f32 = 60.0;

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

/// Scores a query against documents *together*, the way the pin's server-side
/// reranker did.
///
/// This is the one part of the pin's design that a bi-encoder cannot reproduce,
/// so where the user's provider sells the capability the fork buys it rather
/// than approximating it. It is optional by construction: not every provider has
/// a reranking model, and [`LocalStoreClient`] falls back to hybrid scoring
/// rather than requiring one.
///
/// Implemented in `app/` for the same reason as [`EmbeddingProvider`].
#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait RerankProvider: 'static + Send + Sync {
    /// Returns one relevance score per document, in the same order.
    ///
    /// Higher is more relevant. The scale is the provider's own and is never
    /// compared against anything but itself, so no normalization is required.
    async fn rerank(&self, query: &str, documents: Vec<String>) -> Result<Vec<f32>, Error>;
}

/// Local, durable storage for the merkle-node registry, the embedding vectors
/// and the search index built over them.
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

    /// The child lists of `hashes`. A hash with no entry in the returned map has
    /// no recorded children: it is either a leaf or a subtree that has not been
    /// synced.
    ///
    /// This is the primitive every tree walk here is built from, and it replaced
    /// a `leaves_under` that enumerated a whole repository — which each
    /// implementation wrote out separately, and which retrieval no longer wants
    /// at any price: a pruned descent exists precisely so that a query does not
    /// have to list the corpus.
    ///
    /// It takes a batch because a walk proceeds a level at a time, and a
    /// database would rather be asked once for a level than once per node.
    fn children_of(
        &self,
        space: &str,
        hashes: &[NodeHash],
    ) -> anyhow::Result<HashMap<NodeHash, Vec<NodeHash>>>;

    /// The stored vectors for `hashes`. Hashes with no stored vector are
    /// omitted rather than reported — the caller treats "no vector" as "cannot
    /// be scored", which is a normal state during a sync.
    fn vectors_for(
        &self,
        space: &str,
        hashes: &[ContentHash],
    ) -> anyhow::Result<Vec<(ContentHash, Vec<f32>)>>;

    /// Records the search-index summary of each node, replacing any existing
    /// entry.
    ///
    /// Safe to key by node hash alone: a merkle node's hash covers its entire
    /// subtree, so a summary for a given hash describes the same set of
    /// fragments forever. A stored summary can be absent but never stale.
    fn record_node_summaries(
        &self,
        space: &str,
        summaries: &[(NodeHash, NodeSummary)],
    ) -> anyhow::Result<()>;

    /// The stored summaries for `hashes`. A hash with no summary is omitted; the
    /// caller must then treat that subtree as unbounded and open it.
    fn node_summaries_for(
        &self,
        space: &str,
        hashes: &[NodeHash],
    ) -> anyhow::Result<Vec<(NodeHash, NodeSummary)>>;
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
            .unwrap_or(Ordering::Equal)
            .then_with(|| key(left_item).cmp(&key(right_item)))
    });
}

/// Standard competition ranks (1, 2, 2, 4) for `scores`, highest first.
///
/// Ties **must** share a rank. Fusing two rankings means adding their rank
/// contributions, so if a ranker with no opinion at all — BM25 given a query
/// whose terms appear nowhere, which scores every document 0.0 — were allowed to
/// break its own ties arbitrarily, it would inject that arbitrary order into the
/// fused result. Sharing a rank makes an opinionless ranker contribute a
/// constant, which is exactly the right amount of nothing.
fn competition_ranks(scores: &[f32]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..scores.len()).collect();
    order.sort_by(|left, right| scores[*right].total_cmp(&scores[*left]));

    let mut ranks = vec![0usize; scores.len()];
    let mut rank = 0usize;
    for (position, index) in order.iter().enumerate() {
        if position == 0 || scores[*index].total_cmp(&scores[order[position - 1]]) != Ordering::Equal
        {
            rank = position + 1;
        }
        ranks[*index] = rank;
    }
    ranks
}

/// Fuses several rankings of the same items into one score per item.
///
/// Reciprocal rank fusion, `sum over rankers of 1 / (k + rank)`. Chosen over
/// score-weighted fusion because the components are not commensurable: a cosine
/// similarity lives in `[-1, 1]` and clusters tightly near the top, a BM25 score
/// is unbounded and depends on the corpus. Normalizing them onto a common scale
/// would require choosing a normalization, and every choice is a tuning
/// parameter that would need data this fork does not have. Ranks need no such
/// choice.
fn reciprocal_rank_fusion(rankings: &[&[f32]]) -> Vec<f32> {
    let length = rankings.first().map(|scores| scores.len()).unwrap_or(0);
    let mut fused = vec![0.0f32; length];
    for scores in rankings {
        if scores.len() != length {
            continue;
        }
        for (index, rank) in competition_ranks(scores).into_iter().enumerate() {
            fused[index] += 1.0 / (RRF_K + rank as f32);
        }
    }
    fused
}

/// A leaf in the running top-k, ordered the way the exhaustive scan orders its
/// results: by score, then by hash string ascending.
///
/// The tie break is not cosmetic. A bounded heap has to decide which of two
/// equally-scored leaves to evict, and evicting by heap order rather than by the
/// scan's order would make the pruned search return a *different* — though
/// equally good — set than the exhaustive one. Matching the scan's tie break is
/// what lets the tests assert the two are identical.
#[derive(Clone, Debug)]
struct RankedLeaf {
    score: f32,
    key: String,
    hash: ContentHash,
}

impl PartialEq for RankedLeaf {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for RankedLeaf {}

impl PartialOrd for RankedLeaf {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RankedLeaf {
    /// "Greater" means "ranks better".
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.key.cmp(&self.key))
    }
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
    /// The user's cross-encoder, when their provider sells one. `None` is the
    /// ordinary case and means reranking falls back to hybrid scoring.
    rerank_provider: Option<Arc<dyn RerankProvider>>,
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
            rerank_provider: None,
            store,
            config,
        }
    }

    /// Attaches the user's reranking model, if they have one configured.
    ///
    /// Separate from [`new`](Self::new) because a reranker is genuinely optional
    /// — most callers have nothing to pass — and because threading an `Option`
    /// through every construction site would make "no reranker" look like a
    /// decision rather than the default.
    pub fn with_rerank_provider(mut self, provider: Option<Arc<dyn RerankProvider>>) -> Self {
        self.rerank_provider = provider;
        self
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

    /// Builds (or completes) the pruning index for the tree rooted at
    /// `root_hash`, and returns every summary it now knows for that tree.
    ///
    /// # Why it can stop early, and why that is not a cache invalidation problem
    ///
    /// A node's summary is a function of its hash, so a node that already has a
    /// summary needs no work and neither does anything beneath it. The descent
    /// therefore stops at every already-summarized node. After an incremental
    /// re-index only the path from the edited file to the root has new hashes,
    /// so only that path is recomputed — the rest of the repository is a lookup.
    ///
    /// # Why a node can end up with no summary
    ///
    /// A summary is recorded only when *every* leaf beneath the node has a
    /// stored vector. Mid-sync that is often false, and the honest answer is to
    /// record nothing: a summary computed from half a subtree would claim a
    /// radius that excludes the other half, and the search would prune leaves it
    /// should have scored. Absent means "open this subtree", which is slow and
    /// correct. The next build, once the vectors have landed, fills it in.
    ///
    /// The returned map is what lets a search use summaries it has only just
    /// computed. The app's store is written through a queue and read through a
    /// separate connection, so a summary written here is not necessarily
    /// readable yet (see `app::ai::codebase_embeddings`); handing them back
    /// directly sidesteps that entirely for the query that triggered the build.
    fn build_search_index(
        &self,
        space: &str,
        root_hash: &NodeHash,
    ) -> anyhow::Result<HashMap<NodeHash, NodeSummary>> {
        // ---- Phase 1: discover what needs computing, a level at a time. ----
        let mut children_of: HashMap<NodeHash, Vec<NodeHash>> = HashMap::new();
        let mut summaries: HashMap<NodeHash, NodeSummary> = HashMap::new();
        let mut seen: HashSet<NodeHash> = HashSet::from([root_hash.clone()]);
        let mut frontier = vec![root_hash.clone()];
        let mut walked = 0usize;

        while !frontier.is_empty() {
            walked += frontier.len();
            if walked > MAX_WALKED_NODES {
                return Err(anyhow::anyhow!(
                    "codebase index build exceeded {MAX_WALKED_NODES} nodes; the node table is likely corrupt"
                ));
            }

            // Anything already summarized is finished, and so is its subtree.
            for (hash, summary) in self.store.node_summaries_for(space, &frontier)? {
                summaries.insert(hash, summary);
            }
            let pending: Vec<NodeHash> = frontier
                .iter()
                .filter(|hash| !summaries.contains_key(*hash))
                .cloned()
                .collect();
            if pending.is_empty() {
                break;
            }

            let level_children = self.store.children_of(space, &pending)?;
            let mut next = Vec::new();
            for hash in pending {
                // A hash with no child list is a leaf (or an unsynced subtree).
                // Either way there is nothing to descend into and nothing to
                // summarize: its parent will read its vector directly.
                if let Some(children) = level_children.get(&hash) {
                    for child in children {
                        if seen.insert(child.clone()) {
                            next.push(child.clone());
                        }
                    }
                    children_of.insert(hash, children.clone());
                }
            }
            frontier = next;
        }

        // ---- Phase 2: combine, bottom-up. ----
        //
        // A post-order walk rather than a reverse level order, because identical
        // subtrees share a hash: the merkle structure is a DAG, and in a DAG a
        // child can be discovered at a shallower level than one of its parents,
        // which is exactly the case a level order gets wrong.
        //
        // Leaf vectors are fetched per node, as that node is combined, rather
        // than all at once up front. That costs one extra store round trip per
        // node that has leaf children — roughly one per file — but it is the
        // difference between holding a handful of vectors at a time and holding
        // the entire repository's: at 512 dimensions, a 100,000-fragment repo is
        // 200 MB of `f32` if they are all resident. Paying round trips once per
        // sync to avoid that is the right side of the trade; paying them per
        // query would not be, which is why the search does not do this.
        let mut fresh: Vec<(NodeHash, NodeSummary)> = Vec::new();
        let mut in_progress: HashSet<NodeHash> = HashSet::new();
        // Every node whose combine has been attempted. A node's summary is a
        // pure function of its subtree, so a second attempt can only reach the
        // same conclusion — and without this, a subtree shared by several
        // parents (identical files, which is exactly what a merkle DAG dedupes)
        // would be re-walked once per parent.
        let mut attempted: HashSet<NodeHash> = HashSet::new();
        let mut stack: Vec<(NodeHash, bool)> = vec![(root_hash.clone(), false)];

        while let Some((hash, expanded)) = stack.pop() {
            if !expanded {
                if summaries.contains_key(&hash) || attempted.contains(&hash) {
                    continue;
                }
                let Some(children) = children_of.get(&hash) else {
                    // A leaf, or an unsynced subtree. Its parent handles it.
                    continue;
                };
                if !in_progress.insert(hash.clone()) {
                    // Only reachable from a corrupt node table; a merkle DAG has
                    // no cycles. Refusing to descend is what keeps this from
                    // being an infinite loop.
                    log::warn!("Codebase index node table appears to contain a cycle at {hash}");
                    continue;
                }
                let children = children.clone();
                stack.push((hash, true));
                for child in children {
                    stack.push((child, false));
                }
                continue;
            }

            in_progress.remove(&hash);
            attempted.insert(hash.clone());
            let Some(children) = children_of.get(&hash).cloned() else {
                continue;
            };

            // Whichever children are leaves, read now and drop at the end of this
            // iteration.
            let leaf_children: Vec<ContentHash> = children
                .iter()
                .filter(|child| !children_of.contains_key(*child))
                .filter_map(|child| node_hash_to_content_hash(child).ok())
                .collect();
            let leaf_vectors: HashMap<ContentHash, Vec<f32>> = if leaf_children.is_empty() {
                HashMap::new()
            } else {
                self.store
                    .vectors_for(space, &leaf_children)?
                    .into_iter()
                    .collect()
            };

            // `None` anywhere means this subtree is not fully embedded, so it
            // gets no summary at all. See the doc comment.
            let child_summaries: Option<Vec<NodeSummary>> = children
                .iter()
                .map(|child| {
                    if let Some(summary) = summaries.get(child) {
                        return Some(summary.clone());
                    }
                    node_hash_to_content_hash(child)
                        .ok()
                        .and_then(|content_hash| leaf_vectors.get(&content_hash))
                        .map(|vector| NodeSummary::leaf(vector))
                })
                .collect();

            if let Some(child_summaries) = child_summaries
                && let Some(summary) = NodeSummary::combine(&child_summaries)
            {
                fresh.push((hash.clone(), summary.clone()));
                summaries.insert(hash, summary);
            }
        }

        if !fresh.is_empty() {
            self.store.record_node_summaries(space, &fresh)?;
        }

        Ok(summaries)
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl StoreClient for LocalStoreClient {
    /// Records the child lists of intermediate nodes.
    ///
    /// The pin sent these to the server so it could later walk a tree it did not
    /// have the filesystem for. The local store needs them for exactly the same
    /// reason: `get_relevant_fragments` is scoped by descending from a root
    /// hash, and without the child lists there is no tree to descend.
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

    /// Builds this machine's search index for the tree that was just synced.
    ///
    /// At the pin this asked the server to warm a cache keyed by root hash, so
    /// that the *next* client to ask about this tree — possibly on another
    /// machine — would not pay for the walk. There is no shared store here, so
    /// the equivalent is local: compute the [`NodeSummary`] of every node in the
    /// tree that does not already have one, which is what lets a later query
    /// prune instead of scanning. See [`build_search_index`][LocalStoreClient::build_search_index].
    ///
    /// It is deliberately not fatal. The caller attaches the result to telemetry
    /// (`cache_population_error`) and never to correctness, and that is exactly
    /// right: without an index a search is slow, not wrong. A failure here is
    /// reported and the next sync tries again.
    async fn populate_merkle_tree_cache(
        &self,
        embedding_config: EmbeddingConfig,
        root_hash: NodeHash,
        _repo_metadata: RepoMetadata,
    ) -> Result<bool, Error> {
        self.build_search_index(embedding_config.storage_key(), &root_hash)
            .map_err(Error::VectorStore)?;
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

    /// Reorders `fragments` by relevance to `query`.
    ///
    /// Two paths, in quality order:
    ///
    /// 1. **The user's reranking model**, if they have one configured. This is
    ///    the pin's design — a cross-encoder that reads query and fragment
    ///    together — bought from the provider the user already brought. It costs
    ///    one request and, unlike the fallback, no embedding work at all.
    /// 2. **Hybrid vector + lexical scoring**, otherwise. The bi-encoder
    ///    ordering (cosine similarity to the query embedding) is fused with BM25
    ///    over the same fragments. See [`lexical`][super::lexical] for why
    ///    lexical matching is worth this much in code search specifically, and
    ///    [`reciprocal_rank_fusion`] for why the two are combined by rank rather
    ///    than by score.
    ///
    /// A reranker that errors or answers with the wrong number of scores falls
    /// through to the hybrid path rather than failing the search: a worse
    /// ordering of the right fragments beats no results.
    ///
    /// On the hybrid path, fragments already embedded during indexing are scored
    /// from the store, and any fragment with no stored vector is embedded on the
    /// spot — a rerank never silently drops a fragment for lack of a vector.
    async fn rerank_fragments(
        &self,
        query: String,
        fragments: Vec<Fragment>,
    ) -> Result<Vec<Fragment>, Error> {
        if fragments.len() < 2 {
            return Ok(fragments);
        }

        if let Some(reranker) = &self.rerank_provider {
            let documents: Vec<String> = fragments
                .iter()
                .map(|fragment| fragment.content().to_owned())
                .collect();
            match reranker.rerank(&query, documents).await {
                Ok(scores) if scores.len() == fragments.len() => {
                    let mut scored: Vec<(Fragment, f32)> =
                        fragments.into_iter().zip(scores).collect();
                    sort_by_score_desc(&mut scored, |fragment| {
                        fragment.content_hash().to_string()
                    });
                    return Ok(scored.into_iter().map(|(fragment, _)| fragment).collect());
                }
                Ok(scores) => log::warn!(
                    "Rerank provider returned {} scores for {} fragments; \
                     falling back to hybrid reranking",
                    scores.len(),
                    fragments.len()
                ),
                Err(error) => log::warn!(
                    "Rerank provider failed ({error:#}); falling back to hybrid reranking"
                ),
            }
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

        let query_vector = self.embed_one(embedding_config, query.clone()).await?;

        let vector_scores: Vec<f32> = fragments
            .iter()
            .map(|fragment| {
                vectors
                    .get(fragment.content_hash())
                    .map(|vector| cosine_similarity(&query_vector, vector))
                    .unwrap_or(0.0)
            })
            .collect();

        let documents: Vec<&str> = fragments.iter().map(Fragment::content).collect();
        let lexical_scores = lexical::bm25_scores(&query, &documents);

        let fused = reciprocal_rank_fusion(&[&vector_scores, &lexical_scores]);

        let mut scored: Vec<(Fragment, f32)> = fragments.into_iter().zip(fused).collect();
        sort_by_score_desc(&mut scored, |fragment| fragment.content_hash().to_string());

        Ok(scored.into_iter().map(|(fragment, _)| fragment).collect())
    }

    /// Finds the most similar fragments in the tree rooted at `root_hash`.
    ///
    /// Scoping is structural rather than by `repo_metadata`: the search descends
    /// the recorded child lists from `root_hash`, so a fragment that is not
    /// reachable from that root cannot be returned. This is why
    /// `update_intermediate_nodes` has to persist the child lists.
    ///
    /// # The descent
    ///
    /// Best-first over the merkle tree, using each node's [`NodeSummary`] as an
    /// upper bound on what any fragment beneath it could score. A subtree whose
    /// bound is below the current k-th best result cannot contain a result, so
    /// it is never opened. A subtree with no summary is unbounded and is always
    /// opened, which is why an index that has not been built yet is slow rather
    /// than wrong.
    ///
    /// The answer is identical to scoring every reachable leaf — same fragments,
    /// same order. What the index buys is how few of them have to be read.
    async fn get_relevant_fragments(
        &self,
        embedding_config: EmbeddingConfig,
        query: String,
        root_hash: NodeHash,
        _repo_metadata: RepoMetadata,
    ) -> Result<Vec<ContentHash>, Error> {
        let space = embedding_config.storage_key();

        // Establish there is something to search before spending a request on
        // the query embedding. An index that has not synced yet is an empty
        // result, not a failure -- and not a billable one either.
        let root_children = self
            .store
            .children_of(space, std::slice::from_ref(&root_hash))
            .map_err(Error::VectorStore)?;
        if root_children.is_empty() {
            let single = node_hash_to_content_hash(&root_hash)
                .ok()
                .map(|hash| vec![hash])
                .unwrap_or_default();
            let vectors = self
                .store
                .vectors_for(space, &single)
                .map_err(Error::VectorStore)?;
            if vectors.is_empty() {
                return Ok(Vec::new());
            }
        }

        // The index is built at the end of a sync. If it is missing for this
        // root -- the first query after an incremental re-index, or after a sync
        // whose writes have not drained yet -- build it now and use what comes
        // back, rather than scanning the whole repository on every query until
        // something else happens to rebuild it.
        let overlay = if self
            .store
            .node_summaries_for(space, std::slice::from_ref(&root_hash))
            .map_err(Error::VectorStore)?
            .is_empty()
        {
            self.build_search_index(space, &root_hash)
                .map_err(Error::VectorStore)?
        } else {
            HashMap::new()
        };

        let query_vector = self.embed_one(embedding_config, query).await?;
        // A query with no direction cannot bound anything: `upper_bound` sees a
        // width mismatch, answers 1.0 for every node, and the descent degrades
        // to the exhaustive scan -- which then scores everything 0.0, exactly as
        // the unpruned implementation did.
        let query_unit = unit(&query_vector).unwrap_or_default();

        let limit = self.config.max_relevant_fragments.max(1);
        let mut frontier: BinaryHeap<ByScore<NodeHash>> = BinaryHeap::new();
        frontier.push(ByScore {
            score: 1.0,
            item: root_hash,
        });
        let mut best: BinaryHeap<Reverse<RankedLeaf>> = BinaryHeap::new();
        let mut expanded: HashSet<NodeHash> = HashSet::new();
        let mut scored: HashSet<ContentHash> = HashSet::new();
        let mut walked = 0usize;

        loop {
            // The score the k-th best result already has. Nothing whose upper
            // bound falls below it can enter the results, so nothing below it
            // needs opening. `None` until there are k results, because until
            // then every candidate is still in contention.
            let threshold = (best.len() >= limit)
                .then(|| best.peek().map(|Reverse(worst)| worst.score))
                .flatten();

            let mut batch: Vec<NodeHash> = Vec::new();
            while batch.len() < FRONTIER_BATCH {
                let Some(top) = frontier.peek() else { break };
                // Strictly below: an equal bound can still displace an equally
                // scored leaf through the hash tie break, and the whole point of
                // this being exact is that it makes the same choice the
                // exhaustive scan would.
                if threshold.is_some_and(|threshold| top.score < threshold) {
                    break;
                }
                let Some(ByScore { item, .. }) = frontier.pop() else {
                    break;
                };
                if expanded.insert(item.clone()) {
                    batch.push(item);
                }
            }
            if batch.is_empty() {
                break;
            }

            walked += batch.len();
            if walked > MAX_WALKED_NODES {
                return Err(Error::VectorStore(anyhow::anyhow!(
                    "codebase index search exceeded {MAX_WALKED_NODES} nodes; the node table is likely corrupt"
                )));
            }

            let children_of = self
                .store
                .children_of(space, &batch)
                .map_err(Error::VectorStore)?;

            // A node with no child list is a leaf (or an unsynced subtree, which
            // has no vector and so scores nothing). Children come next.
            let mut leaf_candidates: Vec<NodeHash> = Vec::new();
            let mut children: Vec<NodeHash> = Vec::new();
            for hash in &batch {
                match children_of.get(hash) {
                    None => leaf_candidates.push(hash.clone()),
                    Some(child_hashes) => children.extend(
                        child_hashes
                            .iter()
                            .filter(|child| !expanded.contains(*child))
                            .cloned(),
                    ),
                }
            }
            children.sort_by_key(ToString::to_string);
            children.dedup();

            // Summaries first: a child that has one is bounded and goes back on
            // the frontier without being read any further.
            let mut summarized: HashMap<NodeHash, NodeSummary> = HashMap::new();
            let mut unsummarized: Vec<NodeHash> = Vec::new();
            for child in &children {
                match overlay.get(child) {
                    Some(summary) => {
                        summarized.insert(child.clone(), summary.clone());
                    }
                    None => unsummarized.push(child.clone()),
                }
            }
            if !unsummarized.is_empty() {
                for (hash, summary) in self
                    .store
                    .node_summaries_for(space, &unsummarized)
                    .map_err(Error::VectorStore)?
                {
                    summarized.insert(hash, summary);
                }
                unsummarized.retain(|child| !summarized.contains_key(child));
            }

            // A child with no summary is either a leaf -- score it now, its own
            // vector is its exact answer -- or an unindexed subtree, which has to
            // be opened because nothing bounds it.
            leaf_candidates.extend(unsummarized.iter().cloned());
            let lookup: Vec<ContentHash> = leaf_candidates
                .iter()
                .filter_map(|hash| node_hash_to_content_hash(hash).ok())
                .filter(|hash| !scored.contains(hash))
                .collect();
            let mut embedded: HashSet<ContentHash> = HashSet::new();
            if !lookup.is_empty() {
                for (hash, vector) in self
                    .store
                    .vectors_for(space, &lookup)
                    .map_err(Error::VectorStore)?
                {
                    embedded.insert(hash.clone());
                    if !scored.insert(hash.clone()) {
                        continue;
                    }
                    let leaf = RankedLeaf {
                        score: cosine_similarity(&query_vector, &vector),
                        key: hash.to_string(),
                        hash,
                    };
                    if best.len() < limit {
                        best.push(Reverse(leaf));
                    } else if best.peek().is_some_and(|Reverse(worst)| *worst < leaf) {
                        best.pop();
                        best.push(Reverse(leaf));
                    }
                }
            }

            for child in unsummarized {
                let is_embedded_leaf = node_hash_to_content_hash(&child)
                    .is_ok_and(|hash| embedded.contains(&hash) || scored.contains(&hash));
                if !is_embedded_leaf {
                    // Unknown extent: it could contain anything, so it cannot be
                    // pruned.
                    frontier.push(ByScore {
                        score: 1.0,
                        item: child,
                    });
                }
            }

            for (hash, summary) in summarized {
                frontier.push(ByScore {
                    score: summary.upper_bound(&query_unit),
                    item: hash,
                });
            }
        }

        let mut ranked: Vec<RankedLeaf> = best.into_vec().into_iter().map(|item| item.0).collect();
        ranked.sort_by(|left, right| right.cmp(left));

        Ok(ranked.into_iter().map(|leaf| leaf.hash).collect())
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
/// implementation of [`VectorStore`] does.
pub fn node_hash_to_content_hash(hash: &NodeHash) -> Result<ContentHash, Error> {
    ContentHash::from_str(&hash.to_string())
}

#[cfg(test)]
#[path = "local_store_client_tests.rs"]
mod tests;
