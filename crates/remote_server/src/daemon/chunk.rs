//! Codebase scan, chunk and hash for indexing under `crate::capability::CODEBASE_CHUNKS` (Wave H
//! parity, family package P9): `ScanCodebase`, `GetChunks`. The app embeds; the key never reaches
//! the host. P9a is the library and the `phosphor-remote index-scan` command; the proto requests
//! are added (additively) by P9b.
//!
//! Std, `ignore` and `sha2` only. The chunking rules are `crate::shellcmd::chunk`'s (the app's
//! naive chunker), shared with the no-helper path.
//!
//! What gets indexed, like the app: regular files below [`MAX_FILE_BYTES`] that are valid UTF-8,
//! under `.gitignore` plus the app's extra ignore files, `.git` never. Binary files (a NUL in the
//! first 8000 bytes, or not UTF-8) and files with no non-empty fragment are skipped. Global git
//! ignore files and ignore files in parent directories are NOT read, so the result depends on the
//! tree alone.
//!
//! Hashes: `file_hash` and `content_hash` are SHA-256 hex of the file's / fragment's bytes (the
//! app's `file_contents_hash` and fragment `MerkleHash`). `root_hash` is the SHA-256 of the sorted
//! `"<path>\0<file_hash>\n"` lines: it changes iff a path or a file's content does.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::Path;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::shellcmd::chunk::{MAX_FILE_BYTES, chunk_text};

/// Ignore files the app honours besides `.gitignore` (`SUPPORTED_IGNORES`).
const EXTRA_IGNORES: [&str; 4] = [
    ".warpindexingignore",
    ".cursorignore",
    ".cursorindexingignore",
    ".codeiumignore",
];

/// Bytes inspected for a NUL when deciding "binary".
const BINARY_SNIFF_BYTES: usize = 8000;

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

pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
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

fn root_hash(manifest: &[ManifestEntry]) -> String {
    let mut h = Sha256::new();
    for m in manifest {
        h.update(m.path.as_bytes());
        h.update([0u8]);
        h.update(m.file_hash.as_bytes());
        h.update(b"\n");
    }
    let mut out = String::with_capacity(64);
    for b in h.finalize() {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

/// Walks `repo` and returns its manifest, chunk list and root hash.
pub fn scan(repo: &Path) -> anyhow::Result<Scan> {
    anyhow::ensure!(repo.is_dir(), "{} is not a directory", repo.display());
    let mut builder = ignore::WalkBuilder::new(repo);
    builder
        .hidden(false)
        .parents(false)
        .git_global(false)
        .require_git(false)
        .follow_links(false)
        .filter_entry(|e| e.file_name() != ".git");
    for name in EXTRA_IGNORES {
        builder.add_custom_ignore_filename(name);
    }

    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(repo) else {
            continue;
        };
        let Some(rel) = rel.to_str() else { continue };
        files.push((
            rel.replace(std::path::MAIN_SEPARATOR, "/"),
            entry.path().to_path_buf(),
        ));
    }
    files.sort();

    let mut manifest = Vec::new();
    let mut chunks = Vec::new();
    for (rel, abs) in files {
        let Ok(meta) = std::fs::metadata(&abs) else {
            continue;
        };
        if meta.len() >= MAX_FILE_BYTES {
            continue;
        }
        let Ok(bytes) = std::fs::read(&abs) else {
            continue;
        };
        if looks_binary(&bytes) {
            continue;
        }
        let text = std::str::from_utf8(&bytes).expect("checked by looks_binary");
        let file_chunks = chunk_file(&rel, text);
        if file_chunks.is_empty() {
            continue;
        }
        manifest.push(ManifestEntry {
            path: rel,
            file_hash: sha256_hex(&bytes),
        });
        chunks.extend(file_chunks);
    }
    let root_hash = root_hash(&manifest);
    Ok(Scan {
        manifest,
        chunks,
        root_hash,
    })
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

/// `phosphor-remote index-scan <repo> [--known <file>]`: the JSON to print. Deterministic: the
/// same tree gives byte-identical output.
pub fn run_index_scan(repo: &Path, known: Option<&Path>) -> anyhow::Result<String> {
    let current = scan(repo)?;
    let mut json = match known {
        None => serde_json::to_string_pretty(&current)?,
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("reading {}", path.display()))?;
            let known: Scan = serde_json::from_str(&text)
                .with_context(|| format!("{} is not an index-scan output", path.display()))?;
            serde_json::to_string_pretty(&diff(&known, &current))?
        }
    };
    json.push('\n');
    Ok(json)
}

#[cfg(test)]
#[path = "chunk_tests.rs"]
mod tests;
