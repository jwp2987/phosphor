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

use std::path::Path;

use anyhow::Context as _;

pub use crate::index::types::{
    ChunkEntry, ManifestEntry, Scan, ScanDiff, chunk_file, diff, looks_binary, root_hash,
    sha256_hex,
};
use crate::shellcmd::chunk::MAX_FILE_BYTES;

/// Ignore files the app honours besides `.gitignore` (`SUPPORTED_IGNORES`).
const EXTRA_IGNORES: [&str; 4] = [
    ".warpindexingignore",
    ".cursorignore",
    ".cursorindexingignore",
    ".codeiumignore",
];

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
