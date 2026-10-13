//! How an agent-requested shell command is shown on its approval card (issue #847).
//!
//! The card used to render the raw command in a `CodeEditorView`. That misrepresented commands in
//! two ways, and approving still ran the full text:
//!
//! * A single whitespace-delimited word longer than 16,384 chars was dropped by the text
//!   renderer. It took zero width and lost its glyphs, along with anything glued to it, such as a
//!   trailing `;`. So `echo <100K A>; touch ~/x` showed as `echo  touch ~/x`. The cause is in the
//!   Linux text stack, not here. `build_attrs_list` (`crates/warpui/src/windowing/winit/fonts.rs`)
//!   uses default attrs that never match a span, so cosmic-text shapes an empty run at the start
//!   of every word. rustybuzz's `enter()` then leaves the reused scratch buffer with
//!   `max_len = 16384`, and its `push_str` silently drops a longer word.
//! * A shorter long word (a few thousand chars) laid out at infinite width with no wrap. The
//!   card clipped it horizontally, and nothing showed that more text existed to the right.
//!
//! The approval card therefore never hands the shaper an unbounded word. It shows a preview built
//! here instead:
//!
//! * [`PreviewMode::Collapsed`] (the default): every shell operator and separator character stays
//!   visible and highlighted (`; & | < > ( ) $ backtick quotes backslash { } #`), as do newlines
//!   and every other short token. Only the middle of an over-long *run* (a word or a whitespace
//!   stretch) is replaced by a marker that gives the hidden character count. If the result is
//!   still too long, the middle of the command is cut behind one marker that gives the hidden
//!   count and how many separators it covers. Any elision makes [`CommandApprovalPreview::
//!   is_truncated`] true, and the card then withholds Run until the user asks for the full
//!   command (see [`approval_requires_full_view`]).
//! * [`PreviewMode::Full`]: the whole command with nothing elided. Long runs get zero-width-space
//!   break opportunities, which are display-only, so no shaped word reaches the renderer's limit
//!   and the text soft-wraps instead of clipping.
//!
//! In both modes, control characters and invisible format characters (bidi overrides,
//! zero-width spaces, BOM, ...) are drawn as a highlighted `<U+XXXX>` escape, never passed
//! through. A command must not be able to hide part of itself from the person approving it.

/// Runs (words or whitespace stretches) longer than this many chars are elided in the collapsed
/// preview. This is far below the renderer's 16,384-char per-word limit.
pub(crate) const LONG_RUN_CHARS: usize = 160;
/// How many leading chars of an elided run stay visible.
const LONG_RUN_HEAD_CHARS: usize = 48;
/// How many trailing chars of an elided run stay visible.
const LONG_RUN_TAIL_CHARS: usize = 24;
/// Upper bound on the collapsed preview's displayed length, in chars, before its middle is cut.
pub(crate) const COLLAPSED_PREVIEW_MAX_CHARS: usize = 4_000;
/// In the full preview, a zero-width break opportunity is inserted after every this-many chars of
/// one run. The renderer shapes each piece separately, so the per-word limit is never reached.
const FULL_VIEW_BREAK_EVERY_CHARS: usize = 256;
/// Zero-width space: a line-break opportunity that draws nothing.
const ZERO_WIDTH_BREAK: char = '\u{200B}';

/// Characters that are always shown verbatim and highlighted, and are never folded into an
/// elided run, because they change what (or how many things) the shell runs.
fn is_operator_char(c: char) -> bool {
    matches!(
        c,
        ';' | '&' | '|' | '<' | '>' | '(' | ')' | '$' | '`' | '\'' | '"' | '\\' | '{' | '}' | '#'
    )
}

/// Characters that must never reach the screen raw: controls (C0 other than `\n`/`\t`, DEL, C1)
/// and invisible format characters that can hide or reorder text.
fn must_escape(c: char) -> bool {
    if c == '\n' || c == '\t' {
        return false;
    }
    c.is_control()
        || matches!(
            c,
            '\u{00AD}'
                | '\u{034F}'
                | '\u{061C}'
                | '\u{115F}'
                | '\u{1160}'
                | '\u{17B4}'
                | '\u{17B5}'
                | '\u{180E}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{3164}'
                | '\u{FE00}'..='\u{FE0F}'
                | '\u{FEFF}'
                | '\u{FFA0}'
                | '\u{FFF9}'..='\u{FFFB}'
                | '\u{E0000}'..='\u{E0FFF}'
        )
}

