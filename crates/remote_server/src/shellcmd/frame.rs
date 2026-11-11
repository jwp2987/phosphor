//! The framed output envelope: how one shell call returns several named results, and an exit
//! code for each, over a channel that is just bytes.
//!
//! [`Script`] renders N named commands as one script. Each runs in its own subshell and prints
//!
//! ```text
//! @@PHOSPHOR-<nonce> BEGIN <name>
//! <the command's output, byte for byte>
//! @@PHOSPHOR-<nonce> END <name> exit=<code>
//! ```
//!
//! with one newline (added by the script) between the output and the END marker. [`Frame::parse`]
//! reads that back. The nonce is chosen by the caller per call (no randomness here), so output
//! cannot forge a marker without knowing it. Anything before the first BEGIN or after the last END
//! (a login banner, an MOTD) is ignored. Binary output must be base64'd by the command itself
//! (see [`super::base64`]); the envelope is for text and for already-encoded payloads.

use super::MAX_OUTPUT_BYTES;
use super::shell_kind::ShellKind;

/// What happens to a command's stderr.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StderrMode {
    /// Folded into the section's output (`2>&1`).
    Merge,
    /// Thrown away (`2>/dev/null`).
    Discard,
    /// Left alone: it goes to the channel's own stderr, returned separately by the executor.
    Inherit,
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FrameError {
    #[error("invalid {what}: {value:?} (letters, digits, '_', '-' and '.' only; not empty)")]
    InvalidIdentifier { what: &'static str, value: String },
    #[error("no framed output found")]
    NoFrame,
    #[error("output ended inside section {0:?}")]
    Truncated(String),
    #[error("malformed frame: {0}")]
    Malformed(String),
    #[error("section {section:?} is {bytes} bytes, over the {max} byte cap")]
    TooLarge {
        section: String,
        bytes: usize,
        max: usize,
    },
}

fn valid_identifier(s: &str, extra: &[char]) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || extra.contains(&c))
}

#[derive(Clone, Debug)]
struct SectionSpec {
    name: String,
    command: String,
    stderr: StderrMode,
}

/// A script of named sections, built with the caller's nonce.
#[derive(Clone, Debug)]
pub struct Script {
    nonce: String,
    sections: Vec<SectionSpec>,
}

impl Script {
    /// `nonce` must be letters and digits only; make it unguessable to the commands (a random
    /// hex string per call).
    pub fn new(nonce: impl Into<String>) -> Result<Self, FrameError> {
        let nonce = nonce.into();
        if !valid_identifier(&nonce, &[]) {
            return Err(FrameError::InvalidIdentifier {
                what: "nonce",
                value: nonce,
            });
        }
        Ok(Self {
            nonce,
            sections: Vec::new(),
        })
    }

    /// Adds a section. `command` is source for the target shell (POSIX source, or PowerShell
    /// source when rendering for [`ShellKind::Pwsh`]). `name` is letters, digits, `_`, `-`, `.`.
    pub fn section(
        mut self,
        name: impl Into<String>,
        command: impl Into<String>,
        stderr: StderrMode,
    ) -> Result<Self, FrameError> {
        let name = name.into();
        if !valid_identifier(&name, &['-', '.']) {
            return Err(FrameError::InvalidIdentifier {
                what: "section name",
                value: name,
            });
        }
        self.sections.push(SectionSpec {
            name,
            command: command.into(),
            stderr,
        });
        Ok(self)
    }

    pub fn nonce(&self) -> &str {
        &self.nonce
    }

    /// The section names in order (for the per-exec debug line; never the commands).
    pub fn section_names(&self) -> Vec<&str> {
        self.sections.iter().map(|s| s.name.as_str()).collect()
    }

    fn marker(&self) -> String {
        format!("@@PHOSPHOR-{}", self.nonce)
    }

    /// The script source for a POSIX shell (also what `Fish` and `Nu` run, via `sh -c`).
    pub fn render_posix(&self) -> String {
        let marker = self.marker();
        let mut out = String::new();
        for s in &self.sections {
            let redirect = match s.stderr {
                StderrMode::Merge => " 2>&1",
                StderrMode::Discard => " 2>/dev/null",
                StderrMode::Inherit => "",
            };
            out.push_str(&format!(
                "printf '%s\\n' '{marker} BEGIN {name}'\n(\n{command}\n){redirect}\n__phosphor_e=$?\nprintf '\\n%s exit=%s\\n' '{marker} END {name}' \"$__phosphor_e\"\n",
                name = s.name,
                command = s.command,
            ));
        }
        out
    }

