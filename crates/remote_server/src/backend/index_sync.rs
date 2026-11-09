//! The codebase-index driver, written once for both arms: it talks only to [`RemoteFeatures`], so
//! the helper, the shell arm and the router all index the same way (Wave H parity, P9b).
//!
//! ```text
//! sync:   scan -> (window of hashes) get_chunks -> embed in batches -> store_vectors
//! search: embed the query -> query_vectors -> absolute paths
//! ```
//!
//! The text goes to an [`Embedder`], which the app implements over `crates/network_gate` with the
//! user's own `/embeddings` key. This module has no network code and never sees the key; the host
//! (helper or the shell arm's app-side store) only ever receives vectors. Incremental by hash: the
//! host answers `get_chunks` with text only for fragments that have no vector under the
//! embedder's storage key, so a second sync of an unchanged tree embeds nothing, an edit embeds
//! the changed fragments, and a changed storage key (another model) re-embeds everything.
//!
//! There is no timer here. The caller decides when to sync: on the first need, after the app's
//! own write, after a prompt that follows a HEAD-changing git command, or on a user action.

use futures::future::BoxFuture;

use super::{BackendError, RemoteFeatures};
use crate::proto::{
    CodebaseVector, GetChunks, QueryVectors, StoreVectors, get_chunks_response,
    query_vectors_response, scan_codebase_response, store_vectors_response,
};

/// Hashes asked of `get_chunks` at a time: a window's text is a few MB at most.
pub const FETCH_WINDOW: usize = 256;
/// Rescans after a fragment's file changed between the scan and the fetch.
const MAX_RESCANS: usize = 2;

/// Turns text into vectors. Implemented by the app over the network gate.
pub trait Embedder: Send + Sync {
    /// The embedding space the vectors live in (model and dimension, e.g. `voyage:code-3:512`).
    /// `None`: no provider is configured.
    fn storage_key(&self) -> Option<String>;

    /// How many texts to send per request.
    fn max_batch(&self) -> usize {
        64
    }

    /// One vector per input text, in order.
    fn embed(&self, texts: Vec<String>) -> BoxFuture<'_, Result<Vec<Vec<f32>>, String>>;
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("no embedding provider is configured")]
    NoProvider,
    /// The host or the backend failed (unreachable, unsupported, the scan was refused).
    #[error("{0}")]
    Host(String),
    #[error(transparent)]
    Backend(#[from] BackendError),
    #[error("embedding failed: {0}")]
    Embed(String),
    /// Search before the repository has been synced under this embedding model.
    #[error("the repository has not been indexed yet: {0}")]
    NotIndexed(String),
}

