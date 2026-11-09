//! The data a codebase scan produces and the pure functions over it: hashes, per-file chunking,
//! the root hash and the diff between two scans. Shared by the helper (`daemon::chunk` walks the
//! host's tree) and the no-helper path (`backend::shell::index` gets the same facts by running
//! commands), so both arms describe a tree identically.
//!
//! Hashes: `file_hash` and `content_hash` are SHA-256 hex of the file's / fragment's bytes (the
//! app's `file_contents_hash` and fragment `MerkleHash`). `root_hash` is the SHA-256 of the sorted
//! `"<path>\0<file_hash>\n"` lines: it changes iff a path or a file's content does.

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::shellcmd::chunk::chunk_text;

/// Bytes inspected for a NUL when deciding "binary".
pub const BINARY_SNIFF_BYTES: usize = 8000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestEntry {
    pub path: String,
    pub file_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkEntry {
    pub content_hash: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub byte_start: usize,
    pub byte_end: usize,
}

/// The full result of a scan. Entries are sorted (manifest by path, chunks by path then position).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scan {
    pub manifest: Vec<ManifestEntry>,
    pub chunks: Vec<ChunkEntry>,
    pub root_hash: String,
}

/// What changed since a previous [`Scan`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanDiff {
    pub root_hash: String,
    pub unchanged: bool,
    pub added_files: Vec<String>,
    pub changed_files: Vec<String>,
    pub removed_files: Vec<String>,
    /// Content hashes in the new scan that the old one did not have (sorted, unique): the only
    /// fragments that need text and an embedding.
    pub new_content_hashes: Vec<String>,
}

pub fn hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

/// True when `bytes` should not be indexed as text.
pub fn looks_binary(bytes: &[u8]) -> bool {
    bytes[..bytes.len().min(BINARY_SNIFF_BYTES)].contains(&0) || std::str::from_utf8(bytes).is_err()
}

/// Chunks and hashes one file's text. Empty fragments are dropped, as the app's tree does.
pub fn chunk_file(rel_path: &str, text: &str) -> Vec<ChunkEntry> {
    chunk_text(text)
        .into_iter()
        .filter(|s| s.byte_end > s.byte_start)
        .map(|s| ChunkEntry {
            content_hash: sha256_hex(text[s.byte_range()].as_bytes()),
            path: rel_path.to_string(),
            start_line: s.start_line,
            end_line: s.end_line,
            byte_start: s.byte_start,
            byte_end: s.byte_end,
        })
        .collect()
}

/// The root hash of a manifest sorted by path.
pub fn root_hash(manifest: &[ManifestEntry]) -> String {
    let mut h = Sha256::new();
    for m in manifest {
        h.update(m.path.as_bytes());
        h.update([0u8]);
        h.update(m.file_hash.as_bytes());
        h.update(b"\n");
    }
    hex(&h.finalize())
}

/// What changed in `new` compared with `known`.
pub fn diff(known: &Scan, new: &Scan) -> ScanDiff {
    let old_files: HashMap<&str, &str> = known
        .manifest
        .iter()
        .map(|m| (m.path.as_str(), m.file_hash.as_str()))
        .collect();
    let new_paths: HashSet<&str> = new.manifest.iter().map(|m| m.path.as_str()).collect();
    let mut added = Vec::new();
    let mut changed = Vec::new();
    for m in &new.manifest {
        match old_files.get(m.path.as_str()) {
            None => added.push(m.path.clone()),
            Some(h) if *h != m.file_hash => changed.push(m.path.clone()),
            Some(_) => {}
        }
    }
    let mut removed: Vec<String> = known
        .manifest
        .iter()
        .filter(|m| !new_paths.contains(m.path.as_str()))
        .map(|m| m.path.clone())
        .collect();
    removed.sort();
    let old_hashes: HashSet<&str> = known
        .chunks
        .iter()
        .map(|c| c.content_hash.as_str())
        .collect();
    let new_hashes: BTreeSet<String> = new
        .chunks
        .iter()
        .filter(|c| !old_hashes.contains(c.content_hash.as_str()))
        .map(|c| c.content_hash.clone())
        .collect();
    ScanDiff {
        root_hash: new.root_hash.clone(),
        unchanged: known.root_hash == new.root_hash,
        added_files: added,
        changed_files: changed,
        removed_files: removed,
        new_content_hashes: new_hashes.into_iter().collect(),
    }
}
