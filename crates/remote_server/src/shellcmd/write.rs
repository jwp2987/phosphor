//! Writing and deleting files on a host from the shell (parity: `apply_file_diffs` writes).
//!
//! A write is a temp file in the target's directory plus a rename, so a reader never sees half a
//! file and a failed write leaves the old one: the content is base64'd by the app, sent in chunks
//! of at most [`WRITE_CHUNK_BYTES`] (so no command line is longer than the host accepts),
//! decoded and appended to the temp file, and the last command preserves the target's mode and
//! moves the temp file over it. The temp name carries a per-write nonce so every chunk's command
//! finds the same file. An optional [`Expect`] makes the final command refuse (and remove the
//! temp file) when the target is no longer in the state the caller read it in.
//!
//! POSIX hosts get `printf | base64 -d`; PowerShell hosts get `[IO.File]::WriteAllBytes` and
//! `Move-Item` (a failure sets `$LASTEXITCODE` and returns: `exit` inside the framed script block
//! would end the whole host and lose the frame). A symlink target is written through (like `std::fs::write`) rather than replaced.

use super::base64::{self, DecodeTool};
use super::sh_quote;

/// Raw bytes per command. 64 KiB is 88 KiB of base64, under the 128 KiB a single argument may
/// be on Linux.
pub const WRITE_CHUNK_BYTES: usize = 64 * 1024;

/// The exit code of the final command when the target no longer matches [`Expect`].
pub const EXIT_CONFLICT: i32 = 5;
/// The exit code when the directory could not be created.
pub const EXIT_MKDIR: i32 = 2;
/// The exit code when a chunk could not be decoded or appended.
pub const EXIT_APPEND: i32 = 3;
/// The exit code when the temp file could not be moved over the target.
pub const EXIT_RENAME: i32 = 4;

/// What the target must look like for a guarded write to go ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// There must be no file.
    Absent,
    /// The file must have this size and modification time (seconds: the coarsest resolution the
    /// hosts' `stat` commands share).
    Stamp(FileStamp),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    pub mtime_secs: u64,
}

impl Expect {
    /// What the final command compares against: `absent` or `<size>|<mtime>`.
    fn token(self) -> String {
        match self {
            Expect::Absent => "absent".to_string(),
            Expect::Stamp(s) => format!("{}|{}", s.size, s.mtime_secs),
        }
    }
}

/// Which chunk of a write a command carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkPosition {
    pub first: bool,
    pub last: bool,
}

/// Splits `content` into chunks; always at least one (an empty file is one empty chunk).
pub fn split_chunks(content: &[u8]) -> Vec<&[u8]> {
    if content.is_empty() {
        return vec![content];
    }
    content.chunks(WRITE_CHUNK_BYTES).collect()
}

/// `f=...` for `path`, resolving a symlink target so the write goes through it. `~` expands on
/// the host.
fn posix_target(path: &str) -> String {
    let assign = if path == "~" {
        "f=\"$HOME\"".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("f=\"$HOME\"{}", sh_quote(&format!("/{rest}")))
    } else {
        format!("f={}", sh_quote(path))
    };
    format!(
        "{assign}\n\
         if [ -L \"$f\" ]; then t=$(readlink -f -- \"$f\" 2>/dev/null); [ -n \"$t\" ] && f=$t; fi"
    )
}

/// The POSIX command for one chunk of a write.
pub fn posix_write_command(
    path: &str,
    nonce: &str,
    chunk: &[u8],
    position: ChunkPosition,
    decoder: DecodeTool,
    expect: Option<Expect>,
) -> String {
    let mut out = posix_target(path);
    out.push_str(&format!("\ntmp=\"$f.phosphor-tmp-{nonce}\"\n"));
    if position.first {
        out.push_str(&format!(
            "mkdir -p -- \"$(dirname -- \"$f\")\" || exit {EXIT_MKDIR}\n"
        ));
    }
    let redirect = if position.first { ">" } else { ">>" };
    out.push_str(&format!(
        "printf '%s' '{}' | {} {redirect} \"$tmp\" || {{ rm -f -- \"$tmp\"; exit {EXIT_APPEND}; }}\n",
        base64::encode(chunk),
        decoder.filter()
    ));
    if position.last {
        if let Some(expect) = expect {
            out.push_str(&format!(
                "if [ -e \"$f\" ]; then cur=\"$(wc -c < \"$f\" | tr -d ' \\t')|$(stat -c %Y -- \"$f\" 2>/dev/null || stat -f %m -- \"$f\" 2>/dev/null)\"; else cur=absent; fi\n\
                 [ \"$cur\" = '{}' ] || {{ rm -f -- \"$tmp\"; echo CONFLICT; exit {EXIT_CONFLICT}; }}\n",
                expect.token()
            ));
        }
        out.push_str(&format!(
            "if [ -e \"$f\" ]; then m=$(stat -c %a -- \"$f\" 2>/dev/null || stat -f %Lp -- \"$f\" 2>/dev/null); [ -n \"$m\" ] && chmod \"$m\" \"$tmp\" 2>/dev/null; fi\n\
             mv -f -- \"$tmp\" \"$f\" || {{ rm -f -- \"$tmp\"; exit {EXIT_RENAME}; }}\n"
        ));
    }
    out
}