fn escape_char(c: char) -> String {
    format!("<U+{:04X}>", c as u32)
}

/// Formats `n` with thousands separators (`100000` -> `100,000`).
fn group_thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn plural(n: usize, one: &'static str, many: &'static str) -> &'static str {
    if n == 1 { one } else { many }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PreviewMode {
    /// Bounded length. Long runs and, if needed, the middle of the command are elided behind
    /// markers.
    Collapsed,
    /// The entire command. Nothing is elided.
    Full,
}

/// A display form of a command for the approval card.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandApprovalPreview {
    /// The text to draw.
    pub text: String,
    /// Char indices (into `text`) of operator and separator characters, to highlight.
    pub operator_char_indices: Vec<usize>,
    /// Char indices (into `text`) of elision markers and `<U+XXXX>` escapes, to style as
    /// annotations rather than command text.
    pub marker_char_indices: Vec<usize>,
    /// How many chars of the command are not shown in `text`.
    pub hidden_chars: usize,
}

impl CommandApprovalPreview {
    /// True when some part of the command is not visible in this preview.
    pub fn is_truncated(&self) -> bool {
        self.hidden_chars > 0
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PieceKind {
    /// Verbatim command text.
    Text,
    /// A highlighted operator or separator character.
    Operator,
    /// A line break in the command.
    Newline,
    /// An escape or elision marker. Its text is not command text.
    Marker,
}

struct Piece {
    kind: PieceKind,
    text: String,
    /// Chars of the command this piece stands for (shown or hidden).
    command_chars: usize,
    /// Of `command_chars`, how many are not shown.
    hidden_chars: usize,
    /// Command separators (`;`, `&`, `|`, newline) this piece stands for.
    separators: usize,
}

impl Piece {
    fn new(kind: PieceKind, text: String, command_chars: usize) -> Self {
        let separators = match kind {
            PieceKind::Newline => 1,
            PieceKind::Operator if matches!(text.as_str(), ";" | "&" | "|") => 1,
            _ => 0,
        };
        Self {
            kind,
            text,
            command_chars,
            hidden_chars: 0,
            separators,
        }
    }

    fn display_chars(&self) -> usize {
        self.text.chars().count()
    }
}

/// Pushes a run of plain chars (a word, or a whitespace stretch other than newlines). In collapsed
/// mode the run is elided if it is too long. In full mode it gets zero-width break opportunities.
fn push_run(pieces: &mut Vec<Piece>, run: &[char], is_whitespace: bool, mode: PreviewMode) {
    if run.is_empty() {
        return;
    }
    match mode {
        PreviewMode::Collapsed if run.len() > LONG_RUN_CHARS => {
            let hidden = run.len() - LONG_RUN_HEAD_CHARS - LONG_RUN_TAIL_CHARS;
            pieces.push(Piece::new(
                PieceKind::Text,
                run[..LONG_RUN_HEAD_CHARS].iter().collect(),
                LONG_RUN_HEAD_CHARS,
            ));
            let what = if is_whitespace {
                plural(hidden, "whitespace char", "whitespace chars")
            } else {
                plural(hidden, "char", "chars")
            };
            let mut marker = Piece::new(
                PieceKind::Marker,
                format!("…[{} more {what}]…", group_thousands(hidden)),
                hidden,
            );
            marker.hidden_chars = hidden;
            pieces.push(marker);
            pieces.push(Piece::new(
                PieceKind::Text,
                run[run.len() - LONG_RUN_TAIL_CHARS..].iter().collect(),
                LONG_RUN_TAIL_CHARS,
            ));
        }
        PreviewMode::Collapsed => {
            pieces.push(Piece::new(PieceKind::Text, run.iter().collect(), run.len()));
        }
        PreviewMode::Full => {
            let mut text =
                String::with_capacity(run.len() + run.len() / FULL_VIEW_BREAK_EVERY_CHARS);
            for (i, c) in run.iter().enumerate() {
                if i > 0 && i % FULL_VIEW_BREAK_EVERY_CHARS == 0 {
                    text.push(ZERO_WIDTH_BREAK);
                }
                text.push(*c);
            }
            pieces.push(Piece::new(PieceKind::Text, text, run.len()));
        }
    }
}

fn tokenize(command: &str, mode: PreviewMode) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut run: Vec<char> = Vec::new();
    let mut run_is_whitespace = false;

    for c in command.chars() {
        let is_plain_whitespace = c.is_whitespace() && c != '\n' && !must_escape(c);
        let is_plain_word =
            !c.is_whitespace() && !is_operator_char(c) && !must_escape(c) && c != '\n';
        if is_plain_whitespace || is_plain_word {
            if !run.is_empty() && run_is_whitespace != is_plain_whitespace {
                push_run(&mut pieces, &run, run_is_whitespace, mode);
                run.clear();
            }
            run_is_whitespace = is_plain_whitespace;
            run.push(c);
            continue;
        }

        push_run(&mut pieces, &run, run_is_whitespace, mode);
        run.clear();
        if c == '\n' {
            pieces.push(Piece::new(PieceKind::Newline, "\n".to_owned(), 1));
        } else if must_escape(c) {
            pieces.push(Piece::new(PieceKind::Marker, escape_char(c), 1));
        } else {
            pieces.push(Piece::new(PieceKind::Operator, c.to_string(), 1));
        }
    }
    push_run(&mut pieces, &run, run_is_whitespace, mode);
    pieces
}

/// Replaces the middle of `pieces` with a single marker when the displayed text is longer than
/// [`COLLAPSED_PREVIEW_MAX_CHARS`]. The head gets about two thirds of the budget and the tail one
/// third, so the end of the command, where an appended payload sits, is always shown.
fn cut_middle(mut pieces: Vec<Piece>) -> Vec<Piece> {
    let total: usize = pieces.iter().map(Piece::display_chars).sum();
    if total <= COLLAPSED_PREVIEW_MAX_CHARS {
        return pieces;
    }
    let head_budget = COLLAPSED_PREVIEW_MAX_CHARS * 2 / 3;
    let tail_budget = COLLAPSED_PREVIEW_MAX_CHARS - head_budget;

    let mut head_end = 0;
    let mut used = 0;
    while head_end < pieces.len() && used + pieces[head_end].display_chars() <= head_budget {
        used += pieces[head_end].display_chars();
        head_end += 1;
    }
    let mut tail_start = pieces.len();
    used = 0;
    while tail_start > head_end && used + pieces[tail_start - 1].display_chars() <= tail_budget {
        used += pieces[tail_start - 1].display_chars();
        tail_start -= 1;
    }
    if tail_start <= head_end {
        return pieces;
    }

    let tail = pieces.split_off(tail_start);
    let middle = pieces.split_off(head_end);
    let hidden: usize = middle.iter().map(|p| p.command_chars).sum();
    let separators: usize = middle.iter().map(|p| p.separators).sum();
    let separator_note = if separators > 0 {
        format!(
            ", including {} command {}",
            group_thousands(separators),
            plural(separators, "separator", "separators")
        )
    } else {
        String::new()
    };
    let mut marker = Piece::new(
        PieceKind::Marker,
        format!(
            " …[{} {} hidden here{separator_note}]… ",
            group_thousands(hidden),
            plural(hidden, "char", "chars")
        ),
        hidden,
    );
    marker.hidden_chars = hidden;
    marker.separators = separators;
    pieces.push(marker);
    pieces.extend(tail);
    pieces
}

/// Builds the approval-card display of `command`. See the module docs for the guarantees.
pub(crate) fn command_approval_preview(command: &str, mode: PreviewMode) -> CommandApprovalPreview {
    let mut pieces = tokenize(command, mode);
    if mode == PreviewMode::Collapsed {
        pieces = cut_middle(pieces);
    }

    let mut preview = CommandApprovalPreview::default();
    let mut char_index = 0;
    for piece in &pieces {
        let len = piece.display_chars();
        match piece.kind {
            PieceKind::Operator => preview
                .operator_char_indices
                .extend(char_index..char_index + len),
            PieceKind::Marker => preview
                .marker_char_indices
                .extend(char_index..char_index + len),
            PieceKind::Text | PieceKind::Newline => {}
        }
        preview.hidden_chars += piece.hidden_chars;
        preview.text.push_str(&piece.text);
        char_index += len;
    }
    preview
}

/// Whether Run must stay withheld for `command`. True when the collapsed preview hides part of
/// it and the user has not yet chosen to see the full command.
pub(crate) fn approval_requires_full_view(command: &str, full_command_shown: bool) -> bool {
    !full_command_shown && command_approval_preview(command, PreviewMode::Collapsed).is_truncated()
}

/// A rough count of what the command will run. This is a hint for the approval card, not a shell
/// parser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CommandShapeSummary {
    /// Number of non-empty commands separated by `;`, `&&`, `||`, `|`, `&` or newlines outside
    /// quotes.
    pub commands: usize,
    /// Whether the command contains `$(` or a backtick outside single quotes.
    pub has_command_substitution: bool,
}

