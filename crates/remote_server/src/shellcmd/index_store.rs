//! The codebase index's flat-file store ON THE HOST, driven over the shell (no-helper arm): the
//! commands that find the store, stat its files, write them (atomically, from stdin), append
//! vector records and drop stale vector files. Pure: no I/O.
//!
//! The helper keeps the store under `~/.phosphor/index/<hash of the canonical repository path>/`
//! (`PHOSPHOR_REMOTE_INDEX_DIR` overrides the root). The shell arm uses the SAME directory and
//! the SAME file formats ([`crate::index::store`]), so installing or removing the helper keeps
//! the vectors and nothing is embedded twice (#1881). Payloads travel on the command's stdin,
//! never in its arguments, so a vector batch or a scan table of any size is not limited by the
//! host's argument length and does not show in `ps`.
//!
//! # Exit codes
//!
//! [`EXIT_NO_DIR`] the directory is missing, [`EXIT_WRITE`] the write failed,
//! [`EXIT_RENAME`] the final rename failed, [`EXIT_MISSING`] an append target does not exist,
//! [`EXIT_SHORT`] the target is shorter than the caller's view (another writer rewrote it),
//! [`EXIT_TRUNCATE`] a torn tail could not be cut.

use super::sh_quote;

pub const EXIT_NO_DIR: i32 = 2;
pub const EXIT_WRITE: i32 = 3;
pub const EXIT_RENAME: i32 = 4;
pub const EXIT_MISSING: i32 = 5;
pub const EXIT_SHORT: i32 = 6;
pub const EXIT_TRUNCATE: i32 = 7;

fn pwsh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// Section source printing `REPO:<canonical path of repo>` and `ROOT:<the host's index root>`;
/// exit 3 when `repo` is not a directory.
pub fn resolve_command(pwsh: bool, repo: &str) -> String {
    if pwsh {
        format!(
            "$p = Resolve-Path -LiteralPath {} -ErrorAction SilentlyContinue\n\
             if (-not $p -or -not (Test-Path -LiteralPath $p.ProviderPath -PathType Container)) {{ exit 3 }}\n\
             'REPO:' + $p.ProviderPath\n\
             $r = $env:PHOSPHOR_REMOTE_INDEX_DIR\n\
             if (-not $r) {{ $r = Join-Path $HOME '.phosphor' | Join-Path -ChildPath 'index' }}\n\
             'ROOT:' + $r",
            pwsh_quote(repo)
        )
    } else {
        format!(
            "cd {} 2>/dev/null || exit 3\n\
             echo \"REPO:$(pwd -P)\"\n\
             echo \"ROOT:${{PHOSPHOR_REMOTE_INDEX_DIR:-$HOME/.phosphor/index}}\"",
            sh_quote(repo)
        )
    }
}

/// The two paths [`resolve_command`] printed: `(canonical repository, index root)`.
pub fn parse_resolve(text: &str) -> Option<(String, String)> {
    let mut repo = None;
    let mut root = None;
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(r) = line.strip_prefix("REPO:") {
            repo = Some(r.to_string());
        } else if let Some(r) = line.strip_prefix("ROOT:") {
            root = Some(r.to_string());
        }
    }
    Some((
        repo.filter(|r| !r.is_empty())?,
        root.filter(|r| !r.is_empty())?,
    ))
}

/// Section source printing one line per name: `<name> <size> <mtime seconds>` for a file that
/// exists, `<name> -` for one that does not.
pub fn stat_command(pwsh: bool, dir: &str, names: &[&str]) -> String {
    if pwsh {
        let list: Vec<String> = names.iter().map(|n| pwsh_quote(n)).collect();
        format!(
            "foreach ($n in @({})) {{\n\
             $f = Join-Path {} $n\n\
             if (Test-Path -LiteralPath $f -PathType Leaf) {{ $i = Get-Item -LiteralPath $f; $n + ' ' + $i.Length + ' ' + [DateTimeOffset]::new($i.LastWriteTimeUtc).ToUnixTimeSeconds() }} else {{ $n + ' -' }}\n\
             }}",
            list.join(", "),
            pwsh_quote(dir)
        )
    } else {
        let list: Vec<String> = names.iter().map(|n| sh_quote(n)).collect();
        format!(
            "cd {} 2>/dev/null || {{ for n in {}; do echo \"$n -\"; done; exit 0; }}\n\
             for n in {}; do\n\
             if [ -f \"$n\" ]; then\n\
             s=$(wc -c < \"$n\" | tr -d ' \\t')\n\
             m=$( (stat -c %Y -- \"$n\" 2>/dev/null || stat -f %m -- \"$n\" 2>/dev/null || echo 0) | head -n 1)\n\
             echo \"$n $s $m\"\n\
             else echo \"$n -\"; fi\n\
             done",
            sh_quote(dir),
            list.join(" "),
            list.join(" ")
        )
    }
}

/// What [`stat_command`] said about one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStat {
    pub size: u64,
    pub mtime: u64,
}

/// The files [`stat_command`] reported present, by name.
pub fn parse_stat(text: &str) -> std::collections::HashMap<String, FileStat> {
    let mut out = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        // The name never contains a space (store file names are fixed), the numbers are last.
        let mut parts = line.rsplitn(3, ' ');
        let (Some(mtime), Some(size), Some(name)) = (parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        if let (Ok(size), Ok(mtime)) = (size.parse(), mtime.parse()) {
            out.insert(name.to_string(), FileStat { size, mtime });
        }
    }
    out
}

