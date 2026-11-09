//! The helper's side of codebase indexing (capability `codebase_chunks`, Wave H parity P9b):
//! `ScanCodebase`, `GetChunks`, `StoreVectors`, `QueryVectors` over a flat-file store on the host
//! (`~/.phosphor/index/<hash of repo path>/`, see [`crate::index::store`]).
//!
//! The host scans, chunks, hashes and keeps the vectors; it never embeds and never sees the
//! user's embedding key (the app embeds through its network gate). Search is a brute-force
//! cosine pass over the stored vectors.
//!
//! `PHOSPHOR_REMOTE_INDEX_DIR` overrides the store root (tests and the lab).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::chunk;
use crate::index::proto::{
    MAX_CHUNKS_PER_REQUEST, MAX_TOP_K, chunks_error, query_error, ranked_to_proto, scan_error,
    scan_to_proto, store_error,
};
use crate::index::store::IndexStore;
use crate::index::types::{ChunkEntry, sha256_hex};
use crate::proto::{
    CodebaseChunkText, GetChunks, GetChunksResponse, GetChunksSuccess, QueryVectors,
    QueryVectorsResponse, ScanCodebase, ScanCodebaseResponse, StoreVectors, StoreVectorsResponse,
    StoreVectorsSuccess, get_chunks_response, query_vectors_response, scan_codebase_response,
    store_vectors_response,
};

/// Where the helper keeps its indexes.
pub(super) fn default_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("PHOSPHOR_REMOTE_INDEX_DIR") {
        return PathBuf::from(dir);
    }
    dirs::home_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(".phosphor")
        .join("index")
}

/// The store of `repo`, keyed by its canonical path on this host.
fn store_for(root: &Path, repo: &Path) -> IndexStore {
    let canonical = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    IndexStore::open(root, &canonical.to_string_lossy())
}

fn repo_dir(repo_path: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(repo_path);
    if !path.is_absolute() {
        return Err(format!("{repo_path} is not an absolute path"));
    }
    if !path.is_dir() {
        return Err(format!("{repo_path} is not a directory"));
    }
    Ok(path)
}

pub(super) fn scan_codebase(root: &Path, msg: ScanCodebase) -> ScanCodebaseResponse {
    let repo = match repo_dir(&msg.repo_path) {
        Ok(repo) => repo,
        Err(e) => return scan_error(e),
    };
    let scan = match chunk::scan(&repo) {
        Ok(scan) => scan,
        Err(e) => return scan_error(format!("{e:#}")),
    };
    let store = store_for(root, &repo);
    if let Err(e) = store.save_scan(&msg.repo_path, &scan) {
        return scan_error(format!("{e:#}"));
    }
    let live: HashSet<String> = scan.chunks.iter().map(|c| c.content_hash.clone()).collect();
    if let Err(e) = store.compact_wasteful(&live) {
        // Housekeeping only: the scan itself succeeded.
        log::warn!("index compaction failed: {e:#}");
    }
    log::info!(
        "ScanCodebase: {} files, {} fragments",
        scan.manifest.len(),
        scan.chunks.len()
    );
    ScanCodebaseResponse {
        result: Some(scan_codebase_response::Result::Success(scan_to_proto(
            &scan,
        ))),
    }
}