/// What a sync has done so far, for status display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncProgress {
    Scanning,
    /// `completed` of `total` fragments have a vector.
    Embedding {
        completed: usize,
        total: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyncOutcome {
    pub root_hash: String,
    pub files: usize,
    pub fragments: usize,
    /// Fragments embedded by this sync.
    pub embedded: usize,
    /// Fragments that already had a vector.
    pub already: usize,
    /// Fragments whose file kept changing under the sync and that were left for the next one.
    pub left_stale: usize,
}

/// Indexes `repo` on the host `backend` serves. Dropping the future cancels it between requests;
/// what was already stored stays (the next sync resumes from it).
pub async fn sync_repo(
    backend: &dyn RemoteFeatures,
    embedder: &dyn Embedder,
    repo: &str,
    mut progress: impl FnMut(SyncProgress) + Send,
) -> Result<SyncOutcome, SyncError> {
    let storage_key = embedder.storage_key().ok_or(SyncError::NoProvider)?;
    let mut embedded_total = 0usize;
    let mut rescans = 0usize;
    loop {
        progress(SyncProgress::Scanning);
        let scan = match backend.scan_codebase(repo.to_string()).await?.result {
            Some(scan_codebase_response::Result::Success(s)) => s,
            Some(scan_codebase_response::Result::Error(e)) => {
                return Err(SyncError::Host(e.message));
            }
            None => return Err(SyncError::Host("empty ScanCodebase response".into())),
        };
        let hashes: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            scan.chunks
                .iter()
                .filter(|c| seen.insert(c.content_hash.as_str()))
                .map(|c| c.content_hash.clone())
                .collect()
        };
        let total = hashes.len();
        let mut already = 0usize;
        let mut embedded = 0usize;
        let mut missing = 0usize;
        progress(SyncProgress::Embedding {
            completed: 0,
            total,
        });
        for window in hashes.chunks(FETCH_WINDOW) {
            let got = match backend
                .get_chunks(GetChunks {
                    repo_path: repo.to_string(),
                    storage_key: storage_key.clone(),
                    content_hashes: window.to_vec(),
                })
                .await?
                .result
            {
                Some(get_chunks_response::Result::Success(s)) => s,
                Some(get_chunks_response::Result::Error(e)) => {
                    return Err(SyncError::Host(e.message));
                }
                None => return Err(SyncError::Host("empty GetChunks response".into())),
            };
            already += got.already_embedded as usize;
            missing += got.missing_hashes.len();
            for batch in got.chunks.chunks(embedder.max_batch().max(1)) {
                let texts: Vec<String> = batch.iter().map(|c| c.text.clone()).collect();
                let vectors = embedder.embed(texts).await.map_err(SyncError::Embed)?;
                if vectors.len() != batch.len() {
                    return Err(SyncError::Embed(format!(
                        "the provider returned {} vectors for {} inputs",
                        vectors.len(),
                        batch.len()
                    )));
                }
                let stored = backend
                    .store_vectors(StoreVectors {
                        repo_path: repo.to_string(),
                        storage_key: storage_key.clone(),
                        vectors: batch
                            .iter()
                            .zip(vectors)
                            .map(|(c, values)| CodebaseVector {
                                content_hash: c.content_hash.clone(),
                                values,
                            })
                            .collect(),
                    })
                    .await?;
                match stored.result {
                    Some(store_vectors_response::Result::Success(_)) => {}
                    Some(store_vectors_response::Result::Error(e)) => {
                        return Err(SyncError::Host(e.message));
                    }
                    None => return Err(SyncError::Host("empty StoreVectors response".into())),
                }
                embedded += batch.len();
                progress(SyncProgress::Embedding {
                    completed: already + embedded,
                    total,
                });
            }
        }
        embedded_total += embedded;
        if missing > 0 && rescans < MAX_RESCANS {
            // A file changed between the scan and the fetch: look again.
            rescans += 1;
            continue;
        }
        return Ok(SyncOutcome {
            root_hash: scan.root_hash,
            files: scan.manifest.len(),
            fragments: total,
            embedded: embedded_total,
            already,
            left_stale: missing,
        });
    }
}

/// A fragment the search ranked.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Absolute path on the host.
    pub path: String,
    pub start_line: u32,
    pub end_line: u32,
    pub score: f32,
}

/// Joins a repository-relative path to the repository root with `/`.
pub fn absolute_path(repo: &str, relative: &str) -> String {
    format!("{}/{relative}", repo.trim_end_matches('/'))
}

/// The `top_k` best fragments for `query`, best first. The query is embedded here, so the host
/// only sees a vector.
pub async fn search_repo(
    backend: &dyn RemoteFeatures,
    embedder: &dyn Embedder,
    repo: &str,
    query: &str,
    top_k: u32,
) -> Result<Vec<Hit>, SyncError> {
    let storage_key = embedder.storage_key().ok_or(SyncError::NoProvider)?;
    let mut vectors = embedder
        .embed(vec![query.to_string()])
        .await
        .map_err(SyncError::Embed)?;
    let query_vector = match (vectors.pop(), vectors.is_empty()) {
        (Some(v), true) if !v.is_empty() => v,
        _ => {
            return Err(SyncError::Embed(
                "the provider returned no query vector".into(),
            ));
        }
    };
    let response = backend
        .query_vectors(QueryVectors {
            repo_path: repo.to_string(),
            storage_key,
            query_vector,
            top_k,
        })
        .await?;
    match response.result {
        Some(query_vectors_response::Result::Success(s)) => Ok(s
            .fragments
            .into_iter()
            .map(|f| Hit {
                path: absolute_path(repo, &f.path),
                start_line: f.start_line,
                end_line: f.end_line,
                score: f.score,
            })
            .collect()),
        Some(query_vectors_response::Result::Error(e)) => Err(SyncError::NotIndexed(e.message)),
        None => Err(SyncError::Host("empty QueryVectors response".into())),
    }
}

/// The distinct paths of `hits`, best first.
pub fn ranked_paths(hits: &[Hit]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    hits.iter()
        .filter(|h| seen.insert(h.path.as_str()))
        .map(|h| h.path.clone())
        .collect()
}

#[cfg(test)]
#[path = "index_sync_tests.rs"]
mod tests;
