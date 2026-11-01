//! Typeahead held while an ssh connect is in flight (#1292, #983).
//!
//! Between the Enter on an `ssh ...` command and the moment the remote shell is bootstrapped and
//! showing its first prompt, keys typed into the pane must not reach the pty: they would be
//! interleaved with the bootstrap script the app injects (the original #983 corruption) or be
//! split between the local and the remote shell. They are held here, exactly as typed and in
//! order, and handed to the remote shell once it is ready, as plain ssh does with the kernel's
//! tty input queue.
//!
//! The buffer can hold a password typed before its prompt was visible, so it is never logged:
//! its `Debug` output shows only counts, and callers log lengths at most.

use std::fmt;
use std::time::{Duration, Instant};

/// Most bytes held for one connect. A connect that hangs must not grow the buffer without limit;
/// past this the whole buffer is dropped (a truncated command line must never run) and further
/// keys of that connect are ignored.
pub(super) const SSH_TYPEAHEAD_MAX_BYTES: usize = 64 * 1024;

/// A key arriving this soon after the previous held key belongs to the same typing burst. When a
/// prompt appears in the middle of a burst the rest of it is discarded with the head, so a tail
/// is not answered into the prompt (or typed into the editor) on its own. A person reacting to a
/// prompt needs far longer.
pub(super) const SSH_TYPEAHEAD_BURST_GAP: Duration = Duration::from_millis(200);

/// Give-up cap (#1332, #1348): the longest the held keys wait, once the remote shell has shown its
/// first prompt, for the session to become able to take a command (its history registered). The
/// event that registers it ends the wait; this only bounds a registration that never comes. Past
/// it the keys are delivered anyway: they are never kept back for good.
pub(super) const SSH_TYPEAHEAD_PRECMD_WAIT_CAP: Duration = Duration::from_secs(5);

/// Whether delivery of the held keys waits (#1332). `awaiting_first_precmd` is that the session
/// would refuse a command only because the Precmd of the remote shell's first prompt has not been
/// processed yet; `waited` is how long delivery has already waited for it.
pub(super) fn delivery_waits_for_first_precmd(
    awaiting_first_precmd: bool,
    waited: Duration,
) -> bool {
    awaiting_first_precmd && waited < SSH_TYPEAHEAD_PRECMD_WAIT_CAP
}

/// One held input, as it reached the terminal view.
#[derive(Clone, PartialEq, Eq)]
pub(super) enum HeldInput {
    /// Printable characters, delivered like typed text.
    Typed(String),
    /// A control key (Enter, Backspace, Ctrl-U, ...), delivered like a key press.
    Key(String),
    /// Text from a paste, already escaped for the shell.
    Paste(String),
    /// A control sequence (arrows, Home, End, Delete, Tab, ...), delivered like one.
    Seq(String),
}

// Never prints the text (see the module docs); a mismatch in a test shows kind and length.
impl fmt::Debug for HeldInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self {
            Self::Typed(_) => "Typed",
            Self::Key(_) => "Key",
            Self::Paste(_) => "Paste",
            Self::Seq(_) => "Seq",
        };
        write!(f, "{kind}(<{} bytes>)", self.len())
    }
}

impl HeldInput {
    pub(super) fn from_chars(chars: &str) -> Self {
        if chars.chars().all(|c| !c.is_control()) {
            Self::Typed(chars.to_owned())
        } else {
            Self::Key(chars.to_owned())
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::Typed(s) | Self::Key(s) | Self::Paste(s) | Self::Seq(s) => s.len(),
        }
    }
}

/// What [`SshTypeahead::push`] did.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum SshTypeaheadPush {
    Held,
    /// This input pushed the buffer past its bound: everything was dropped. Reported once.
    Overflowed,
    /// The buffer already overflowed; the input was ignored.
    Ignored,
}

#[derive(Default)]
pub(super) struct SshTypeahead {
    items: Vec<HeldInput>,
    bytes: usize,
    overflowed: bool,
}

// Never prints the held text: it may be a password typed before its prompt appeared.
impl fmt::Debug for SshTypeahead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SshTypeahead")
            .field("items", &self.items.len())
            .field("bytes", &self.bytes)
            .field("overflowed", &self.overflowed)
            .finish()
    }
}

impl SshTypeahead {
    pub(super) fn push(&mut self, input: HeldInput) -> SshTypeaheadPush {
        if self.overflowed {
            return SshTypeaheadPush::Ignored;
        }
        if self.bytes + input.len() > SSH_TYPEAHEAD_MAX_BYTES {
            self.items.clear();
            self.bytes = 0;
            self.overflowed = true;
            return SshTypeaheadPush::Overflowed;
        }
        self.bytes += input.len();
        // Adjacent printable runs are one entry; the delivered bytes are the same.
        match (self.items.last_mut(), input) {
            (Some(HeldInput::Typed(last)), HeldInput::Typed(more)) => last.push_str(&more),
            (_, input) => self.items.push(input),
        }
        SshTypeaheadPush::Held
    }

    /// Gives up the whole buffer as if it had grown past its bound (the editor's own queue did).
    pub(super) fn overflow(&mut self) -> SshTypeaheadPush {
        if self.overflowed {
            return SshTypeaheadPush::Ignored;
        }
        self.items.clear();
        self.bytes = 0;
        self.overflowed = true;
        SshTypeaheadPush::Overflowed
    }

    /// Nothing is held (an overflowed buffer holds nothing).
    pub(super) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Bytes held; for logging, never the content.
    pub(super) fn len_bytes(&self) -> usize {
        self.bytes
    }

    #[cfg(test)]
    pub(super) fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Forgets everything, including an overflow.
    pub(super) fn clear(&mut self) {
        *self = Self::default();
    }

    /// The held inputs in the order they were typed.
    pub(super) fn take(&mut self) -> Vec<HeldInput> {
        self.bytes = 0;
        std::mem::take(&mut self.items)
    }

    /// The text the user would see had they typed the held keys into an empty editor, without
    /// running anything: Enter becomes a line break, Backspace / Ctrl-U / Ctrl-W edit, other
    /// control keys are skipped. For the fallback when the connect failed.
    pub(super) fn unexecuted_text(&self) -> String {
        let mut text = String::new();
        for item in &self.items {
            match item {
                HeldInput::Typed(s) => text.push_str(s),
                HeldInput::Paste(s) => text.push_str(&s.replace("\r\n", "\n").replace('\r', "\n")),
                HeldInput::Seq(_) => {}
                HeldInput::Key(k) => match k.as_str() {
                    "\r" | "\n" | "\r\n" => text.push('\n'),
                    "\u{7f}" | "\u{8}" => {
                        text.pop();
                    }
                    "\u{15}" => {
                        let line_start = text.rfind('\n').map_or(0, |i| i + 1);
                        text.truncate(line_start);
                    }
                    "\u{17}" => {
                        let kept = text.trim_end_matches([' ', '\t']).len();
                        let word_start = text[..kept]
                            .rfind(|c: char| c.is_whitespace())
                            .map_or(0, |i| i + 1);
                        text.truncate(word_start);
                    }
                    _ => {}
                },
            }
        }
        text.trim_end_matches('\n').to_owned()
    }
}

/// Whether a key arriving at `now` continues a typing burst whose last key was at `last_key`.
pub(super) fn burst_continues(last_key: Option<Instant>, now: Instant) -> bool {
    last_key.is_some_and(|at| now.saturating_duration_since(at) < SSH_TYPEAHEAD_BURST_GAP)
}

#[cfg(test)]
#[path = "ssh_typeahead_tests.rs"]
mod tests;
