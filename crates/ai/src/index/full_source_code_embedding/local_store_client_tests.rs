//! Tests for [`LocalStoreClient`].
//!
//! These are new, not ported: there is no pin equivalent, because at the pin
//! every one of these behaviours lived on the server. They cover the contracts
//! the rest of the subsystem relies on — in particular that `sync_merkle_tree`
//! returns the *un*known hashes, that retrieval is scoped by reachability from
//! the root, and that a missing provider is reported rather than swallowed.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use futures::executor::block_on;
use string_offset::ByteOffset;

use super::*;

/// An in-memory [`VectorStore`], so the client's logic can be tested without a
/// database.
#[derive(Default)]
struct InMemoryVectorStore {
    nodes: Mutex<HashMap<String, HashMap<String, Vec<String>>>>,
    vectors: Mutex<HashMap<String, HashMap<String, Vec<f32>>>>,
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

    fn leaves_under(&self, space: &str, root: &NodeHash) -> anyhow::Result<Vec<ContentHash>> {
        let nodes = self.nodes.lock().unwrap();
        let vectors = self.vectors.lock().unwrap();
        let Some(space_nodes) = nodes.get(space) else {
            return Ok(Vec::new());
        };
        let space_vectors = vectors.get(space);

        let mut seen: HashSet<String> = HashSet::new();
        let mut queue = vec![root.to_string()];
        let mut leaves = Vec::new();

        while let Some(current) = queue.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            match space_nodes.get(&current) {
                Some(children) => queue.extend(children.iter().cloned()),
                None => {
                    // Not an intermediate node: it is a leaf if we hold a vector
                    // for it. Anything else is a subtree we have not embedded.
                    if space_vectors.is_some_and(|vectors| vectors.contains_key(&current))
                        && let Ok(hash) = ContentHash::from_str(&current)
                    {
                        leaves.push(hash);
                    }
                }
            }
        }

        Ok(leaves)
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
        Ok(hashes
            .iter()
            .filter_map(|hash| {
                space_vectors
                    .get(&hash.to_string())
                    .map(|vector| (hash.clone(), vector.clone()))
            })
            .collect())
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
fn populate_merkle_tree_cache_succeeds_because_there_is_no_remote_cache() {
    block_on(async {
        let client = client(
            Arc::new(ScriptedProvider::new(&[])),
            Arc::new(InMemoryVectorStore::default()),
        );

        assert!(
            client
                .populate_merkle_tree_cache(
                    EmbeddingConfig::default(),
                    node("root"),
                    RepoMetadata { path: None },
                )
                .await
                .expect("no remote cache means nothing to fail"),
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
