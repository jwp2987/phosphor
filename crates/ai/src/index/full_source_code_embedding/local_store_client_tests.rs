//! Tests for [`LocalStoreClient`].
//!
//! These are new, not ported: there is no pin equivalent, because at the pin
//! every one of these behaviours lived on the server. They cover the contracts
//! the rest of the subsystem relies on — in particular that `sync_merkle_tree`
//! returns the *un*known hashes, that retrieval is scoped by reachability from
//! the root, and that a missing provider is reported rather than swallowed.
//!
//! Two groups are about the regressions the BYOP store shipped with:
//!
//! * `search_*` and `index_*` measure the pruned descent. The claim being tested
//!   is not "it is faster" — a wall clock would measure this machine, not the
//!   algorithm — but the two things that make it faster and keep it honest: the
//!   number of leaf vectors a query has to read grows far more slowly than the
//!   corpus, and the results are *identical* to scoring every leaf.
//! * `rerank_*` and `ranking_quality_*` measure the ordering. The fusion is only
//!   worth having if it beats the bi-encoder it replaces, so there is a fixture
//!   with known-correct answers and an MRR comparison, not an assertion that one
//!   hand-picked query comes out right.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use string_offset::ByteOffset;

use super::*;

/// An in-memory [`VectorStore`], so the client's logic can be tested without a
/// database — and so the cost of a query can be counted rather than timed.
///
/// The counters are the whole measurement apparatus for the complexity claim. A
/// query's cost is dominated by how many stored vectors it has to read and
/// decode, so counting rows read is a direct, machine-independent proxy for the
/// work a real SQLite-backed store would do.
#[derive(Default)]
struct InMemoryVectorStore {
    nodes: Mutex<HashMap<String, HashMap<String, Vec<String>>>>,
    vectors: Mutex<HashMap<String, HashMap<String, Vec<f32>>>>,
    summaries: Mutex<HashMap<String, HashMap<String, NodeSummary>>>,
    /// Leaf vectors returned to the caller.
    vectors_read: AtomicUsize,
    /// Node summaries returned to the caller.
    summaries_read: AtomicUsize,
    /// Calls to `children_of`, i.e. round trips the descent made.
    children_queries: AtomicUsize,
}

impl InMemoryVectorStore {
    fn reset_counters(&self) {
        self.vectors_read.store(0, AtomicOrdering::Relaxed);
        self.summaries_read.store(0, AtomicOrdering::Relaxed);
        self.children_queries.store(0, AtomicOrdering::Relaxed);
    }

    fn vectors_read(&self) -> usize {
        self.vectors_read.load(AtomicOrdering::Relaxed)
    }

    fn summaries_read(&self) -> usize {
        self.summaries_read.load(AtomicOrdering::Relaxed)
    }

    fn children_queries(&self) -> usize {
        self.children_queries.load(AtomicOrdering::Relaxed)
    }

    fn summary_count(&self) -> usize {
        self.summaries
            .lock()
            .unwrap()
            .values()
            .map(|space| space.len())
            .sum()
    }
}

impl VectorStore for InMemoryVectorStore {
    fn record_nodes(&self, space: &str, nodes: &[IntermediateNode]) -> anyhow::Result<()> {
        let mut all = self.nodes.lock().unwrap();
        let space_nodes = all.entry(space.to_owned()).or_default();
        for node in nodes {
            space_nodes.insert(
                node.hash.to_string(),
                node.children.iter().map(ToString::to_string).collect(),
            );
        }
        Ok(())
    }

    fn known_hashes(&self, space: &str, hashes: &[NodeHash]) -> anyhow::Result<HashSet<NodeHash>> {
        let nodes = self.nodes.lock().unwrap();
        let vectors = self.vectors.lock().unwrap();
        let space_nodes = nodes.get(space);
        let space_vectors = vectors.get(space);

        Ok(hashes
            .iter()
            .filter(|hash| {
                let key = hash.to_string();
                space_nodes.is_some_and(|nodes| nodes.contains_key(&key))
                    || space_vectors.is_some_and(|vectors| vectors.contains_key(&key))
            })
            .cloned()
            .collect())
    }

    fn record_embeddings(
        &self,
        space: &str,
        embeddings: &[(ContentHash, Vec<f32>)],
    ) -> anyhow::Result<()> {
        let mut all = self.vectors.lock().unwrap();
        let space_vectors = all.entry(space.to_owned()).or_default();
        for (hash, vector) in embeddings {
            space_vectors.insert(hash.to_string(), vector.clone());
        }
        Ok(())
    }

    fn children_of(
        &self,
        space: &str,
        hashes: &[NodeHash],
    ) -> anyhow::Result<HashMap<NodeHash, Vec<NodeHash>>> {
        self.children_queries.fetch_add(1, AtomicOrdering::Relaxed);
        let nodes = self.nodes.lock().unwrap();
        let Some(space_nodes) = nodes.get(space) else {
            return Ok(HashMap::new());
        };
        Ok(hashes
            .iter()
            .filter_map(|hash| {
                space_nodes.get(&hash.to_string()).map(|children| {
                    (
                        hash.clone(),
                        children
                            .iter()
                            .filter_map(|child| NodeHash::from_str(child).ok())
                            .collect(),
                    )
                })
            })
            .collect())
    }

    fn record_node_summaries(
        &self,
        space: &str,
        summaries: &[(NodeHash, NodeSummary)],
    ) -> anyhow::Result<()> {
        let mut all = self.summaries.lock().unwrap();
        let space_summaries = all.entry(space.to_owned()).or_default();
        for (hash, summary) in summaries {
            space_summaries.insert(hash.to_string(), summary.clone());
        }
        Ok(())
    }

    fn node_summaries_for(
        &self,
        space: &str,
        hashes: &[NodeHash],
    ) -> anyhow::Result<Vec<(NodeHash, NodeSummary)>> {
        let summaries = self.summaries.lock().unwrap();
        let Some(space_summaries) = summaries.get(space) else {
            return Ok(Vec::new());
        };
        let found: Vec<(NodeHash, NodeSummary)> = hashes
            .iter()
            .filter_map(|hash| {
                space_summaries
                    .get(&hash.to_string())
                    .map(|summary| (hash.clone(), summary.clone()))
            })
            .collect();
        self.summaries_read
            .fetch_add(found.len(), AtomicOrdering::Relaxed);
        Ok(found)
    }

    fn vectors_for(
        &self,
        space: &str,
        hashes: &[ContentHash],
    ) -> anyhow::Result<Vec<(ContentHash, Vec<f32>)>> {
        let vectors = self.vectors.lock().unwrap();
        let Some(space_vectors) = vectors.get(space) else {
            return Ok(Vec::new());
        };
        let found: Vec<(ContentHash, Vec<f32>)> = hashes
            .iter()
            .filter_map(|hash| {
                space_vectors
                    .get(&hash.to_string())
                    .map(|vector| (hash.clone(), vector.clone()))
            })
            .collect();
        self.vectors_read
            .fetch_add(found.len(), AtomicOrdering::Relaxed);
        Ok(found)
    }
}

/// An [`EmbeddingProvider`] that maps a fixed table of strings to vectors, and
/// records what it was asked for.
struct ScriptedProvider {
    table: HashMap<String, Vec<f32>>,
    calls: Mutex<Vec<Vec<String>>>,
}

