pub mod bindings;
pub mod clipboard;
pub mod color;
pub mod extensions;
#[cfg(feature = "local_fs")]
pub mod file;
pub mod git;
pub mod image;
pub(crate) mod link_detection;
pub mod links;
pub mod openable_file_type;
#[cfg(feature = "local_tty")]
pub mod path;
pub mod repo_detection;
pub mod retry_strategies;
pub mod sync;
pub mod time_format;
pub mod tooltips;
pub(crate) mod traffic_lights;
pub(crate) mod truncation;
pub mod vm_detection;
#[cfg(windows)]
pub mod windows;

use itertools::Itertools;
use std::cmp::Ordering;
use std::fmt;
use std::ops::Range;

pub fn merge_ranges(mut ranges: Vec<Range<usize>>) -> Vec<Range<usize>> {
    let mut i = 1;
    while i < ranges.len() {
        if ranges[i - 1].end.cmp(&ranges[i].start) >= Ordering::Equal {
            let removed = ranges.remove(i);
            if removed.start.cmp(&ranges[i - 1].start) < Ordering::Equal {
                ranges[i - 1].start = removed.start;
            }
            if removed.end.cmp(&ranges[i - 1].end) > Ordering::Equal {
                ranges[i - 1].end = removed.end;
            }
        } else {
            i += 1;
        }
    }
    ranges
}

pub fn dedupe_from_last(lines: Vec<String>) -> Vec<String> {
    let mut unique_elements = lines.into_iter().rev().unique().collect::<Vec<_>>();
    unique_elements.reverse();
    unique_elements
}

/// Encodes `command` as the base64 UTF-16LE payload PowerShell's `-EncodedCommand`
/// flag expects, in place of `-Command`/`-c <command>`.
///
/// This sidesteps two layers of re-parsing that a plain string argument goes
/// through on Windows: the argv-to-command-line quoting used to hand a child
/// process its arguments (which backslash-escapes embedded `"` per the MSVCRT
/// convention) and PowerShell's own `-Command` tokenizer, which on PS 7.6 does
/// not honor that escaping and aborts with a parser error on any command
/// containing a `"` (e.g. a quoted Windows path). Cross-platform (not gated
/// behind `cfg(windows)`) so shell executors that also run on Unix, like
/// `local_command_executor`, can call it whenever `shell_type` happens to be
/// `PowerShell`. See `arguments_for_session_spawning_command` in
/// `terminal/local_tty/shell.rs` for the interactive-session-launch use of the
/// same flag.
pub fn encode_pwsh_command(command: &str) -> String {
    let utf16le: Vec<u8> = command
        .encode_utf16()
        .flat_map(|w| w.to_le_bytes())
        .collect();
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(&utf16le)
}

pub fn parse_ascii_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }

    let mut result: u32 = 0;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(result)
}

/// AsciiDebug is intended to make it easy to inspect the contents of byte slices that are mostly ASCII
/// characters (but may not be valid unicode). It changes the output of the wrapped byte slice to
/// a human readable string with non-ASCII characters written as hex escapes.
///
/// E.g. `log::info!("{:?}", &AsciiDebug(some_byte_slice));`
pub struct AsciiDebug<'a>(pub &'a [u8]);

impl fmt::Debug for AsciiDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\"")?;
        for &byte in self.0 {
            // Check if the byte is a standard printable character.
            if (32..126).contains(&byte) {
                write!(f, "{}", byte as char)?;
            } else {
                write!(f, "\\{{{byte:02X}}}")?;
            }
        }
        write!(f, "\"")?;
        Ok(())
    }
}

#[test]
fn encode_pwsh_command_round_trips_without_trailing_nul() {
    let script = "Write-Host \"has $env:FOO embedded `\"quotes`\" in it\"";
    let encoded = encode_pwsh_command(script);

    use base64::Engine as _;
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .expect("encode_pwsh_command must produce valid base64");
    assert_eq!(decoded_bytes.len() % 2, 0, "UTF-16LE payload must be an even number of bytes");

    let code_units: Vec<u16> = decoded_bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert_ne!(
        code_units.last(),
        Some(&0u16),
        "no NUL terminator: -EncodedCommand decodes into a length-prefixed managed \
         string, not a C string"
    );
    assert_eq!(String::from_utf16(&code_units).unwrap(), script);
}

#[test]
fn test_dedupe() {
    let history_lines = vec![
        "1".to_string(),
        "3".to_string(),
        "2".to_string(),
        "1".to_string(),
    ];
    assert_eq!(
        dedupe_from_last(history_lines),
        vec!["3".to_string(), "2".to_string(), "1".to_string()]
    );
}

#[test]
fn test_parse_ascii_u32() {
    assert_eq!(parse_ascii_u32(b"123"), Some(123));
    assert_eq!(parse_ascii_u32(b"0"), Some(0));
    assert_eq!(parse_ascii_u32(b"4294967295"), Some(4294967295)); // Max u32
    assert_eq!(parse_ascii_u32(b"4294967296"), None); // Overflow
    assert_eq!(parse_ascii_u32(b""), None);
    assert_eq!(parse_ascii_u32(b"12a3"), None);
}
