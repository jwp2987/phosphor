//! Codebase indexing over the shell (no-helper arm): the command that lists and hashes a
//! repository's files, the parser for its output, and the command that reads one file back.
//! Pure: no I/O. The helper walks its own tree instead (`daemon::chunk`); both end in the same
//! `index::types::Scan`.
//!
//! # What is listed
//!
//! Inside a git work tree: `git ls-files -z --cached --others --exclude-standard` (tracked files
//! plus untracked ones that are not ignored). Anywhere else: `find` pruning `.git`,
//! `node_modules`, `target`, `__pycache__` and `.venv` (there are no ignore files to read without
//! git). Only regular files under [`MAX_FILE_BYTES`] are hashed (`find -type f -size`), so a
//! symlink, a socket or a huge asset costs nothing. The ignore files the helper honours besides
//! `.gitignore` (`.cursorignore` and friends) are not applied here: a known difference, listed in
//! `docs/testing.md`.
//!
//! # Hashes
//!
//! `sha256sum` (GNU), else `shasum -a 256` (macOS, perl), else `sha256 -r` (BSD): the same
//! SHA-256 of the file's bytes the helper computes, so a file's `file_hash` is the same on both
//! arms. (`git ls-files -s` is not used: it hashes the index, so an edit not yet staged would go
//! unnoticed.)
//!
//! # Output
//!
//! `MODE:git` or `MODE:find`, then one `<64 hex><sep><path>` line per hashed file. A path the
//! hash tool had to escape (a newline or backslash in it, which GNU marks with a leading `\`) is
//! skipped. Exit codes: 3 the directory cannot be entered, 4 no SHA-256 tool.

use super::base64::Base64Tool;
use super::chunk::MAX_FILE_BYTES;
use super::sh_quote;

pub const EXIT_NO_DIRECTORY: i32 = 3;
pub const EXIT_NO_HASH_TOOL: i32 = 4;

/// The POSIX source of the list-and-hash section for the repository at `repo`.
pub fn scan_command(repo: &str) -> String {
    let repo = sh_quote(repo);
    let max = MAX_FILE_BYTES;
    format!(
        "cd {repo} || exit {EXIT_NO_DIRECTORY}\n\
         if command -v sha256sum >/dev/null 2>&1; then H='sha256sum'; \
         elif command -v shasum >/dev/null 2>&1; then H='shasum -a 256'; \
         elif command -v sha256 >/dev/null 2>&1; then H='sha256 -r'; \
         else exit {EXIT_NO_HASH_TOOL}; fi\n\
         if command -v git >/dev/null 2>&1 && git rev-parse --is-inside-work-tree >/dev/null 2>&1; \
         then M=git; else M=find; fi\n\
         echo \"MODE:$M\"\n\
         list_files() {{\n\
         if [ \"$M\" = git ]; then git ls-files -z --cached --others --exclude-standard 2>/dev/null; \
         else find . \\( -name .git -o -name node_modules -o -name target -o -name __pycache__ -o -name .venv \\) -prune -o -type f -print0 2>/dev/null; fi\n\
         }}\n\
         list_files | xargs -0 sh -c 'find \"$@\" -maxdepth 0 -type f -size -{max}c -print0' sh 2>/dev/null | xargs -0 $H 2>/dev/null\n\
         exit 0"
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListMode {
    Git,
    Find,
}

/// One file the scan hashed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HashedFile {
    /// Relative to the repository root, `/`-separated, no leading `./`.
    pub path: String,
    pub sha256: String,
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Reads the scan section's output. Sorted by path, one entry per path.
pub fn parse_scan_output(text: &str) -> (Option<ListMode>, Vec<HashedFile>) {
    let mut mode = None;
    let mut files = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(m) = line.strip_prefix("MODE:") {
            mode = match m {
                "git" => Some(ListMode::Git),
                "find" => Some(ListMode::Find),
                _ => mode,
            };
            continue;
        }
        // GNU escapes odd names with a leading backslash; those lines cannot be trusted.
        if line.starts_with('\\') || line.len() < 66 {
            continue;
        }
        let (hash, rest) = line.split_at(64);
        if !is_sha256_hex(hash) {
            continue;
        }
        // `hash  path` (text mode), `hash *path` (binary mode) or `hash path` (BSD -r).
        let path = rest
            .strip_prefix("  ")
            .or_else(|| rest.strip_prefix(" *"))
            .or_else(|| rest.strip_prefix(' '));
        let Some(path) = path else { continue };
        let path = path.strip_prefix("./").unwrap_or(path);
        // `-` is what xargs with no input makes the hash tool print for stdin.
        if path.is_empty() || path == "-" || path.contains('\u{fffd}') {
            continue;
        }
        files.push(HashedFile {
            path: path.to_string(),
            sha256: hash.to_ascii_lowercase(),
        });
    }
    files.sort_by(|a, b| a.path.cmp(&b.path));
    files.dedup_by(|a, b| a.path == b.path);
    (mode, files)
}

/// The source of one file's section: the file's first [`MAX_FILE_BYTES`] bytes, encoded by
/// `tool`. A file that grew since the listing is cut, and its hash then differs from the
/// listing's, which the caller notices.
pub fn read_file_command(tool: Base64Tool, repo: &str, rel: &str) -> String {
    format!(
        "cd {} && head -c {MAX_FILE_BYTES} < {} | {}",
        sh_quote(repo),
        sh_quote(rel),
        tool.encoder_filter()
    )
}

#[cfg(test)]
#[path = "index_tests.rs"]
mod tests;