fn pwsh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

fn pwsh_target(path: &str) -> String {
    if path == "~" {
        "$f = $HOME".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("$f = Join-Path $HOME {}", pwsh_quote(rest))
    } else {
        format!("$f = {}", pwsh_quote(path))
    }
}

/// The PowerShell command for one chunk of a write (same contract as [`posix_write_command`]).
pub fn pwsh_write_command(
    path: &str,
    nonce: &str,
    chunk: &[u8],
    position: ChunkPosition,
    expect: Option<Expect>,
) -> String {
    let mut out = pwsh_target(path);
    out.push_str(&format!("\n$tmp = $f + '.phosphor-tmp-{nonce}'\n"));
    out.push_str("$ErrorActionPreference = 'Stop'\n");
    let b64 = base64::encode(chunk);
    if position.first {
        out.push_str(&format!(
            "try {{ New-Item -ItemType Directory -Force -Path (Split-Path -Parent $f) | Out-Null }} catch {{ $global:LASTEXITCODE = {EXIT_MKDIR}; return }}\n\
             try {{ [IO.File]::WriteAllBytes($tmp, [Convert]::FromBase64String('{b64}')) }} catch {{ $global:LASTEXITCODE = {EXIT_APPEND}; return }}\n"
        ));
    } else {
        out.push_str(&format!(
            "try {{ $b = [Convert]::FromBase64String('{b64}'); $s = [IO.File]::Open($tmp, 'Append'); $s.Write($b, 0, $b.Length); $s.Dispose() }} catch {{ Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue; $global:LASTEXITCODE = {EXIT_APPEND}; return }}\n"
        ));
    }
    if position.last {
        if let Some(expect) = expect {
            out.push_str(&format!(
                "if (Test-Path -LiteralPath $f) {{ $i = Get-Item -LiteralPath $f; $cur = '' + $i.Length + '|' + [DateTimeOffset]::new($i.LastWriteTimeUtc).ToUnixTimeSeconds() }} else {{ $cur = 'absent' }}\n\
                 if ($cur -ne '{}') {{ Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue; 'CONFLICT'; $global:LASTEXITCODE = {EXIT_CONFLICT}; return }}\n",
                expect.token()
            ));
        }
        out.push_str(&format!(
            "try {{ Move-Item -LiteralPath $tmp -Destination $f -Force }} catch {{ Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue; $global:LASTEXITCODE = {EXIT_RENAME}; return }}\n"
        ));
    }
    out
}

/// The POSIX command that deletes a file (an error, like `std::fs::remove_file`, when it does not
/// exist or is a directory).
pub fn posix_delete_command(path: &str) -> String {
    let assign = if path == "~" {
        "f=\"$HOME\"".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("f=\"$HOME\"{}", sh_quote(&format!("/{rest}")))
    } else {
        format!("f={}", sh_quote(path))
    };
    format!("{assign}\nrm -- \"$f\"\n")
}

pub fn pwsh_delete_command(path: &str) -> String {
    format!(
        "{}\n$ErrorActionPreference = 'Stop'\nRemove-Item -LiteralPath $f\n",
        pwsh_target(path)
    )
}

/// The message for a write whose command exited `exit` with `output` (stderr merged).
pub fn write_failure_message(path: &str, exit: i32, output: &str) -> String {
    let detail = output.trim();
    let why = match exit {
        EXIT_CONFLICT => {
            return format!(
                "Failed to write {path}: the file changed on the host after it was read, so it \
                 was not overwritten."
            );
        }
        EXIT_MKDIR => "could not create the directory",
        EXIT_APPEND => "could not write the temporary file",
        EXIT_RENAME => "could not move the temporary file into place",
        _ => "the host's write command failed",
    };
    if detail.is_empty() {
        format!("Failed to write {path}: {why}")
    } else {
        format!("Failed to write {path}: {why} ({detail})")
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