impl ScriptedProvider {
    fn new(entries: &[(&str, &[f32])]) -> Self {
        Self {
            table: entries
                .iter()
                .map(|(text, vector)| ((*text).to_owned(), vector.to_vec()))
                .collect(),
            calls: Mutex::new(Vec::new()),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
}

#[async_trait]
impl EmbeddingProvider for ScriptedProvider {
    async fn embed(
        &self,
        _embedding_config: EmbeddingConfig,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, Error> {
        self.calls.lock().unwrap().push(texts.clone());
        texts
            .into_iter()
            .map(|text| {
                self.table
                    .get(&text)
                    .cloned()
                    .ok_or_else(|| Error::Other(anyhow::anyhow!("unscripted input: {text}")))
            })
            .collect()
    }
}

/// An [`EmbeddingProvider`] that answers every request with the same vector.
///
/// The retrieval tests care about the *query* embedding and supply the corpus
/// directly, so scripting every input would be noise.
struct FixedProvider {
    vector: Vec<f32>,
    calls: AtomicUsize,
}

impl FixedProvider {
    fn new(vector: Vec<f32>) -> Self {
        Self {
            vector,
            calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl EmbeddingProvider for FixedProvider {
    async fn embed(
        &self,
        _embedding_config: EmbeddingConfig,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, Error> {
        self.calls.fetch_add(1, AtomicOrdering::Relaxed);
        Ok(texts.iter().map(|_| self.vector.clone()).collect())
    }
}

/// A [`RerankProvider`] that scores documents from a fixed table, and can be
/// told to fail instead.
struct ScriptedReranker {
    scores: HashMap<String, f32>,
    fail: bool,
    calls: AtomicUsize,
}

impl ScriptedReranker {
    fn new(entries: &[(&str, f32)]) -> Self {
        Self {
            scores: entries
                .iter()
                .map(|(text, score)| ((*text).to_owned(), *score))
                .collect(),
            fail: false,
            calls: AtomicUsize::new(0),
        }
    }

    fn failing() -> Self {
        Self {
            scores: HashMap::new(),
            fail: true,
            calls: AtomicUsize::new(0),
        }
    }

    fn call_count(&self) -> usize {
        self.calls.load(AtomicOrdering::Relaxed)
    }
}

#[async_trait]
impl RerankProvider for ScriptedReranker {
    async fn rerank(&self, _query: &str, documents: Vec<String>) -> Result<Vec<f32>, Error> {
        self.calls.fetch_add(1, AtomicOrdering::Relaxed);
        if self.fail {
            return Err(Error::Other(anyhow::anyhow!("reranker is down")));
        }
        Ok(documents
            .iter()
            .map(|document| self.scores.get(document).copied().unwrap_or(0.0))
            .collect())
    }
}

/// A provider that is not configured, which is the state a fresh install is in.
struct UnconfiguredProvider;

#[async_trait]
impl EmbeddingProvider for UnconfiguredProvider {
    async fn embed(
        &self,
        embedding_config: EmbeddingConfig,
        _texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, Error> {
        Err(Error::NoEmbeddingProvider {
            model: embedding_config.model_id(),
        })
    }
}

fn fragment(content: &str) -> Fragment {
    Fragment::from_byte_range(
        content.to_owned(),
        ContentHash::from_content(content),
        PathBuf::from("/repo/src/lib.rs"),
        ByteOffset::from(0)..ByteOffset::from(content.len()),
    )
}

fn client(provider: Arc<dyn EmbeddingProvider>, store: Arc<dyn VectorStore>) -> LocalStoreClient {
    LocalStoreClient::new(provider, store, LocalCodebaseContextConfig::default())
}

fn node(hash: &str) -> NodeHash {
    NodeHash::from(ContentHash::from_content(hash))
}

#[test]
fn cosine_similarity_is_one_for_identical_vectors() {
    let similarity = cosine_similarity(&[1.0, 2.0, 3.0], &[1.0, 2.0, 3.0]);
    assert!(
        (similarity - 1.0).abs() < 1e-6,
        "identical vectors must score 1.0, got {similarity}"
    );
}

#[test]
fn cosine_similarity_is_scale_invariant() {
    let similarity = cosine_similarity(&[1.0, 2.0, 3.0], &[10.0, 20.0, 30.0]);
    assert!(
        (similarity - 1.0).abs() < 1e-6,
        "a scaled copy points the same way and must score 1.0, got {similarity}"
    );
}

#[test]
fn cosine_similarity_is_zero_for_orthogonal_vectors() {
    assert!(cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
}

#[test]
fn cosine_similarity_is_negative_for_opposed_vectors() {
    let similarity = cosine_similarity(&[1.0, 0.0], &[-1.0, 0.0]);
    assert!(
        (similarity + 1.0).abs() < 1e-6,
        "opposed vectors must score -1.0, got {similarity}"
    );
}

#[test]
fn cosine_similarity_never_returns_nan() {
    // A zero vector has no direction; a length mismatch is a corrupt row. Both
    // must score 0.0 rather than NaN, because a NaN in a comparator makes the
    // sort order arbitrary.
    assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    assert_eq!(cosine_similarity(&[1.0], &[1.0, 1.0]), 0.0);
    assert_eq!(cosine_similarity(&[], &[]), 0.0);
}

#[test]
fn sync_merkle_tree_returns_the_hashes_the_store_does_not_have() {
    block_on(async {
        let store = Arc::new(InMemoryVectorStore::default());
        let known = node("known");
        let unknown = node("unknown");
        store
            .record_nodes(
                EmbeddingConfig::default().storage_key(),
                &[IntermediateNode {
                    hash: known.clone(),
                    children: vec![],
                }],
            )
            .unwrap();

        let client = client(Arc::new(ScriptedProvider::new(&[])), store);
        let need_sync = client
            .sync_merkle_tree(
                vec![known.clone(), unknown.clone()],
                EmbeddingConfig::default(),
            )
            .await
            .expect("sync_merkle_tree should succeed");

        assert_eq!(
            need_sync,
            HashSet::from([unknown]),
            "only the hash the store has never seen needs syncing"
        );
    });
}

#[test]
fn sync_merkle_tree_treats_an_embedded_leaf_as_already_synced() {
    block_on(async {
        // A leaf is "present" because its vector exists, not because it has a node
        // row -- leaves never go through update_intermediate_nodes.
        let store = Arc::new(InMemoryVectorStore::default());
        let leaf_content = ContentHash::from_content("fn main() {}");
        store
            .record_embeddings(
                EmbeddingConfig::default().storage_key(),
                &[(leaf_content.clone(), vec![1.0, 0.0])],
            )
            .unwrap();

        let client = client(Arc::new(ScriptedProvider::new(&[])), store);
        let need_sync = client
            .sync_merkle_tree(
                vec![NodeHash::from(leaf_content)],
                EmbeddingConfig::default(),
            )
            .await
            .expect("sync_merkle_tree should succeed");

        assert!(
            need_sync.is_empty(),
            "an already-embedded leaf must not be re-synced"
        );
    });
}

#[test]
fn sync_merkle_tree_is_scoped_by_embedding_model() {
    block_on(async {
        // Vectors from two models live in different spaces and must never satisfy
        // each other -- mixing vector spaces silently produces nonsense rankings.
        let store = Arc::new(InMemoryVectorStore::default());
        let hash = node("shared");
        store
            .record_nodes(
                EmbeddingConfig::Voyage3_5_512.storage_key(),
                &[IntermediateNode {
                    hash: hash.clone(),
                    children: vec![],
                }],
            )
            .unwrap();

        let client = client(Arc::new(ScriptedProvider::new(&[])), store);
        let need_sync = client
            .sync_merkle_tree(vec![hash.clone()], EmbeddingConfig::OpenAiTextSmall3_256)
            .await
            .expect("sync_merkle_tree should succeed");

        assert_eq!(
            need_sync,
            HashSet::from([hash]),
            "a node stored under one model must look unsynced to another"
        );
    });
}

#[test]
fn generate_embeddings_stores_one_vector_per_fragment() {
    block_on(async {
        let store = Arc::new(InMemoryVectorStore::default());
        let provider = Arc::new(ScriptedProvider::new(&[
            ("alpha", &[1.0, 0.0]),
            ("beta", &[0.0, 1.0]),
        ]));
        let client = client(provider.clone(), store.clone());

        let fragments = vec![fragment("alpha"), fragment("beta")];
        let statuses = client
            .generate_embeddings(
                EmbeddingConfig::default(),
                fragments,
                node("root"),
                RepoMetadata { path: None },
            )
            .await
            .expect("generate_embeddings should succeed");

        assert_eq!(statuses.len(), 2);
        assert!(statuses.values().all(|ok| *ok));
        assert_eq!(
            provider.call_count(),
            1,
            "a batch of fragments must cost exactly one provider request"
        );

        let stored = store
            .vectors_for(
                EmbeddingConfig::default().storage_key(),
                &[
                    ContentHash::from_content("alpha"),
                    ContentHash::from_content("beta"),
                ],
            )
            .unwrap();
        assert_eq!(stored.len(), 2, "both vectors must be persisted");
    });
}

#[test]
fn generate_embeddings_fails_loudly_when_no_provider_is_configured() {
    block_on(async {
        let client = client(
            Arc::new(UnconfiguredProvider),
            Arc::new(InMemoryVectorStore::default()),
        );

        let error = client
            .generate_embeddings(
                EmbeddingConfig::default(),
                vec![fragment("alpha")],
                node("root"),
                RepoMetadata { path: None },
            )
            .await
            .expect_err("an unconfigured provider must be an error, never an empty success");

        assert!(
            matches!(error, Error::NoEmbeddingProvider { .. }),
            "expected NoEmbeddingProvider, got {error}"
        );
    });
}

#[test]
fn generate_embeddings_rejects_a_short_provider_response() {
    block_on(async {
        // Results are paired with inputs positionally, so a short return would
        // silently mislabel vectors. It must be an error.
        let store = Arc::new(InMemoryVectorStore::default());
        let provider = Arc::new(ScriptedProvider::new(&[("alpha", &[1.0, 0.0])]));
        let client = client(provider, store);

        let error = client
            .generate_embeddings(
                EmbeddingConfig::default(),
                vec![fragment("alpha"), fragment("beta")],
                node("root"),
                RepoMetadata { path: None },
            )
            .await
            .expect_err("an unscripted input must fail rather than return fewer vectors");

        assert!(matches!(error, Error::Other(_)), "got {error}");
    });
}

#[test]
fn get_relevant_fragments_ranks_by_similarity_to_the_query() {
    block_on(async {
        let space = EmbeddingConfig::default().storage_key();
        let store = Arc::new(InMemoryVectorStore::default());

        let near = ContentHash::from_content("near");
        let far = ContentHash::from_content("far");
        let root = node("root");

        store
            .record_nodes(
                space,
                &[IntermediateNode {
                    hash: root.clone(),
                    children: vec![NodeHash::from(near.clone()), NodeHash::from(far.clone())],
                }],
            )
            .unwrap();
        store
            .record_embeddings(
                space,
                &[
                    (near.clone(), vec![1.0, 0.0]),
                    (far.clone(), vec![0.0, 1.0]),
                ],
            )
            .unwrap();

        let provider = Arc::new(ScriptedProvider::new(&[("query", &[0.9, 0.1])]));
        let client = client(provider, store);

        let ranked = client
            .get_relevant_fragments(
                EmbeddingConfig::default(),
                "query".to_owned(),
                root,
                RepoMetadata { path: None },
            )
            .await
            .expect("retrieval should succeed");

        assert_eq!(
            ranked,
            vec![near, far],
            "the fragment pointing the same way as the query must rank first"
        );
    });
}

#[test]
fn get_relevant_fragments_is_scoped_to_the_requested_root() {
    block_on(async {
        // The pin scoped a query by passing the repo path to the server. Locally the
        // scope is structural: unreachable leaves must not leak into results, even
        // though they sit in the same table.
        let space = EmbeddingConfig::default().storage_key();
        let store = Arc::new(InMemoryVectorStore::default());

        let mine = ContentHash::from_content("mine");
        let other_repo = ContentHash::from_content("other_repo");
        let root = node("root");
        let unrelated_root = node("unrelated_root");

        store
            .record_nodes(
                space,
                &[
                    IntermediateNode {
                        hash: root.clone(),
                        children: vec![NodeHash::from(mine.clone())],
                    },
                    IntermediateNode {
                        hash: unrelated_root,
                        children: vec![NodeHash::from(other_repo.clone())],
                    },
                ],
            )
            .unwrap();
        store
            .record_embeddings(
                space,
                &[(mine.clone(), vec![1.0, 0.0]), (other_repo, vec![1.0, 0.0])],
            )
            .unwrap();

        let provider = Arc::new(ScriptedProvider::new(&[("query", &[1.0, 0.0])]));
        let client = client(provider, store);

        let ranked = client
            .get_relevant_fragments(
                EmbeddingConfig::default(),
                "query".to_owned(),
                root,
                RepoMetadata { path: None },
            )
            .await
            .expect("retrieval should succeed");

        assert_eq!(
            ranked,
            vec![mine],
            "only leaves reachable from the requested root may be returned"
        );
    });
}

#[test]
fn get_relevant_fragments_returns_nothing_without_embedding_when_the_tree_is_empty() {
    block_on(async {
        // An index that has not synced yet has no leaves. That is an empty result,
        // not an error -- but it must not cost a provider request either.
        let provider = Arc::new(ScriptedProvider::new(&[]));
        let client = client(provider.clone(), Arc::new(InMemoryVectorStore::default()));

        let ranked = client
            .get_relevant_fragments(
                EmbeddingConfig::default(),
                "query".to_owned(),
                node("root"),
                RepoMetadata { path: None },
            )
            .await
            .expect("an empty tree is an empty result, not a failure");

        assert!(ranked.is_empty());
        assert_eq!(
            provider.call_count(),
            0,
            "an empty tree must not spend an embedding request"
        );
    });
}

#[test]
fn get_relevant_fragments_reports_a_missing_provider() {
    block_on(async {
        let space = EmbeddingConfig::default().storage_key();
        let store = Arc::new(InMemoryVectorStore::default());
        let leaf = ContentHash::from_content("leaf");
        let root = node("root");
        store
            .record_nodes(
                space,
                &[IntermediateNode {
                    hash: root.clone(),
                    children: vec![NodeHash::from(leaf.clone())],
                }],
            )
            .unwrap();
        store
            .record_embeddings(space, &[(leaf, vec![1.0, 0.0])])
            .unwrap();

        let client = client(Arc::new(UnconfiguredProvider), store);
        let error = client
            .get_relevant_fragments(
                EmbeddingConfig::default(),
                "query".to_owned(),
                root,
                RepoMetadata { path: None },
            )
            .await
            .expect_err("a search with no provider must say so, not return no matches");

        assert!(matches!(error, Error::NoEmbeddingProvider { .. }));
    });
}

#[test]
fn rerank_fragments_orders_by_similarity_to_the_query() {
    block_on(async {
        let space = EmbeddingConfig::default().storage_key();
        let store = Arc::new(InMemoryVectorStore::default());
        store
            .record_embeddings(
                space,
                &[
                    (ContentHash::from_content("alpha"), vec![0.0, 1.0]),
                    (ContentHash::from_content("beta"), vec![1.0, 0.0]),
                ],
            )
            .unwrap();

        let provider = Arc::new(ScriptedProvider::new(&[("query", &[1.0, 0.0])]));
        let client = client(provider.clone(), store);

        let reranked = client
            .rerank_fragments(
                "query".to_owned(),
                vec![fragment("alpha"), fragment("beta")],
            )
            .await
            .expect("rerank should succeed");

        assert_eq!(
            reranked
                .iter()
                .map(|fragment| fragment.content().to_owned())
                .collect::<Vec<_>>(),
            vec!["beta".to_owned(), "alpha".to_owned()],
            "the fragment closest to the query must come first"
        );
        assert_eq!(
            provider.call_count(),
            1,
            "fragments already in the store cost only the query embedding"
        );
    });
}

#[test]
fn rerank_fragments_embeds_fragments_the_store_does_not_have() {
    block_on(async {
        let provider = Arc::new(ScriptedProvider::new(&[
            ("query", &[1.0, 0.0]),
            ("alpha", &[0.0, 1.0]),
            ("beta", &[1.0, 0.0]),
        ]));
        let store = Arc::new(InMemoryVectorStore::default());
        let client = client(provider.clone(), store.clone());

        let reranked = client
            .rerank_fragments(
                "query".to_owned(),
                vec![fragment("alpha"), fragment("beta")],
            )
            .await
            .expect("rerank should succeed");

        assert_eq!(
            reranked
                .iter()
                .map(|fragment| fragment.content().to_owned())
                .collect::<Vec<_>>(),
            vec!["beta".to_owned(), "alpha".to_owned()],
            "a fragment with no stored vector must still be scored, not dropped"
        );
        assert_eq!(
            provider.call_count(),
            2,
            "one batch for the missing fragments, one for the query"
        );

        let cached = store
            .vectors_for(
                EmbeddingConfig::default().storage_key(),
                &[
                    ContentHash::from_content("alpha"),
                    ContentHash::from_content("beta"),
                ],
            )
            .unwrap();
        assert_eq!(
            cached.len(),
            2,
            "vectors computed during a rerank must be cached for next time"
        );
    });
}

#[test]
fn rerank_fragments_short_circuits_below_two_fragments() {
    block_on(async {
        // Nothing to reorder, so it must not spend an embedding request.
        let provider = Arc::new(ScriptedProvider::new(&[]));
        let client = client(provider.clone(), Arc::new(InMemoryVectorStore::default()));

        let reranked = client
            .rerank_fragments("query".to_owned(), vec![fragment("alpha")])
            .await
            .expect("a single fragment needs no reranking");

        assert_eq!(reranked.len(), 1);
        assert_eq!(provider.call_count(), 0);
    });
}

#[test]
fn update_intermediate_nodes_reports_every_node_stored() {
    block_on(async {
        let store = Arc::new(InMemoryVectorStore::default());
        let client = client(Arc::new(ScriptedProvider::new(&[])), store.clone());

        let parent = node("parent");
        let child = node("child");
        let statuses = client
            .update_intermediate_nodes(
                EmbeddingConfig::default(),
                vec![IntermediateNode {
                    hash: parent.clone(),
                    children: vec![child.clone()],
                }],
            )
            .await
            .expect("recording nodes should succeed");

        assert_eq!(statuses, HashMap::from([(parent.clone(), true)]));

        let known = store
            .known_hashes(EmbeddingConfig::default().storage_key(), &[parent])
            .unwrap();
        assert_eq!(known.len(), 1, "the node must be readable back");
    });
}

#[test]
fn codebase_context_config_reports_the_local_defaults() {
    block_on(async {
        let client = LocalStoreClient::new(
            Arc::new(ScriptedProvider::new(&[])),
            Arc::new(InMemoryVectorStore::default()),
            LocalCodebaseContextConfig {
                embedding_config: EmbeddingConfig::VoyageCode3_512,
                embedding_cadence: Duration::from_secs(60),
                max_relevant_fragments: 10,
            },
        );

        let config = client
            .codebase_context_config()
            .await
            .expect("local config never fails");

        assert_eq!(config.embedding_config, EmbeddingConfig::VoyageCode3_512);
        assert_eq!(config.embedding_cadence, Duration::from_secs(60));
    });
}

#[test]
fn populate_merkle_tree_cache_succeeds_on_a_tree_with_nothing_to_index() {
    block_on(async {
        // There is no remote cache to warm, so this builds the local search
        // index instead. An empty store has no tree, which is not a failure --
        // it is the state before the first sync.
        let store = Arc::new(InMemoryVectorStore::default());
        let client = client(Arc::new(ScriptedProvider::new(&[])), store.clone());

        assert!(
            client
                .populate_merkle_tree_cache(
                    EmbeddingConfig::default(),
                    node("root"),
                    RepoMetadata { path: None },
                )
                .await
                .expect("an empty tree means nothing to index, not something to fail"),
        );
        assert_eq!(
            store.summary_count(),
            0,
            "a node with nothing embedded beneath it must not get a summary that \
             claims coverage the index does not have"
        );
    });
}

#[test]
fn embedding_config_storage_keys_round_trip_and_are_distinct() {
    // The storage key is what keeps two models' vectors apart, so a collision
    // would silently mix vector spaces.
    let all = [
        EmbeddingConfig::OpenAiTextSmall3_256,
        EmbeddingConfig::VoyageCode3_512,
        EmbeddingConfig::Voyage3_5_Lite_512,
        EmbeddingConfig::Voyage3_5_512,
        EmbeddingConfig::Voyage4_512,
    ];

    let keys: HashSet<&str> = all.iter().map(|config| config.storage_key()).collect();
    assert_eq!(keys.len(), all.len(), "storage keys must be distinct");

    for config in all {
        assert_eq!(
            EmbeddingConfig::from_storage_key(config.storage_key()),
            Some(config),
            "{config:?} must round-trip through its storage key"
        );
    }
    assert_eq!(EmbeddingConfig::from_storage_key("nonsense"), None);
}

// ---------------------------------------------------------------------------
// The search index: what it costs, and the exactness that cost may not buy.
// ---------------------------------------------------------------------------

/// A deterministic generator, so a "random" corpus is the same corpus on every
/// machine and every run.
///
/// A flaky cost assertion is worse than none: it gets muted, and then the
/// regression it was guarding lands unopposed.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(2_862_933_555_777_941_757).wrapping_add(1))
    }

    /// The next value in `[-1.0, 1.0)`.
    fn next_signed(&mut self) -> f32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let unit = ((self.0 >> 40) as f32) / ((1u64 << 24) as f32);
        unit * 2.0 - 1.0
    }

    fn next_vector(&mut self, dimensions: usize) -> Vec<f32> {
        (0..dimensions).map(|_| self.next_signed()).collect()
    }
}

/// A synthetic repository: a balanced merkle tree over `leaf_count` fragments,
/// with vectors laid out the way a real repository's are.
///
/// Every node's direction is its parent's plus noise, and `hierarchical` decides
/// how that noise is scaled — which is the whole point of the fixture, because
/// it is the property the pruning bound lives or dies on:
///
/// * `hierarchical: true` scales the noise by depth, so the top-level split is
///   wide and each level below it is tighter. That is what a repository looks
///   like: `crates/ai` and `app/src/terminal` have almost nothing in common,
///   while two files in the same directory have almost everything in common.
/// * `hierarchical: false` applies the same noise at every level, so depth in
///   the tree says nothing about distance in embedding space. No real repository
///   looks like this, and it is here to prove the search is still *correct* when
///   the assumption it is tuned for does not hold.
struct SyntheticRepo {
    root: NodeHash,
    nodes: Vec<IntermediateNode>,
    leaves: Vec<(ContentHash, Vec<f32>)>,
}

fn synthetic_repo(
    leaf_count: usize,
    branching: usize,
    dimensions: usize,
    spread: f32,
    hierarchical: bool,
    seed: u64,
) -> SyntheticRepo {
    assert!(leaf_count > 0 && branching > 1);

    // Structure first: level 0 is the fragments, and each level above groups the
    // one below into `branching`-sized nodes, up to a single root.
    let mut levels: Vec<Vec<String>> = vec![(0..leaf_count).map(|i| format!("leaf-{i}")).collect()];
    while levels.last().expect("at least one level").len() > 1 {
        let depth = levels.len();
        let below = levels.last().expect("at least one level").len();
        levels.push(
            (0..below.div_ceil(branching))
                .map(|index| format!("node-{depth}-{index}"))
                .collect(),
        );
    }

    let mut nodes = Vec::new();
    for depth in 1..levels.len() {
        for (index, name) in levels[depth].iter().enumerate() {
            let children: Vec<NodeHash> = levels[depth - 1]
                .iter()
                .skip(index * branching)
                .take(branching)
                .map(|child| NodeHash::from(ContentHash::from_content(child)))
                .collect();
            nodes.push(IntermediateNode {
                hash: NodeHash::from(ContentHash::from_content(name)),
                children,
            });
        }
    }

    // Directions, top down: a child points where its parent does, plus noise.
    let mut rng = Lcg::new(seed);
    let mut directions: HashMap<String, Vec<f32>> = HashMap::new();
    let root_name = levels
        .last()
        .expect("at least one level")
        .first()
        .expect("exactly one root")
        .clone();
    directions.insert(root_name.clone(), rng.next_vector(dimensions));

    for depth in (1..levels.len()).rev() {
        let magnitude = if hierarchical {
            spread * depth as f32
        } else {
            spread
        };
        for (index, name) in levels[depth].iter().enumerate() {
            let parent = directions
                .get(name)
                .cloned()
                .unwrap_or_else(|| vec![0.0; dimensions]);
            for child in levels[depth - 1]
                .iter()
                .skip(index * branching)
                .take(branching)
            {
                let noise = rng.next_vector(dimensions);
                let direction: Vec<f32> = parent
                    .iter()
                    .zip(noise.iter())
                    .map(|(base, offset)| base + magnitude * offset)
                    .collect();
                directions.insert(child.clone(), direction);
            }
        }
    }

    let leaves = levels[0]
        .iter()
        .map(|name| {
            (
                ContentHash::from_content(name),
                directions
                    .get(name)
                    .cloned()
                    .expect("every leaf gets a direction"),
            )
        })
        .collect();

    SyntheticRepo {
        root: NodeHash::from(ContentHash::from_content(&root_name)),
        nodes,
        leaves,
    }
}

/// Loads a synthetic repository's nodes and vectors into a store.
fn load_repo(store: &InMemoryVectorStore, repo: &SyntheticRepo) {
    let space = EmbeddingConfig::default().storage_key();
    store.record_nodes(space, &repo.nodes).expect("records");
    store
        .record_embeddings(space, &repo.leaves)
        .expect("records");
}

/// What scoring every leaf would return: the answer the pruned search is
/// required to match, computed independently of it.
fn exhaustive_ranking(
    leaves: &[(ContentHash, Vec<f32>)],
    query: &[f32],
    limit: usize,
) -> Vec<ContentHash> {
    let mut scored: Vec<(ContentHash, f32)> = leaves
        .iter()
        .map(|(hash, vector)| (hash.clone(), cosine_similarity(query, vector)))
        .collect();
    scored.sort_by(|(left_hash, left), (right_hash, right)| {
        right
            .partial_cmp(left)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left_hash.to_string().cmp(&right_hash.to_string()))
    });
    scored.truncate(limit);
    scored.into_iter().map(|(hash, _)| hash).collect()
}

fn indexed_client(
    store: Arc<InMemoryVectorStore>,
    repo: &SyntheticRepo,
    query_vector: Vec<f32>,
    limit: usize,
) -> LocalStoreClient {
    load_repo(&store, repo);
    let client = LocalStoreClient::new(
        Arc::new(FixedProvider::new(query_vector)),
        store,
        LocalCodebaseContextConfig {
            max_relevant_fragments: limit,
            ..LocalCodebaseContextConfig::default()
        },
    );
    block_on(client.populate_merkle_tree_cache(
        EmbeddingConfig::default(),
        repo.root.clone(),
        RepoMetadata { path: None },
    ))
    .expect("index build should succeed");
    client
}

/// Runs one query against a freshly indexed synthetic repository and reports
/// what it cost, in leaf vectors read.
///
/// Asserts exactness on the way past, so no cost number can ever be reported for
/// a search that got the wrong answer.
fn measure_search(leaf_count: usize, seed: u64) -> usize {
    const LIMIT: usize = 10;
    let repo = synthetic_repo(leaf_count, 8, 16, 0.35, true, seed);
    // A query pointing at one particular fragment, so there is a real best
    // answer rather than a tie across the corpus.
    let query_vector = repo.leaves[leaf_count / 3].1.clone();
    let store = Arc::new(InMemoryVectorStore::default());
    let client = indexed_client(store.clone(), &repo, query_vector.clone(), LIMIT);

    store.reset_counters();
    let ranked = block_on(client.get_relevant_fragments(
        EmbeddingConfig::default(),
        "query".to_owned(),
        repo.root.clone(),
        RepoMetadata { path: None },
    ))
    .expect("search should succeed");

    assert_eq!(
        ranked,
        exhaustive_ranking(&repo.leaves, &query_vector, LIMIT),
        "the pruned search over {leaf_count} fragments must return exactly what \
         scoring every leaf returns"
    );
    assert!(
        store.summaries_read() < leaf_count,
        "reading the index must not itself become the scan it replaced: {} \
         summaries read for {leaf_count} fragments",
        store.summaries_read()
    );

    store.vectors_read()
}

#[test]
fn search_returns_exactly_what_the_exhaustive_scan_returns() {
    // The headline property. Pruning may only save work, never change the
    // answer -- so it is asserted at every corpus size the cost tests use, and
    // `measure_search` refuses to report a cost without it.
    for leaf_count in [64usize, 512, 4096] {
        let read = measure_search(leaf_count, 7);
        assert!(
            read > 0,
            "a search over {leaf_count} leaves must read some vectors"
        );
    }
}

#[test]
fn search_reads_a_small_fraction_of_a_clustered_corpus() {
    // The regression this change exists for: the previous implementation read
    // every leaf on every query. The measured figure at the time of writing is
    // 88 of 4,096; the assertion is set an order of magnitude looser so that it
    // fails on a lost index rather than on a tie broken differently.
    let read = measure_search(4096, 11);
    assert!(
        read < 4096 / 4,
        "a query over 4096 clustered fragments read {read} vectors; the point of \
         the index is that it does not have to look at most of the corpus"
    );
}

#[test]
fn search_cost_grows_far_more_slowly_than_the_corpus() {
    // The complexity claim, stated as a measurement rather than as an assertion
    // about big-O: multiply the corpus by 16 and the work must not multiply by
    // anything close to 16. Measured at the time of writing: 88 vectors read at
    // 512 fragments, 72 at 8,192 -- flat, and slightly *cheaper* on the larger
    // corpus, because a bigger tree gives the bound more places to cut.
    //
    // The corpus grows the way a repository grows -- more directories of roughly
    // constant size, not one directory that gets bigger -- because that is the
    // shape the pruning bound exploits, and measuring a shape no repository has
    // would measure nothing. The shape that defeats it is measured separately,
    // below.
    let small = measure_search(512, 3);
    let large = measure_search(8192, 3);

    let corpus_growth = 8192.0 / 512.0;
    let cost_growth = large as f32 / small as f32;
    assert!(
        cost_growth < corpus_growth / 4.0,
        "the corpus grew {corpus_growth}x (512 -> 8192 fragments) but the query \
         read {small} -> {large} vectors, a {cost_growth}x growth. Linear is what \
         this index exists to stop being."
    );
    assert!(
        large < 8192 / 8,
        "a query over 8192 fragments read {large} vectors"
    );
}

#[test]
fn search_is_still_exact_when_the_corpus_has_no_locality_to_exploit() {
    // The honest half of the claim. If a repository's embeddings have no
    // hierarchical locality, node radii overlap, little prunes, and the search
    // approaches reading everything -- measured at 416 of 512 here, against 96
    // for the same size with locality. What must not happen, and is what this
    // asserts, is that it starts returning a different answer.
    let repo = synthetic_repo(512, 8, 16, 12.0, false, 5);
    let query_vector = repo.leaves[17].1.clone();
    let store = Arc::new(InMemoryVectorStore::default());
    let client = indexed_client(store.clone(), &repo, query_vector.clone(), 10);

    let ranked = block_on(client.get_relevant_fragments(
        EmbeddingConfig::default(),
        "query".to_owned(),
        repo.root.clone(),
        RepoMetadata { path: None },
    ))
    .expect("search should succeed");

    assert_eq!(
        ranked,
        exhaustive_ranking(&repo.leaves, &query_vector, 10),
        "a corpus that defeats the pruning bound must still get the right answer"
    );
}

#[test]
fn search_without_a_built_index_is_exact_and_leaves_one_behind() {
    // The first query after a sync that never got to build its index -- an
    // incremental re-index, or a sync whose writes have not drained. Rather than
    // opening every subtree on this query and every query after it, it builds the
    // index itself, uses what it just computed, and leaves it behind so the next
    // query is cheap. Both halves are asserted: the answer is the exhaustive
    // one, and the second query does not re-read the corpus.
    let repo = synthetic_repo(256, 8, 16, 0.35, true, 13);
    let query_vector = repo.leaves[9].1.clone();
    let store = Arc::new(InMemoryVectorStore::default());
    load_repo(&store, &repo);
    let client = LocalStoreClient::new(
        Arc::new(FixedProvider::new(query_vector.clone())),
        store.clone(),
        LocalCodebaseContextConfig {
            max_relevant_fragments: 10,
            ..LocalCodebaseContextConfig::default()
        },
    );

    assert_eq!(store.summary_count(), 0, "no index has been built yet");

    let ranked = block_on(client.get_relevant_fragments(
        EmbeddingConfig::default(),
        "query".to_owned(),
        repo.root.clone(),
        RepoMetadata { path: None },
    ))
    .expect("search should succeed");

    assert_eq!(
        ranked,
        exhaustive_ranking(&repo.leaves, &query_vector, 10),
        "an unindexed search must be exact, not merely fast"
    );
    assert!(
        store.summary_count() > 0,
        "a search that had to build the index must persist it"
    );

    store.reset_counters();
    let repeat = block_on(client.get_relevant_fragments(
        EmbeddingConfig::default(),
        "query".to_owned(),
        repo.root.clone(),
        RepoMetadata { path: None },
    ))
    .expect("search should succeed");
    assert_eq!(
        repeat, ranked,
        "the indexed answer must match the unindexed one"
    );
    assert!(
        store.vectors_read() < 256,
        "the second query must use the index the first built, but it read {} of \
         256 vectors",
        store.vectors_read()
    );
}

#[test]
fn index_build_reuses_the_summaries_it_already_has() {
    // A merkle node's hash covers its subtree, so a summary can never be stale --
    // only absent. That is what makes an incremental re-index cheap: a second
    // build over an unchanged tree must not re-read a single leaf vector.
    let repo = synthetic_repo(512, 8, 16, 0.35, true, 17);
    let store = Arc::new(InMemoryVectorStore::default());
    let client = indexed_client(store.clone(), &repo, vec![1.0; 16], 10);
    let after_first = store.summary_count();
    assert!(after_first > 0, "the first build must produce summaries");

    store.reset_counters();
    block_on(client.populate_merkle_tree_cache(
        EmbeddingConfig::default(),
        repo.root.clone(),
        RepoMetadata { path: None },
    ))
    .expect("a second build should succeed");

    assert_eq!(
        store.vectors_read(),
        0,
        "the root already has a summary, so nothing beneath it needs recomputing"
    );
    assert_eq!(
        store.children_queries(),
        0,
        "and the tree must not be walked at all"
    );
    assert_eq!(store.summary_count(), after_first);
}

#[test]
fn index_build_skips_a_subtree_that_is_not_fully_embedded() {
    // A summary computed from half a subtree would claim a radius that excludes
    // the other half, and the search would then prune leaves it should have
    // scored. Recording nothing is the only honest answer: the search opens the
    // subtree instead, which is slow and right.
    let space = EmbeddingConfig::default().storage_key();
    let store = Arc::new(InMemoryVectorStore::default());

    let embedded = ContentHash::from_content("embedded");
    let pending = ContentHash::from_content("pending");
    let parent = node("parent");
    let root = node("root");

    store
        .record_nodes(
            space,
            &[
                IntermediateNode {
                    hash: root.clone(),
                    children: vec![parent.clone()],
                },
                IntermediateNode {
                    hash: parent,
                    children: vec![
                        NodeHash::from(embedded.clone()),
                        NodeHash::from(pending),
                    ],
                },
            ],
        )
        .unwrap();
    // Only one of the two leaves has landed.
    store
        .record_embeddings(space, &[(embedded, vec![1.0, 0.0])])
        .unwrap();

    let client = client(Arc::new(ScriptedProvider::new(&[])), store.clone());
    block_on(client.populate_merkle_tree_cache(
        EmbeddingConfig::default(),
        root,
        RepoMetadata { path: None },
    ))
    .expect("a partial index is not a failure");

    assert_eq!(
        store.summary_count(),
        0,
        "neither the half-embedded parent nor the root above it may be summarized"
    );
}

#[test]
fn a_search_scoped_to_one_root_ignores_another_root_that_has_an_index() {
    // Structural scoping has to survive the index: summaries are keyed by node
    // hash across the whole store, so a bug here would let one repository's
    // fragments answer another repository's query.
    let space = EmbeddingConfig::default().storage_key();
    let store = Arc::new(InMemoryVectorStore::default());

    let mine = ContentHash::from_content("mine");
    let theirs = ContentHash::from_content("theirs");
    let root = node("root");
    let other = node("other");

    store
        .record_nodes(
            space,
            &[
                IntermediateNode {
                    hash: root.clone(),
                    children: vec![NodeHash::from(mine.clone())],
                },
                IntermediateNode {
                    hash: other.clone(),
                    children: vec![NodeHash::from(theirs.clone())],
                },
            ],
        )
        .unwrap();
    store
        .record_embeddings(
            space,
            &[(mine.clone(), vec![0.0, 1.0]), (theirs, vec![1.0, 0.0])],
        )
        .unwrap();

    let client = LocalStoreClient::new(
        Arc::new(FixedProvider::new(vec![1.0, 0.0])),
        store,
        LocalCodebaseContextConfig::default(),
    );
    for hash in [root.clone(), other] {
        block_on(client.populate_merkle_tree_cache(
            EmbeddingConfig::default(),
            hash,
            RepoMetadata { path: None },
        ))
        .expect("index build should succeed");
    }

    let ranked = block_on(client.get_relevant_fragments(
        EmbeddingConfig::default(),
        "query".to_owned(),
        root,
        RepoMetadata { path: None },
    ))
    .expect("search should succeed");

    assert_eq!(
        ranked,
        vec![mine],
        "the better-scoring fragment under the other root must not leak in"
    );
}

// ---------------------------------------------------------------------------
// Ranking: the fusion, and evidence that it beats what it replaced.
// ---------------------------------------------------------------------------

#[test]
fn competition_ranks_share_a_rank_between_ties() {
    assert_eq!(competition_ranks(&[3.0, 1.0, 2.0]), vec![1, 3, 2]);
    assert_eq!(competition_ranks(&[1.0, 1.0, 0.0]), vec![1, 1, 3]);
    assert_eq!(
        competition_ranks(&[0.0, 0.0, 0.0]),
        vec![1, 1, 1],
        "a ranker with no opinion must not smuggle an arbitrary order into the fusion"
    );
    assert_eq!(competition_ranks(&[]), Vec::<usize>::new());
}

#[test]
fn fusion_with_an_opinionless_ranker_preserves_the_other_ones_order() {
    // The property that lets BM25 join the pipeline unconditionally: when the
    // query shares no term with any fragment, BM25 scores everything zero and
    // the fused order must be exactly the vector order.
    let vector_scores = [0.9f32, 0.1, 0.5];
    let silent = [0.0f32, 0.0, 0.0];
    let fused = reciprocal_rank_fusion(&[&vector_scores, &silent]);

    assert!(fused[0] > fused[2] && fused[2] > fused[1], "got {fused:?}");
}

#[test]
fn fusion_lets_a_lexical_match_overtake_a_marginally_better_vector_score() {
    // The failure mode this fixes, in miniature: the embedding model's favourite
    // (index 0) is topically plausible but shares no term with the query, while
    // the fragment that actually contains the symbol (index 2) is a hair behind
    // on cosine.
    let vector_scores = [0.85f32, 0.80, 0.79, 0.60, 0.55];
    let lexical_scores = [0.0f32, 0.0, 9.0, 2.0, 1.0];
    let fused = reciprocal_rank_fusion(&[&vector_scores, &lexical_scores]);

    let best = fused
        .iter()
        .enumerate()
        .max_by(|(_, left), (_, right)| left.total_cmp(right))
        .map(|(index, _)| index);
    assert_eq!(
        best,
        Some(2),
        "the lexically unambiguous fragment must reach the top; got {fused:?}"
    );
}

#[test]
fn fusion_does_not_let_one_ranker_alone_dictate_the_top_result() {
    // The other side of the same coin, and the reason the fallback is a fusion
    // rather than "use BM25 when it has an opinion": a fragment that only one
    // ranker likes must not beat one that both rank near the top. Without this,
    // a single incidental token match would drag an irrelevant fragment to the
    // front of every result list.
    let vector_scores = [0.90f32, 0.20, 0.85];
    let lexical_scores = [4.0f32, 9.0, 3.0];
    let fused = reciprocal_rank_fusion(&[&vector_scores, &lexical_scores]);

    assert!(
        fused[0] > fused[1],
        "index 1 is the lexical favourite but the vector ranker's last choice; \
         it must not win outright. got {fused:?}"
    );
}

#[test]
fn rerank_uses_the_provider_reranker_when_one_is_configured() {
    block_on(async {
        // A cross-encoder's ordering wins outright -- and, unlike the fallback,
        // costs no embedding work at all.
        let reranker = Arc::new(ScriptedReranker::new(&[("alpha", 0.9), ("beta", 0.1)]));
        let provider = Arc::new(ScriptedProvider::new(&[]));
        let client = LocalStoreClient::new(
            provider.clone(),
            Arc::new(InMemoryVectorStore::default()),
            LocalCodebaseContextConfig::default(),
        )
        .with_rerank_provider(Some(reranker.clone()));

        let reranked = client
            .rerank_fragments(
                "query".to_owned(),
                vec![fragment("beta"), fragment("alpha")],
            )
            .await
            .expect("rerank should succeed");

        assert_eq!(
            reranked
                .iter()
                .map(|fragment| fragment.content().to_owned())
                .collect::<Vec<_>>(),
            vec!["alpha".to_owned(), "beta".to_owned()]
        );
        assert_eq!(reranker.call_count(), 1);
        assert_eq!(
            provider.call_count(),
            0,
            "a cross-encoder needs no embeddings, so a rerank must not spend any"
        );
    });
}

#[test]
fn rerank_falls_back_to_hybrid_when_the_reranker_fails() {
    block_on(async {
        // A reranker that is down costs the search its ordering, never its
        // results.
        let space = EmbeddingConfig::default().storage_key();
        let store = Arc::new(InMemoryVectorStore::default());
        store
            .record_embeddings(
                space,
                &[
                    (ContentHash::from_content("alpha"), vec![0.0, 1.0]),
                    (ContentHash::from_content("beta"), vec![1.0, 0.0]),
                ],
            )
            .unwrap();

        let reranker = Arc::new(ScriptedReranker::failing());
        let provider = Arc::new(ScriptedProvider::new(&[("query", &[1.0, 0.0])]));
        let client = client(provider, store).with_rerank_provider(Some(reranker.clone()));

        let reranked = client
            .rerank_fragments(
                "query".to_owned(),
                vec![fragment("alpha"), fragment("beta")],
            )
            .await
            .expect("a failed reranker must not fail the search");

        assert_eq!(reranker.call_count(), 1);
        assert_eq!(
            reranked
                .iter()
                .map(|fragment| fragment.content().to_owned())
                .collect::<Vec<_>>(),
            vec!["beta".to_owned(), "alpha".to_owned()],
            "the hybrid path must have produced the ordering"
        );
    });
}

/// A code-search fixture: six fragments from a plausible settings/provider
/// module, each with the embedding an imagined model produced for it.
///
/// The embeddings are one-hot, which is not a model's behaviour but is what
/// makes the experiment legible: with fragment `j` at basis vector `e_j`, the
/// cosine of any query against fragment `j` is just that query's `j`-th
/// component. So [`RANKING_QUERIES`] states the model's opinion of every
/// fragment *directly*, and there is no arithmetic between the fixture and the
/// thing being measured.
///
/// What is being modelled is the documented bi-encoder failure on code: it gets
/// the topic right and the specific answer wrong, ranking the fragment that
/// actually answers the query second behind one that is merely about the same
/// subject. **That is a fixture, not a measurement of any real model.** What
/// this test establishes is that fusion repairs that failure mode, not that any
/// particular model exhibits it to any particular degree.
const RANKING_FIXTURE: &[(&str, &str, &[f32])] = &[
    (
        "resolve_embedding_endpoint",
        "pub fn resolve_embedding_endpoint(app: &AppContext) -> Option<EmbeddingEndpoint> { \
         let provider = usable_provider(app)?; Some(provider.endpoint()) }",
        &[1.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    ),
    (
        "chat_completion_url",
        "pub fn chat_completion_url(base: &str) -> String { \
         format!(\"{base}/chat/completions\") }",
        &[0.0, 1.0, 0.0, 0.0, 0.0, 0.0],
    ),
    (
        "merge_settings_layers",
        "pub fn merge_settings_layers(base: Settings, overlay: Settings) -> Settings { \
         base.merge(overlay) }",
        &[0.0, 0.0, 1.0, 0.0, 0.0, 0.0],
    ),
    (
        "watch_settings_file",
        "pub fn watch_settings_file(path: &Path, tx: Sender<Settings>) -> Result<Watcher> { \
         Watcher::new(path, move |_| tx.send(reload(path)?)) }",
        &[0.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    ),
    (
        "load_json_settings",
        "pub fn load_json_settings(path: &Path) -> Result<Settings> { \
         let text = fs::read_to_string(path)?; serde_json::from_str(&text) }",
        &[0.0, 0.0, 0.0, 0.0, 1.0, 0.0],
    ),
    (
        "default_settings",
        "pub fn default_settings() -> Settings { \
         Settings { theme: Theme::Dark, ..Default::default() } }",
        &[0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
    ),
];

/// Queries against [`RANKING_FIXTURE`]: the query text, the one fragment that
/// answers it, and the imagined model's embedding of the query — which, given
/// one-hot fragments, is exactly its cosine against each fragment in fixture
/// order.
///
/// Deliberately mixed. The first three are the bi-encoder failure: the model's
/// favourite is a fragment that shares no word with the query at all, and the
/// right answer is a close second. The fourth is the opposite case — the model
/// gets it right from a paraphrase that BM25 is nearly blind to — and it is here
/// so that "just use BM25 and delete the vectors" is measured rather than
/// assumed away. The test scores lexical-only alongside the other two for the
/// same reason.
const RANKING_QUERIES: &[(&str, &str, &[f32])] = &[
    (
        "resolve the embedding endpoint from settings",
        "resolve_embedding_endpoint",
        &[0.78, 0.85, 0.40, 0.38, 0.36, 0.34],
    ),
    (
        "merge two settings layers",
        "merge_settings_layers",
        &[0.30, 0.85, 0.80, 0.40, 0.35, 0.60],
    ),
    (
        "watch the settings file for changes on disk",
        "watch_settings_file",
        &[0.88, 0.30, 0.50, 0.84, 0.45, 0.40],
    ),
    (
        "how is a preference file turned into a struct",
        "load_json_settings",
        &[0.70, 0.20, 0.60, 0.50, 0.90, 0.40],
    ),
];

/// Mean reciprocal rank over the fixture's queries: `1 / position` of the right
/// answer, averaged. Chosen because there is exactly one right answer per query,
/// which is the case MRR is for.
fn mean_reciprocal_rank(orderings: &[(&str, Vec<String>)]) -> f32 {
    let total: f32 = orderings
        .iter()
        .map(|(answer, ordering)| {
            ordering
                .iter()
                .position(|name| name == answer)
                .map(|index| 1.0 / (index + 1) as f32)
                .unwrap_or(0.0)
        })
        .sum();
    total / orderings.len() as f32
}

fn fixture_fragments() -> Vec<Fragment> {
    RANKING_FIXTURE
        .iter()
        .map(|(name, body, _)| {
            Fragment::from_byte_range(
                (*body).to_owned(),
                ContentHash::from_content(body),
                PathBuf::from(format!("/repo/src/{name}.rs")),
                ByteOffset::from(0)..ByteOffset::from(body.len()),
            )
        })
        .collect()
}

fn fixture_name_for(fragment: &Fragment) -> String {
    RANKING_FIXTURE
        .iter()
        .find(|(_, body, _)| *body == fragment.content())
        .map(|(name, _, _)| (*name).to_owned())
        .expect("every fragment came from the fixture")
}

/// The provider for the fixture: fragment bodies and query texts both map to the
/// vectors the fixture declares.
fn fixture_provider() -> ScriptedProvider {
    let mut entries: Vec<(&str, &[f32])> = RANKING_FIXTURE
        .iter()
        .map(|(_, body, vector)| (*body, *vector))
        .collect();
    entries.extend(
        RANKING_QUERIES
            .iter()
            .map(|(query, _, vector)| (*query, *vector)),
    );
    ScriptedProvider::new(&entries)
}

/// Sorts `(name, score)` pairs the way the client does — score descending, ties
/// broken by a stable key — so that the comparison is not measuring sort
/// instability.
fn ranked_names(mut scored: Vec<(String, f32)>) -> Vec<String> {
    scored.sort_by(|(left_name, left), (right_name, right)| {
        right
            .partial_cmp(left)
            .unwrap_or(Ordering::Equal)
            .then_with(|| left_name.cmp(right_name))
    });
    scored.into_iter().map(|(name, _)| name).collect()
}

#[test]
fn ranking_quality_hybrid_beats_the_bi_encoder_it_replaces() {
    block_on(async {
        // The measurement behind the claim that the fusion is an improvement.
        // Three rankers over one fixture: the bi-encoder alone (what the first
        // BYOP implementation shipped), BM25 alone, and the fusion of the two
        // (what ships now).
        let client = LocalStoreClient::new(
            Arc::new(fixture_provider()),
            Arc::new(InMemoryVectorStore::default()),
            LocalCodebaseContextConfig::default(),
        );

        let mut hybrid = Vec::new();
        let mut bi_encoder = Vec::new();
        let mut lexical_only = Vec::new();

        for (query, answer, query_vector) in RANKING_QUERIES {
            let fragments = fixture_fragments();

            let reranked = client
                .rerank_fragments((*query).to_owned(), fragments.clone())
                .await
                .expect("rerank should succeed");
            hybrid.push((
                *answer,
                reranked.iter().map(fixture_name_for).collect::<Vec<_>>(),
            ));

            bi_encoder.push((
                *answer,
                ranked_names(
                    RANKING_FIXTURE
                        .iter()
                        .map(|(name, _, vector)| {
                            ((*name).to_owned(), cosine_similarity(query_vector, vector))
                        })
                        .collect(),
                ),
            ));

            let documents: Vec<&str> = fragments.iter().map(Fragment::content).collect();
            let scores = lexical::bm25_scores(query, &documents);
            lexical_only.push((
                *answer,
                ranked_names(fragments.iter().map(fixture_name_for).zip(scores).collect()),
            ));
        }

        let hybrid_mrr = mean_reciprocal_rank(&hybrid);
        let bi_encoder_mrr = mean_reciprocal_rank(&bi_encoder);
        let lexical_mrr = mean_reciprocal_rank(&lexical_only);

        assert!(
            hybrid_mrr > bi_encoder_mrr,
            "the fusion exists to beat the bi-encoder, but MRR went \
             {bi_encoder_mrr} -> {hybrid_mrr}"
        );
        assert!(
            hybrid_mrr >= lexical_mrr,
            "the fusion must not be worse than dropping the vectors entirely \
             (hybrid {hybrid_mrr} vs lexical-only {lexical_mrr}); if it were, the \
             cheaper answer would be to delete the embedding half of the reranker"
        );
        assert!(
            hybrid_mrr > 0.99,
            "on this fixture the fusion should put every answer first; got {hybrid_mrr}"
        );
    });
}
