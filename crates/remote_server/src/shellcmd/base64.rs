//! Base64 for binary-safe transfer over a text channel, and the probe that finds out which
//! encoder the remote host has.
//!
//! The host side is whatever the host ships: GNU `base64 -w0`, BSD/busybox `base64`,
//! `openssl base64`, or, as the last resort, `od -An -v -tx1` (hex, decoded by [`decode_hex`]).
//! [`probe_command`] prints one tag; [`Base64Tool::from_probe_output`] reads it back. The probe is
//! run once per host and cached by the caller.

/// Which encoder the host has, best first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Base64Tool {
    /// GNU coreutils: `base64 -w0` (no line wrapping).
    GnuW0,
    /// BSD, macOS and busybox: plain `base64` (may wrap at 76 columns; the decoder ignores
    /// whitespace).
    Plain,
    /// `openssl base64` (wraps at 64 columns).
    Openssl,
    /// POSIX `od -An -v -tx1`: hex, not base64. Slowest and 2.0x larger, but it is in POSIX.
    Od,
}

const TAG_GNU: &str = "gnu-w0";
const TAG_PLAIN: &str = "plain";
const TAG_OPENSSL: &str = "openssl";
const TAG_OD: &str = "od";
const TAG_NONE: &str = "none";

/// A POSIX command that prints exactly one of `gnu-w0`, `plain`, `openssl`, `od`, `none`.
/// Nothing is written to a file and nothing is installed.
pub fn probe_command() -> String {
    format!(
        "if base64 -w0 </dev/null >/dev/null 2>&1; then echo {TAG_GNU}; \
         elif base64 </dev/null >/dev/null 2>&1; then echo {TAG_PLAIN}; \
         elif command -v openssl >/dev/null 2>&1 && openssl base64 </dev/null >/dev/null 2>&1; then echo {TAG_OPENSSL}; \
         elif command -v od >/dev/null 2>&1; then echo {TAG_OD}; \
         else echo {TAG_NONE}; fi"
    )
}

impl Base64Tool {
    /// Reads the probe's output. `None` for `none` or anything unrecognised.
    pub fn from_probe_output(output: &str) -> Option<Self> {
        match output.trim() {
            TAG_GNU => Some(Self::GnuW0),
            TAG_PLAIN => Some(Self::Plain),
            TAG_OPENSSL => Some(Self::Openssl),
            TAG_OD => Some(Self::Od),
            _ => None,
        }
    }

    /// The command that writes the file named by the already-quoted `quoted_path` to stdout in
    /// this tool's encoding. Reads via stdin redirection so a path starting with `-` is safe.
    pub fn encode_file_command(self, quoted_path: &str) -> String {
        match self {
            Self::GnuW0 => format!("base64 -w0 < {quoted_path}"),
            Self::Plain => format!("base64 < {quoted_path}"),
            Self::Openssl => format!("openssl base64 < {quoted_path}"),
            Self::Od => format!("od -An -v -tx1 < {quoted_path}"),
        }
    }

    /// Decodes what [`Self::encode_file_command`] printed.
    pub fn decode(self, output: &[u8]) -> Result<Vec<u8>, DecodeError> {
        match self {
            Self::Od => decode_hex(output),
            _ => decode(output),
        }
    }
}

/// A malformed encoded payload.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("invalid base64 byte 0x{0:02x}")]
    InvalidByte(u8),
    #[error("base64 input has a dangling partial group")]
    DanglingGroup,
    #[error("invalid padding")]
    BadPadding,
    #[error("odd number of hex digits")]
    OddHex,
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// Standard base64 with padding.
pub fn encode(input: &[u8]) -> String {
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

fn value_of(byte: u8) -> Option<u32> {
    match byte {
        b'A'..=b'Z' => Some(u32::from(byte - b'A')),
        b'a'..=b'z' => Some(u32::from(byte - b'a') + 26),
        b'0'..=b'9' => Some(u32::from(byte - b'0') + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

/// Standard base64; ASCII whitespace anywhere is ignored (tools wrap at 64 or 76 columns).
/// Padding is optional at the end.
pub fn decode(input: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(input.len() / 4 * 3);
    let mut acc: u32 = 0;
    let mut bits = 0u32;
    let mut padding = 0usize;
    for &byte in input {
        if byte.is_ascii_whitespace() {
            continue;
        }
        if byte == b'=' {
            padding += 1;
            continue;
        }
        if padding > 0 {
            // Data after padding.
            return Err(DecodeError::BadPadding);
        }
        let v = value_of(byte).ok_or(DecodeError::InvalidByte(byte))?;
        acc = (acc << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
            acc &= (1 << bits) - 1;
        }
    }
    // `bits` of 6 means one dangling sextet, which cannot encode a byte.
    if bits == 6 {
        return Err(DecodeError::DanglingGroup);
    }
    if padding > 2 {
        return Err(DecodeError::BadPadding);
    }
    Ok(out)
}

/// Decodes `od -An -v -tx1` output: hex byte pairs separated by whitespace.
pub fn decode_hex(input: &[u8]) -> Result<Vec<u8>, DecodeError> {
    let mut out = Vec::with_capacity(input.len() / 3);
    let mut high: Option<u8> = None;
    for &byte in input {
        if byte.is_ascii_whitespace() {
            if high.is_some() {
                return Err(DecodeError::OddHex);
            }
            continue;
        }
        let v = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return Err(DecodeError::InvalidByte(byte)),
        };
        match high.take() {
            None => high = Some(v),
            Some(h) => out.push((h << 4) | v),
        }
    }
    if high.is_some() {
        return Err(DecodeError::OddHex);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "base64_tests.rs"]
mod tests;