pub(super) fn get_chunks(root: &Path, msg: GetChunks) -> GetChunksResponse {
    if msg.content_hashes.len() > MAX_CHUNKS_PER_REQUEST {
        return chunks_error(format!(
            "{} hashes in one request; at most {MAX_CHUNKS_PER_REQUEST}",
            msg.content_hashes.len()
        ));
    }
    let repo = match repo_dir(&msg.repo_path) {
        Ok(repo) => repo,
        Err(e) => return chunks_error(e),
    };
    let store = store_for(root, &repo);
    let Some(stored) = store.load_scan() else {
        return chunks_error("this repository has not been scanned");
    };
    let known = if msg.storage_key.is_empty() {
        HashSet::new()
    } else {
        match store.known_hashes(&msg.storage_key) {
            Ok(known) => known,
            Err(e) => return chunks_error(format!("{e:#}")),
        }
    };

    let mut by_hash: HashMap<&str, &ChunkEntry> = HashMap::new();
    for c in &stored.scan.chunks {
        by_hash.entry(c.content_hash.as_str()).or_insert(c);
    }
    let mut already = 0u64;
    let mut missing = Vec::new();
    // Wanted fragments grouped by file so each file is read once.
    let mut wanted: Vec<(&ChunkEntry, &str)> = Vec::new();
    let mut seen: HashSet<&str> = HashSet::new();
    for hash in &msg.content_hashes {
        if !seen.insert(hash.as_str()) {
            continue;
        }
        if known.contains(hash) {
            already += 1;
        } else if let Some(chunk) = by_hash.get(hash.as_str()) {
            wanted.push((chunk, hash));
        } else {
            missing.push(hash.clone());
        }
    }
    wanted.sort_by(|a, b| a.0.path.cmp(&b.0.path));

    let mut chunks = Vec::with_capacity(wanted.len());
    let mut current: Option<(&str, Option<Vec<u8>>)> = None;
    for (entry, hash) in wanted {
        if current.as_ref().map(|(p, _)| *p) != Some(entry.path.as_str()) {
            let bytes = std::fs::read(repo.join(&entry.path)).ok();
            current = Some((entry.path.as_str(), bytes));
        }
        let text = current
            .as_ref()
            .and_then(|(_, bytes)| bytes.as_deref())
            .and_then(|bytes| bytes.get(entry.byte_start..entry.byte_end))
            .filter(|slice| sha256_hex(slice) == *hash)
            .and_then(|slice| std::str::from_utf8(slice).ok());
        match text {
            Some(text) => chunks.push(CodebaseChunkText {
                content_hash: hash.to_string(),
                text: text.to_string(),
            }),
            // The file changed since the scan: the app rescans.
            None => missing.push(hash.to_string()),
        }
    }
    GetChunksResponse {
        result: Some(get_chunks_response::Result::Success(GetChunksSuccess {
            chunks,
            already_embedded: already,
            missing_hashes: missing,
        })),
    }
}

pub(super) fn store_vectors(root: &Path, msg: StoreVectors) -> StoreVectorsResponse {
    if msg.storage_key.is_empty() {
        return store_error("storage_key is empty");
    }
    let repo = match repo_dir(&msg.repo_path) {
        Ok(repo) => repo,
        Err(e) => return store_error(e),
    };
    let store = store_for(root, &repo);
    let vectors: Vec<(String, Vec<f32>)> = msg
        .vectors
        .into_iter()
        .map(|v| (v.content_hash, v.values))
        .collect();
    match store.append_vectors(&msg.storage_key, &vectors) {
        Ok(stored) => {
            // A changed embedding model makes every older space unusable.
            store.drop_other_spaces(&msg.storage_key);
            StoreVectorsResponse {
                result: Some(store_vectors_response::Result::Success(
                    StoreVectorsSuccess {
                        stored: stored as u64,
                    },
                )),
            }
        }
        Err(e) => store_error(format!("{e:#}")),
    }
}

pub(super) fn query_vectors(root: &Path, msg: QueryVectors) -> QueryVectorsResponse {
    let repo = match repo_dir(&msg.repo_path) {
        Ok(repo) => repo,
        Err(e) => return query_error(e),
    };
    if msg.query_vector.is_empty() {
        return query_error("the query vector is empty");
    }
    let top_k = msg.top_k.clamp(1, MAX_TOP_K) as usize;
    match store_for(root, &repo).query(&msg.storage_key, &msg.query_vector, top_k) {
        Ok(hits) => QueryVectorsResponse {
            result: Some(query_vectors_response::Result::Success(ranked_to_proto(
                hits,
            ))),
        },
        Err(e) => query_error(format!("{e:#}")),
    }
}

#[cfg(test)]
#[path = "index_store_tests.rs"]
mod tests;