fn end_segment(segment_has_content: &mut bool, summary: &mut CommandShapeSummary) {
    if *segment_has_content {
        summary.commands += 1;
    }
    *segment_has_content = false;
}

pub(crate) fn command_shape_summary(command: &str) -> CommandShapeSummary {
    let chars: Vec<char> = command.chars().collect();
    let mut summary = CommandShapeSummary::default();
    let mut in_single = false;
    let mut in_double = false;
    let mut segment_has_content = false;
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];
        if in_single {
            if c == '\'' {
                in_single = false;
            }
            i += 1;
            continue;
        }
        match c {
            '\\' => {
                // An escaped char (including an escaped newline) is plain content.
                segment_has_content = true;
                i += 2;
                continue;
            }
            '\'' if !in_double => {
                in_single = true;
                segment_has_content = true;
            }
            '"' => {
                in_double = !in_double;
                segment_has_content = true;
            }
            '`' => {
                summary.has_command_substitution = true;
                segment_has_content = true;
            }
            '$' if chars.get(i + 1) == Some(&'(') => {
                summary.has_command_substitution = true;
                segment_has_content = true;
            }
            ';' | '\n' | '|' if !in_double => {
                end_segment(&mut segment_has_content, &mut summary);
                // `||` is one separator.
                if c == '|' && chars.get(i + 1) == Some(&'|') {
                    i += 1;
                }
            }
            '&' if !in_double => {
                let prev = i.checked_sub(1).map(|p| chars[p]);
                let next = chars.get(i + 1).copied();
                if next == Some('&') {
                    end_segment(&mut segment_has_content, &mut summary);
                    i += 1;
                } else if matches!(prev, Some('>') | Some('<')) || next == Some('>') {
                    // `2>&1`, `<&3`, `&>file`: a redirection, not a separator.
                    segment_has_content = true;
                } else {
                    end_segment(&mut segment_has_content, &mut summary);
                }
            }
            c if c.is_whitespace() => {}
            _ => segment_has_content = true,
        }
        i += 1;
    }
    end_segment(&mut segment_has_content, &mut summary);
    summary
}