/// Section source that creates `dir` and writes its stdin to `dir/name` through a temporary
/// file and a rename, so a reader sees the old file or the new one, never a torn one.
pub fn put_command(pwsh: bool, dir: &str, name: &str, nonce: &str) -> String {
    if pwsh {
        format!(
            "$d = {}\n\
             New-Item -ItemType Directory -Force -Path $d | Out-Null\n\
             if (-not (Test-Path -LiteralPath $d -PathType Container)) {{ exit {EXIT_NO_DIR} }}\n\
             $t = Join-Path $d ('.tmp-{nonce}')\n\
             try {{ $o = [System.IO.File]::Create($t); [Console]::OpenStandardInput().CopyTo($o); $o.Dispose() }} catch {{ Remove-Item -LiteralPath $t -Force -ErrorAction SilentlyContinue; exit {EXIT_WRITE} }}\n\
             try {{ Move-Item -LiteralPath $t -Destination (Join-Path $d {}) -Force }} catch {{ Remove-Item -LiteralPath $t -Force -ErrorAction SilentlyContinue; exit {EXIT_RENAME} }}",
            pwsh_quote(dir),
            pwsh_quote(name)
        )
    } else {
        format!(
            "d={}\n\
             mkdir -p -- \"$d\" || exit {EXIT_NO_DIR}\n\
             t=\"$d/.tmp-{nonce}\"\n\
             cat > \"$t\" || {{ rm -f -- \"$t\"; exit {EXIT_WRITE}; }}\n\
             mv -f -- \"$t\" \"$d\"/{} || {{ rm -f -- \"$t\"; exit {EXIT_RENAME}; }}",
            sh_quote(dir),
            sh_quote(name)
        )
    }
}

/// Section source that appends its stdin to `dir/name`, first cutting the file back to
/// `whole_len` bytes if it is longer (a torn tail). It refuses ([`EXIT_SHORT`]) a file shorter
/// than `whole_len`: someone rewrote it since the caller looked.
pub fn append_command(pwsh: bool, dir: &str, name: &str, whole_len: u64) -> String {
    if pwsh {
        format!(
            "$f = Join-Path {} {}\n\
             if (-not (Test-Path -LiteralPath $f -PathType Leaf)) {{ exit {EXIT_MISSING} }}\n\
             $o = [System.IO.File]::Open($f, [System.IO.FileMode]::Open, [System.IO.FileAccess]::Write)\n\
             if ($o.Length -lt {whole_len}) {{ $o.Dispose(); exit {EXIT_SHORT} }}\n\
             $o.SetLength({whole_len})\n\
             [void]$o.Seek({whole_len}, [System.IO.SeekOrigin]::Begin)\n\
             try {{ [Console]::OpenStandardInput().CopyTo($o) }} catch {{ $o.Dispose(); exit {EXIT_WRITE} }}\n\
             $o.Dispose()",
            pwsh_quote(dir),
            pwsh_quote(name)
        )
    } else {
        format!(
            "cd {} 2>/dev/null || exit {EXIT_NO_DIR}\n\
             f={}\n\
             [ -f \"$f\" ] || exit {EXIT_MISSING}\n\
             size=$(wc -c < \"$f\" | tr -d ' \\t')\n\
             if [ \"$size\" -lt {whole_len} ]; then exit {EXIT_SHORT}; fi\n\
             if [ \"$size\" -gt {whole_len} ]; then dd if=/dev/null of=\"$f\" bs=1 seek={whole_len} 2>/dev/null || exit {EXIT_TRUNCATE}; fi\n\
             cat >> \"$f\" || exit {EXIT_WRITE}",
            sh_quote(dir),
            sh_quote(name)
        )
    }
}

/// Section source that deletes every `vectors-*.bin` in `dir` except `keep`.
pub fn drop_other_vectors_command(pwsh: bool, dir: &str, keep: &str) -> String {
    if pwsh {
        format!(
            "Get-ChildItem -LiteralPath {} -Filter 'vectors-*.bin' -File -ErrorAction SilentlyContinue | Where-Object {{ $_.Name -ne {} }} | Remove-Item -Force",
            pwsh_quote(dir),
            pwsh_quote(keep)
        )
    } else {
        format!(
            "cd {} 2>/dev/null || exit 0\n\
             for f in vectors-*.bin; do [ -f \"$f\" ] && [ \"$f\" != {} ] && rm -f -- \"$f\"; done\n\
             exit 0",
            sh_quote(dir),
            sh_quote(keep)
        )
    }
}

/// Section source listing the vector files of `dir`, as [`stat_command`] reports them (the
/// names are `vectors-*.bin`).
pub fn list_vectors_command(pwsh: bool, dir: &str) -> String {
    if pwsh {
        format!(
            "Get-ChildItem -LiteralPath {} -Filter 'vectors-*.bin' -File -ErrorAction SilentlyContinue | ForEach-Object {{ $_.Name + ' ' + $_.Length + ' ' + [DateTimeOffset]::new($_.LastWriteTimeUtc).ToUnixTimeSeconds() }}",
            pwsh_quote(dir)
        )
    } else {
        format!(
            "cd {} 2>/dev/null || exit 0\n\
             for n in vectors-*.bin; do\n\
             [ -f \"$n\" ] || continue\n\
             s=$(wc -c < \"$n\" | tr -d ' \\t')\n\
             m=$( (stat -c %Y -- \"$n\" 2>/dev/null || stat -f %m -- \"$n\" 2>/dev/null || echo 0) | head -n 1)\n\
             echo \"$n $s $m\"\n\
             done\n\
             exit 0",
            sh_quote(dir)
        )
    }
}

#[cfg(test)]
#[path = "index_store_tests.rs"]
mod tests;
