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

/// Raw bytes per command when the host has no base64 decoder and the chunk travels as `printf`
/// octal escapes (up to four characters per byte): the command stays under 64 KiB.
pub const PRINTF_CHUNK_BYTES: usize = 16 * 1024;

/// The exit code of the final command when the target no longer matches [`Expect`].
pub const EXIT_CONFLICT: i32 = 5;
/// The exit code when the directory could not be created.
pub const EXIT_MKDIR: i32 = 2;
/// The exit code when a chunk could not be decoded or appended.
pub const EXIT_APPEND: i32 = 3;
/// The exit code when the temp file could not be moved over the target.
pub const EXIT_RENAME: i32 = 4;
/// The exit code when a hash-guarded command could not compute the target's SHA-256 (the host
/// has no `sha256sum`, `shasum` or `Get-FileHash`, or the file cannot be hashed). Nothing was
/// changed. Distinct from [`EXIT_CONFLICT`]: the file did NOT change as far as anyone knows, the
/// host just cannot say, so the message must not claim it did (#1837 follow-up).
pub const EXIT_NO_HASH: i32 = 6;

/// What the target must look like for a guarded write to go ahead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// There must be no file.
    Absent,
    /// The file must have this size and modification time (seconds: the coarsest resolution the
    /// hosts' `stat` commands share).
    Stamp(FileStamp),
    /// The file must have this SHA-256 (hex). Stronger than a stamp: a change within the same
    /// second and size is seen, and a `touch` that leaves the bytes alone is not a change. Only
    /// for hosts that have `sha256sum`, `shasum` or `Get-FileHash` (the caller learned the hash
    /// from the host itself, so it knows it has).
    Sha256(HexDigest),
}

/// A SHA-256 as 64 lowercase hex characters (the form `sha256sum` prints).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HexDigest([u8; 64]);