/// One-line notes shown above the command on the approval card, in display order.
pub(crate) fn approval_notes(
    command: &str,
    collapsed: &CommandApprovalPreview,
    full_command_shown: bool,
) -> Vec<String> {
    let mut notes = Vec::new();
    if collapsed.is_truncated() {
        let hidden = group_thousands(collapsed.hidden_chars);
        if full_command_shown {
            notes.push(format!(
                "Showing the full command ({} chars).",
                group_thousands(command.chars().count())
            ));
        } else {
            notes.push(format!(
                "{hidden} {} of this command {} hidden. Show the full command to enable Run.",
                plural(collapsed.hidden_chars, "char", "chars"),
                plural(collapsed.hidden_chars, "is", "are"),
            ));
        }
    }
    let shape = command_shape_summary(command);
    if shape.commands > 1 {
        notes.push(format!("This runs {} commands.", shape.commands));
    }
    if shape.has_command_substitution {
        notes.push("This contains command substitution ($(…) or backticks).".to_owned());
    }
    notes
}

/// The command as a one-line header title: the first line, with long runs elided and invisible
/// characters escaped, plus `…` if more lines follow.
pub(crate) fn one_line_title(text: &str) -> String {
    let (first_line, has_more) = match text.find('\n') {
        Some(pos) => (&text[..pos], !text[pos..].trim().is_empty()),
        None => (text, false),
    };
    let mut title = command_approval_preview(first_line, PreviewMode::Collapsed).text;
    if has_more {
        title.push('…');
    }
    title
}

#[cfg(test)]
#[path = "command_approval_preview_tests.rs"]
mod tests;
