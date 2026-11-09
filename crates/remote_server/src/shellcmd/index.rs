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
//! `MODE:git` or `MODE:find` (and, for page 0, `TOTAL:<files listed>`), then one `<64 hex><sep><path>` line per hashed file. A path the
//! hash tool had to escape (a newline or backslash in it, which GNU marks with a leading `\`) is
//! skipped. Exit codes: 3 the directory cannot be entered, 4 no SHA-256 tool.
//!
//! # Pages
//!
//! A repository is listed and hashed [`SCAN_PAGE_FILES`] files at a time ([`scan_page_command`]),
//! so a 100 000-file repository neither hits the 10 MiB output cap nor the command timeout.

use super::base64::Base64Tool;
use super::chunk::MAX_FILE_BYTES;
use super::sh_quote;

pub const EXIT_NO_DIRECTORY: i32 = 3;
pub const EXIT_NO_HASH_TOOL: i32 = 4;

/// Files one scan page lists and hashes. A page's output is about this many `<hash> <path>`
/// lines (a few hundred KiB), far under the output cap, and its hashing far under the command
/// timeout, however large the repository is (#1871).
pub const SCAN_PAGE_FILES: usize = 2000;

/// The POSIX source of one page of the list-and-hash section for the repository at `repo`: files
/// `page * page_size + 1 ..= (page + 1) * page_size` of the listing (git's order, or `find`'s).
/// With `with_total` it also prints `TOTAL:<n>`, the listing's length, which tells the caller how
/// many pages there are. Every page re-lists the repository (cheap: one `git ls-files`), then
/// hashes only its own slice, so no page's output or time grows with the repository.
///
/// The listing is sliced by lines; a file name that contains a newline is therefore cut in two
/// and neither half is a file, so it is never hashed (the same fate as a name the hash tool
/// escapes).
pub fn scan_page_command(repo: &str, page: usize, page_size: usize, with_total: bool) -> String {
    let repo = sh_quote(repo);
    let max = MAX_FILE_BYTES;
    let first = page * page_size + 1;
    let last = (page + 1) * page_size;
    let total = if with_total {
        "echo \"TOTAL:$(list_files | tr '\\0' '\\n' | wc -l | tr -d ' \\t')\"\n"
    } else {
        ""
    };
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
         {total}\
         list_files | tr '\\0' '\\n' | sed -n '{first},{last}p;{last}q' | tr '\\n' '\\0' | xargs -0 sh -c 'find \"$@\" -maxdepth 0 -type f -size -{max}c -print0' sh 2>/dev/null | xargs -0 $H 2>/dev/null\n\
         exit 0"
    )
}

/// The PowerShell source of [`scan_page_command`]: the same output (`MODE:`, `TOTAL:`, then
/// `<sha256 hex>  <path>` lines with `/` separators), listed with `git ls-files` or a walk that
/// skips the same directories, hashed with `Get-FileHash`.
pub fn scan_page_command_pwsh(
    repo: &str,
    page: usize,
    page_size: usize,
    with_total: bool,
) -> String {
    let repo = repo.replace('\'', "''");
    let max = MAX_FILE_BYTES;
    let skip = page * page_size;
    let total = if with_total {
        "'TOTAL:' + $all.Count\n"
    } else {
        ""
    };
    format!(
        "Set-Location -LiteralPath '{repo}' -ErrorAction SilentlyContinue\n\
         if (-not $?) {{ exit {EXIT_NO_DIRECTORY} }}\n\
         $M = 'find'\n\
         if (Get-Command git -ErrorAction SilentlyContinue) {{ git rev-parse --is-inside-work-tree 2>$null | Out-Null; if ($LASTEXITCODE -eq 0) {{ $M = 'git' }} }}\n\
         'MODE:' + $M\n\
         if ($M -eq 'git') {{\n\
         $all = @((git ls-files -z --cached --others --exclude-standard 2>$null) -split [char]0 | Where-Object {{ $_ }})\n\
         }} else {{\n\
         $skipDirs = @('.git', 'node_modules', 'target', '__pycache__', '.venv')\n\
         $all = @(); $stack = New-Object System.Collections.Stack; $stack.Push('')\n\
         while ($stack.Count -gt 0) {{ $d = $stack.Pop(); $here = if ($d) {{ $d }} else {{ '.' }}\n\
         foreach ($e in (Get-ChildItem -LiteralPath $here -Force -ErrorAction SilentlyContinue)) {{\n\
         $rel = if ($d) {{ $d + '/' + $e.Name }} else {{ $e.Name }}\n\
         if ($e.PSIsContainer) {{ if ($skipDirs -notcontains $e.Name -and -not $e.LinkType) {{ $stack.Push($rel) }} }} else {{ $all += $rel }}\n\
         }} }}\n\
         }}\n\
         {total}\
         foreach ($p in ($all | Select-Object -Skip {skip} -First {page_size})) {{\n\
         $i = Get-Item -LiteralPath $p -Force -ErrorAction SilentlyContinue\n\
         if ($i -and -not $i.PSIsContainer -and -not $i.LinkType -and $i.Length -lt {max}) {{\n\
         $h = (Get-FileHash -LiteralPath $p -Algorithm SHA256 -ErrorAction SilentlyContinue).Hash\n\
         if ($h) {{ $h.ToLower() + '  ' + $p.Replace('\\', '/') }}\n\
         }} }}\n\
         exit 0"
    )
}

/// The listing's length a page-0 scan reported (`TOTAL:<n>`).
pub fn parse_total(text: &str) -> Option<usize> {
    text.lines()
        .find_map(|l| l.trim_end_matches('\r').strip_prefix("TOTAL:"))
        .and_then(|n| n.trim().parse().ok())
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

/// The PowerShell source of [`read_file_command`]: the file's first [`MAX_FILE_BYTES`] bytes as
/// one line of base64.
pub fn read_file_command_pwsh(repo: &str, rel: &str) -> String {
    format!(
        "$fs = [System.IO.File]::OpenRead((Join-Path '{}' '{}'))\n\
         $b = New-Object byte[] {MAX_FILE_BYTES}\n\
         $n = 0\n\
         while ($n -lt {MAX_FILE_BYTES}) {{ $r = $fs.Read($b, $n, {MAX_FILE_BYTES} - $n); if ($r -le 0) {{ break }}; $n += $r }}\n\
         $fs.Dispose()\n\
         [Convert]::ToBase64String($b, 0, $n)",
        repo.replace('\'', "''"),
        rel.replace('\'', "''")
    )
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
