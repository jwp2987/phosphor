//! What the shell arm of buffer editing asks a host: the state of a file (its size and
//! modification time, and its SHA-256) so a later save can refuse to overwrite a change somebody
//! else made (design ruling R4: conflicts are found at save, there is no live push).
//!
//! The commands are the ones the guarded write compares against ([`super::write::Expect`]), so
//! what is read here is what the write checks, byte for byte: `size|mtime-seconds`, or the hash.
//! A host without `sha256sum`/`shasum`/`Get-FileHash` prints an empty hash and the caller falls
//! back to the stamp.

use super::write::{FileStamp, HexDigest, posix_target, pwsh_target};

/// The section that prints `<size>|<mtime seconds>` of `path`, or `absent`.
pub fn stamp_command(pwsh: bool, path: &str) -> String {
    if pwsh {
        format!(
            "{}\nif (Test-Path -LiteralPath $f) {{ $i = Get-Item -LiteralPath $f; '' + $i.Length + '|' + [DateTimeOffset]::new($i.LastWriteTimeUtc).ToUnixTimeSeconds() }} else {{ 'absent' }}",
            pwsh_target(path)
        )
    } else {
        format!(
            "{}\nif [ -e \"$f\" ]; then echo \"$(wc -c < \"$f\" | tr -d ' \\t')|$(stat -c %Y -- \"$f\" 2>/dev/null || stat -f %m -- \"$f\" 2>/dev/null)\"; else echo absent; fi",
            posix_target(path)
        )
    }
}

/// The section that prints the SHA-256 of `path` as hex, or nothing (no file, or the host has no
/// tool for it).
pub fn sha_command(pwsh: bool, path: &str) -> String {
    if pwsh {
        format!(
            "{}\nif (Test-Path -LiteralPath $f) {{ (Get-FileHash -Algorithm SHA256 -LiteralPath $f).Hash.ToLower() }}",
            pwsh_target(path)
        )
    } else {
        format!(
            "{}\nif [ -e \"$f\" ]; then {{ sha256sum -- \"$f\" 2>/dev/null || shasum -a 256 -- \"$f\" 2>/dev/null; }} | {{ read -r h _; printf '%s' \"$h\"; }}; fi",
            posix_target(path)
        )
    }
}

/// Reads the output of [`stamp_command`]. `None` for `absent` or anything unreadable.
pub fn parse_stamp(text: &str) -> Option<FileStamp> {
    let (size, mtime) = text.trim().split_once('|')?;
    Some(FileStamp {
        size: size.trim().parse().ok()?,
        mtime_secs: mtime.trim().parse().ok()?,
    })
}

/// Reads the output of [`sha_command`].
pub fn parse_sha(text: &str) -> Option<HexDigest> {
    HexDigest::parse(text)
}

#[cfg(test)]
#[path = "buffer_tests.rs"]
mod tests;