    /// The script source for PowerShell. Best effort until the lab's `u_pwsh` shell proves it:
    /// the markers are written through the pipeline so ordering with the command's own output is
    /// preserved, and the parser accepts the CRLF line ends Windows adds.
    pub fn render_pwsh(&self) -> String {
        let marker = self.marker();
        let mut out = String::new();
        for s in &self.sections {
            let redirect = match s.stderr {
                StderrMode::Merge => " 2>&1",
                StderrMode::Discard => " 2>$null",
                StderrMode::Inherit => "",
            };
            out.push_str(&format!(
                "Write-Output '{marker} BEGIN {name}'\n$global:LASTEXITCODE = 0\n& {{\n{command}\n}}{redirect}\n$__ok = $?\n$__e = $LASTEXITCODE\nif (-not $__ok -and -not $__e) {{ $__e = 1 }}\nWrite-Output ('{marker} END {name} exit=' + $__e)\n",
                name = s.name,
                command = s.command,
            ));
        }
        out
    }

    /// The remote command string for a login shell of `kind`: render, then wrap.
    pub fn render(&self, kind: ShellKind) -> String {
        match kind {
            ShellKind::Pwsh => kind.wrap(&self.render_pwsh()),
            _ => kind.wrap(&self.render_posix()),
        }
    }
}

/// One section read back.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedSection {
    pub name: String,
    /// The section's output with the one framing newline removed. Bytes, not text.
    pub output: Vec<u8>,
    pub exit: i32,
}

impl ParsedSection {
    pub fn success(&self) -> bool {
        self.exit == 0
    }

    /// The output as text (invalid UTF-8 replaced).
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.output).into_owned()
    }
}

/// A parsed envelope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub sections: Vec<ParsedSection>,
}

impl Frame {
    pub fn section(&self, name: &str) -> Option<&ParsedSection> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// [`Self::parse_with_cap`] at [`MAX_OUTPUT_BYTES`].
    pub fn parse(output: &[u8], nonce: &str) -> Result<Self, FrameError> {
        Self::parse_with_cap(output, nonce, MAX_OUTPUT_BYTES)
    }

    /// Parses `output`, which was produced by a script rendered with `nonce`. Any one section
    /// bigger than `max_section_bytes` is an error.
    pub fn parse_with_cap(
        output: &[u8],
        nonce: &str,
        max_section_bytes: usize,
    ) -> Result<Self, FrameError> {
        let prefix = format!("@@PHOSPHOR-{nonce} ").into_bytes();
        let mut sections = Vec::new();
        // The open section: (name, offset where its output starts).
        let mut open: Option<(String, usize)> = None;
        let mut pos = 0;
        while pos < output.len() {
            let Some(rel) = find(&output[pos..], &prefix) else {
                break;
            };
            let at = pos + rel;
            if at != 0 && output[at - 1] != b'\n' {
                // Not at the start of a line: ordinary output that happens to contain the text.
                pos = at + 1;
                continue;
            }
            let eol = output[at..].iter().position(|&b| b == b'\n');
            let (line_end, next) = match eol {
                Some(p) => (at + p, at + p + 1),
                None => (output.len(), output.len()),
            };
            let mut line = &output[at + prefix.len()..line_end];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
            let line = String::from_utf8_lossy(line);
            if let Some(name) = line.strip_prefix("BEGIN ") {
                if let Some((open_name, _)) = &open {
                    return Err(FrameError::Malformed(format!(
                        "section {name:?} began inside {open_name:?}"
                    )));
                }
                open = Some((name.to_owned(), next));
            } else if let Some(rest) = line.strip_prefix("END ") {
                let Some((open_name, start)) = open.take() else {
                    return Err(FrameError::Malformed(format!(
                        "END without BEGIN: {rest:?}"
                    )));
                };
                let (name, exit) = rest
                    .rsplit_once(" exit=")
                    .and_then(|(n, e)| Some((n, e.trim().parse::<i32>().ok()?)))
                    .ok_or_else(|| FrameError::Malformed(format!("bad END line: {rest:?}")))?;
                if name != open_name {
                    return Err(FrameError::Malformed(format!(
                        "END {name:?} closed section {open_name:?}"
                    )));
                }
                let mut body = &output[start..at];
                if body.ends_with(b"\r\n") {
                    body = &body[..body.len() - 2];
                } else if body.ends_with(b"\n") {
                    body = &body[..body.len() - 1];
                }
                if body.len() > max_section_bytes {
                    return Err(FrameError::TooLarge {
                        section: open_name,
                        bytes: body.len(),
                        max: max_section_bytes,
                    });
                }
                sections.push(ParsedSection {
                    name: open_name,
                    output: body.to_vec(),
                    exit,
                });
            } else {
                return Err(FrameError::Malformed(format!("unknown marker: {line:?}")));
            }
            pos = next;
        }
        if let Some((name, _)) = open {
            return Err(FrameError::Truncated(name));
        }
        if sections.is_empty() {
            return Err(FrameError::NoFrame);
        }
        Ok(Self { sections })
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
#[path = "frame_tests.rs"]
mod tests;