impl HexDigest {
    /// Accepts exactly 64 hex characters (any case); anything else is not a digest.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.len() != 64 || !text.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let mut out = [0u8; 64];
        for (slot, b) in out.iter_mut().zip(text.bytes()) {
            *slot = b.to_ascii_lowercase();
        }
        Some(Self(out))
    }

    pub fn as_str(&self) -> &str {
        // Ascii by construction.
        std::str::from_utf8(&self.0).unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileStamp {
    pub size: u64,
    pub mtime_secs: u64,
}

impl Expect {
    /// The `WriteFile.expect` form a helper can check: `Absent` and `Sha256`. A stamp has no wire
    /// form (the helper does not compare size+mtime), so it is `None`.
    pub fn to_wire(self) -> Option<crate::proto::WriteExpect> {
        use crate::proto::write_expect::State;
        let state = match self {
            Expect::Absent => State::Absent(true),
            Expect::Sha256(d) => State::Sha256(d.as_str().to_string()),
            Expect::Stamp(_) => return None,
        };
        Some(crate::proto::WriteExpect { state: Some(state) })
    }

    /// The POSIX lines that compute `cur` and refuse (running `cleanup` first) unless it matches:
    /// exit [`EXIT_NO_HASH`] when a hash cannot be computed, [`EXIT_CONFLICT`] when it differs.
    fn posix_check(self, cleanup: &str) -> String {
        let mut out = String::from(self.posix_current());
        if matches!(self, Expect::Sha256(_)) {
            out.push_str(&format!(
                "[ \"$cur\" != nohash ] || {{ {cleanup}echo NOHASH; exit {EXIT_NO_HASH}; }}\n"
            ));
        }
        out.push_str(&format!(
            "[ \"$cur\" = '{}' ] || {{ {cleanup}echo CONFLICT; exit {EXIT_CONFLICT}; }}\n",
            self.token()
        ));
        out
    }

    /// The PowerShell counterpart of [`Self::posix_check`].
    fn pwsh_check(self, cleanup: &str) -> String {
        let mut out = String::from(self.pwsh_current());
        if matches!(self, Expect::Sha256(_)) {
            out.push_str(&format!(
                "if ($cur -eq 'nohash') {{ {cleanup}'NOHASH'; $global:LASTEXITCODE = {EXIT_NO_HASH}; return }}\n"
            ));
        }
        out.push_str(&format!(
            "if ($cur -ne '{}') {{ {cleanup}'CONFLICT'; $global:LASTEXITCODE = {EXIT_CONFLICT}; return }}\n",
            self.token()
        ));
        out
    }

    /// What the final command compares against: `absent` or `<size>|<mtime>`.
    fn token(self) -> String {
        match self {
            Expect::Absent => "absent".to_string(),
            Expect::Stamp(s) => format!("{}|{}", s.size, s.mtime_secs),
            Expect::Sha256(d) => d.as_str().to_string(),
        }
    }

    /// The POSIX lines that set `cur` to what the target is now, in the form [`Self::token`]
    /// takes (`absent` when there is no file).
    fn posix_current(self) -> &'static str {
        match self {
            Expect::Absent | Expect::Stamp(_) => {
                "if [ -e \"$f\" ]; then cur=\"$(wc -c < \"$f\" | tr -d ' \\t')|$(stat -c %Y -- \"$f\" 2>/dev/null || stat -f %m -- \"$f\" 2>/dev/null)\"; else cur=absent; fi\n"
            }
            Expect::Sha256(_) => {
                "if [ -e \"$f\" ]; then cur=$( { sha256sum -- \"$f\" 2>/dev/null || shasum -a 256 -- \"$f\" 2>/dev/null; } | { read -r h _; printf '%s' \"$h\"; } ); [ -n \"$cur\" ] || cur=nohash; else cur=absent; fi\n"
            }
        }
    }

    /// The PowerShell statement that sets `$cur` the same way.
    fn pwsh_current(self) -> &'static str {
        match self {
            Expect::Absent | Expect::Stamp(_) => {
                "if (Test-Path -LiteralPath $f) { $i = Get-Item -LiteralPath $f; $cur = '' + $i.Length + '|' + [DateTimeOffset]::new($i.LastWriteTimeUtc).ToUnixTimeSeconds() } else { $cur = 'absent' }\n"
            }
            Expect::Sha256(_) => {
                "if (Test-Path -LiteralPath $f) { try { $cur = (Get-FileHash -Algorithm SHA256 -LiteralPath $f).Hash.ToLower() } catch { $cur = 'nohash' } } else { $cur = 'absent' }\n"
            }
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
/// `printf` is true for hosts that write through [`printf_escape`] instead of base64.
pub fn split_chunks(content: &[u8], printf: bool) -> Vec<&[u8]> {
    if content.is_empty() {
        return vec![content];
    }
    content
        .chunks(if printf {
            PRINTF_CHUNK_BYTES
        } else {
            WRITE_CHUNK_BYTES
        })
        .collect()
}

/// `bytes` as the body of a single-quoted POSIX `printf` format: letters, digits and most
/// punctuation as they are, everything else (including `\`, `%`, `'`, controls, NUL and every
/// non-ASCII byte) as a three-digit octal escape, which every POSIX `printf` understands.
pub fn printf_escape(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &b in bytes {
        match b {
            b'\\' | b'%' | b'\'' => out.push_str(&format!("\\{b:03o}")),
            b' '..=b'~' => out.push(b as char),
            _ => out.push_str(&format!("\\{b:03o}")),
        }
    }
    out
}

/// `f=...` for `path`, resolving a symlink target so the write goes through it. `~` expands on
/// the host.
pub(super) fn posix_target(path: &str) -> String {
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
    decoder: Option<DecodeTool>,
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
    // With no decoder on the host the bytes travel as printf octal escapes.
    let write = match decoder {
        Some(decoder) => format!(
            "printf '%s' '{}' | {}",
            base64::encode(chunk),
            decoder.filter()
        ),
        None => format!("printf '{}'", printf_escape(chunk)),
    };
    out.push_str(&format!(
        "{write} {redirect} \"$tmp\" || {{ rm -f -- \"$tmp\"; exit {EXIT_APPEND}; }}\n"
    ));
    if position.last {
        if let Some(expect) = expect {
            out.push_str(&expect.posix_check("rm -f -- \"$tmp\"; "));
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

pub(super) fn pwsh_target(path: &str) -> String {
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
            out.push_str(&expect.pwsh_check(
                "Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue; ",
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
    format!("{}\nrm -- \"$f\"\n", posix_target_plain(path))
}

/// `f=...` for `path` (`~` expands on the host), with no symlink resolution: a delete removes the
/// link itself.
fn posix_target_plain(path: &str) -> String {
    if path == "~" {
        "f=\"$HOME\"".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("f=\"$HOME\"{}", sh_quote(&format!("/{rest}")))
    } else {
        format!("f={}", sh_quote(path))
    }
}

/// [`posix_delete_command`] that removes the file only if it is still in the state `expect`
/// names (#1837 follow-up): exit [`EXIT_CONFLICT`] when it differs, [`EXIT_NO_HASH`] when a
/// hash expectation cannot be checked; nothing is removed in either case. A file that is gone
/// is a conflict under a hash expectation. The check and the `rm` are two commands in one
/// script, a window of milliseconds, the same as the guarded write's check and rename.
pub fn posix_guarded_delete_command(path: &str, expect: Expect) -> String {
    let mut out = posix_target_plain(path);
    out.push('\n');
    out.push_str(&expect.posix_check(""));
    out.push_str("rm -- \"$f\"\n");
    out
}

/// The PowerShell counterpart of [`posix_guarded_delete_command`].
pub fn pwsh_guarded_delete_command(path: &str, expect: Expect) -> String {
    let mut out = pwsh_target(path);
    out.push_str("\n$ErrorActionPreference = 'Stop'\n");
    out.push_str(&expect.pwsh_check(""));
    out.push_str("Remove-Item -LiteralPath $f\n");
    out
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
        EXIT_NO_HASH => {
            return format!(
                "Failed to write {path}: could not check that the file on the host is unchanged \
                 (no sha256 tool), so nothing was written."
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

/// The message for a guarded delete whose command exited `exit` with `output` (stderr merged).
pub fn delete_failure_message(path: &str, exit: i32, output: &str) -> String {
    match exit {
        EXIT_CONFLICT => format!(
            "Failed to delete {path}: the file changed on the host after it was read, so it \
             was not deleted."
        ),
        EXIT_NO_HASH => format!(
            "Failed to delete {path}: could not check that the file on the host is unchanged \
             (no sha256 tool), so nothing was deleted."
        ),
        _ => {
            let detail = output.trim();
            if detail.is_empty() {
                format!("Failed to delete {path}")
            } else {
                format!("Failed to delete {path}: {detail}")
            }
        }
    }
}

#[cfg(any(feature = "client", feature = "daemon"))]
/// Whether the file at `path` is in the state `expect` names: absent, or bytes with that SHA-256.
/// An unreadable file or an empty `WriteExpect` does not match. Shared by the helper and the app
/// daemon's guarded delete (#1837 follow-up).
pub fn wire_expectation_holds(path: &std::path::Path, expect: &crate::proto::WriteExpect) -> bool {
    use crate::proto::write_expect::State;
    match &expect.state {
        Some(State::Absent(true)) => {
            std::fs::symlink_metadata(path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        }
        Some(State::Sha256(want)) => std::fs::read(path)
            .map(|bytes| crate::index::sha256_hex(&bytes).eq_ignore_ascii_case(want.trim()))
            .unwrap_or(false),
        _ => false,
    }
}

#[cfg(test)]
#[path = "write_tests.rs"]
mod tests;
