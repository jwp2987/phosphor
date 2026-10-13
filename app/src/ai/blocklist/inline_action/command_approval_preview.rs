//! How an agent-requested shell command is shown on its approval card (issue #847).
//!
//! The card used to render the raw command in a `CodeEditorView`. That misrepresented commands in
//! three ways, and approving still ran the full text:
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
//! * Blank lines pushed a payload below the card's 500px fold: `echo hi`, ~3,000 newlines, then
//!   `rm -rf ~`.
//!
//! The approval card therefore never hands the shaper an unbounded word. It shows a preview built
//! here instead:
//!
//! * [`PreviewMode::Collapsed`] (the default) keeps every shell operator and separator character
//!   visible and highlighted (`; & | < > ( ) $ backtick quotes backslash { } #`). Newlines and
//!   every other short token stay visible too. The following are replaced by a marker that gives
//!   the hidden count:
//!   * the middle of an over-long word (more than [`LONG_RUN_CHARS`]);
//!   * the middle of a whitespace stretch (more than [`LONG_WHITESPACE_RUN_CHARS`]);
//!   * every run of [`BLANK_LINE_RUN_COLLAPSE`] or more blank lines.
//!
//!   If the result is still longer than [`COLLAPSED_PREVIEW_MAX_CHARS`] or
//!   [`COLLAPSED_PREVIEW_MAX_LINES`], the middle of the command is cut behind one marker. That
//!   marker gives the hidden count and how many separators it covers. Any elision makes
//!   [`CommandApprovalPreview::is_truncated`] true. The card then withholds Run until the user
//!   asks for the full command (see [`approval_requires_full_view`]). The action model refuses
//!   every user-initiated accept of such a command, whichever path it comes from.
//! * [`PreviewMode::Full`] shows every non-whitespace character of the command. Only whitespace
//!   stretches and blank-line runs stay collapsed, because they hide nothing else and would
//!   otherwise push the end out of sight. The card echoes the end in an "Ends with" note.
//!   A zero-width space is inserted wherever [`FULL_VIEW_BREAK_EVERY_CHARS`] display chars have
//!   gone by without a guaranteed break (a newline or an earlier insertion). Whitespace does
//!   not count as one, because UAX #14 keeps some spaces unbroken, and neither do operators,
//!   quotes, escapes or markers glued to letters (#864). The breaks are display-only, so no
//!   shaped word reaches the renderer's limit, and the text soft-wraps instead of clipping.
//!   U+200B is a
//!   Default_Ignorable code point, so the shaper hides it rather than drawing a missing-glyph box.
//!   `layout_line` in `fonts.rs` already relies on that. Still, this needs a manual check on a
//!   real X11 build with a 100K word.
//!
//! In both modes, the following are drawn as a highlighted `<U+XXXX>` escape, never passed
//! through:
//! * control characters;
//! * invisible format characters (bidi overrides, zero-width spaces, BOM, ...);
//! * non-ASCII whitespace, which a shell treats as part of a word;
//! * a third or later consecutive combining mark;
//! * a literal `<` that starts `<U+`, so typed text cannot pose as an escape.
//!
//! A command must not be able to hide part of itself from the person approving it.

use std::cell::OnceCell;

use unicode_general_category::{GeneralCategory, get_general_category};

/// Words longer than this many chars are elided in the collapsed preview. This is far below the
/// renderer's 16,384-char per-word limit.
pub(crate) const LONG_RUN_CHARS: usize = 160;
/// How many leading chars of an elided word stay visible.
const LONG_RUN_HEAD_CHARS: usize = 48;
/// How many trailing chars of an elided word stay visible.
const LONG_RUN_TAIL_CHARS: usize = 24;
/// Whitespace stretches (spaces/tabs) longer than this are elided in the collapsed preview, so
/// padding cannot push the rest of a line out of view.
pub(crate) const LONG_WHITESPACE_RUN_CHARS: usize = 32;
/// How many leading and trailing chars of an elided whitespace stretch stay visible.
const WHITESPACE_KEEP_CHARS: usize = 2;
/// This many or more consecutive blank lines are collapsed behind a marker.
pub(crate) const BLANK_LINE_RUN_COLLAPSE: usize = 3;
/// Upper bound on the collapsed preview's displayed length, in chars, before its middle is cut.
///
/// The collapsed preview is drawn with no height limit and no scroll container: with soft wrap,
/// laid-out height depends on the card width, which a char or line budget cannot bound. So
/// the card itself never clips it (the surrounding block list still scrolls). This budget, and
/// the line budget below, only keep the card a reasonable size.
pub(crate) const COLLAPSED_PREVIEW_MAX_CHARS: usize = 1_500;
/// Upper bound on the collapsed preview's line count before its middle is cut.
pub(crate) const COLLAPSED_PREVIEW_MAX_LINES: usize = 20;
/// Lines kept above a line-count cut.
const CUT_HEAD_LINES: usize = 12;
/// Lines kept below a line-count cut.
const CUT_TAIL_LINES: usize = 6;
/// In the full preview, a zero-width break opportunity is inserted after every this-many chars of
/// one run. The renderer shapes each piece separately, so the per-word limit is never reached.
const FULL_VIEW_BREAK_EVERY_CHARS: usize = 256;
/// Zero-width space: a line-break opportunity that draws nothing.
pub(crate) const ZERO_WIDTH_BREAK: char = '\u{200B}';

/// Characters that are always shown verbatim and highlighted, and are never folded into an
/// elided run, because they change what (or how many things) the shell runs.
fn is_operator_char(c: char) -> bool {
    matches!(
        c,
        ';' | '&' | '|' | '<' | '>' | '(' | ')' | '$' | '`' | '\'' | '"' | '\\' | '{' | '}' | '#'
    )
}

/// Combining marks (general categories Mn and Me). A long stack of them can paint over
/// neighbouring text, so after the second in a row each is escaped.
fn is_combining_mark(c: char) -> bool {
    matches!(
        get_general_category(c),
        GeneralCategory::NonspacingMark | GeneralCategory::EnclosingMark
    )
}

/// Characters that are written right to left (Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan,
/// Mandaic, their presentation forms, and the RTL supplementary blocks). Between ASCII operators
/// they reorder the neutrals and brackets around them, so the displayed order would no longer be
/// the order the shell reads.
fn is_strong_rtl(c: char) -> bool {
    matches!(
        c,
        '\u{0590}'..='\u{08FF}'
            | '\u{FB1D}'..='\u{FDFF}'
            | '\u{FE70}'..='\u{FEFF}'
            | '\u{10800}'..='\u{10FFF}'
            | '\u{1E800}'..='\u{1EFFF}'
    )
}

/// Characters that must never reach the screen raw:
/// * controls, other than `\n` and `\t`;
/// * format characters (category Cf), which are invisible and can hide or reorder text;
/// * line and paragraph separators, private-use and unassigned code points;
/// * non-ASCII whitespace, which looks like a word break but is part of a word to the shell;
/// * glyphs that render blank (Hangul fillers, braille blank, object replacement), and variation
///   selectors;
/// * fullwidth Latin letters, digits, and fullwidth look-alikes of shell operators. Everyday
///   CJK punctuation such as `，：！？` is left alone.
fn must_escape(c: char) -> bool {
    if c == '\n' || c == '\t' {
        return false;
    }
    if c.is_control() {
        return true;
    }
    match get_general_category(c) {
        GeneralCategory::Format
        | GeneralCategory::LineSeparator
        | GeneralCategory::ParagraphSeparator
        | GeneralCategory::PrivateUse
        | GeneralCategory::Surrogate => return true,
        GeneralCategory::SpaceSeparator if c != ' ' => return true,
        _ => {}
    }
    matches!(
        c,
        '\u{034F}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{2800}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FF02}'..='\u{FF04}'
            | '\u{FF06}'..='\u{FF0A}'
            | '\u{FF0D}'
            | '\u{FF0F}'..='\u{FF19}'
            | '\u{FF1B}'..='\u{FF1E}'
            | '\u{FF21}'..='\u{FF3A}'
            | '\u{FF3C}'
            | '\u{FF40}'..='\u{FF5E}'
            | '\u{FFA0}'
            | '\u{FFFC}'
            // Default_Ignorable code points that are unassigned (in some Unicode version this
            // crate's tables may predate) render as nothing, so they are escaped too. Ordinary
            // unassigned code points, such as emoji newer than the tables, are left alone.
            | '\u{2065}'
            | '\u{FFF0}'..='\u{FFF8}'
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
    /// Bounded length and line count. Long runs, blank-line runs and, if needed, the middle of
    /// the command are elided behind markers.
    Collapsed,
    /// Every non-whitespace character of the command. Only long whitespace and blank-line
    /// runs stay collapsed.
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
    /// Whether this piece stands only for whitespace (other than newlines). Used to find blank
    /// lines.
    blank: bool,
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
            blank: false,
        }
    }

    fn marker(text: String, hidden: usize) -> Self {
        let mut piece = Self::new(PieceKind::Marker, text, hidden);
        piece.hidden_chars = hidden;
        piece
    }

    fn blank(mut self, blank: bool) -> Self {
        self.blank = blank;
        self
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
    let (limit, head, tail) = if is_whitespace {
        (
            LONG_WHITESPACE_RUN_CHARS,
            WHITESPACE_KEEP_CHARS,
            WHITESPACE_KEEP_CHARS,
        )
    } else {
        (LONG_RUN_CHARS, LONG_RUN_HEAD_CHARS, LONG_RUN_TAIL_CHARS)
    };
    // Whitespace is elided in both modes: it hides nothing but whitespace, and in the full view
    // it would otherwise push the rest of the command out of sight. Words are elided only in the
    // collapsed preview.
    let elide = run.len() > limit && (is_whitespace || mode == PreviewMode::Collapsed);
    if elide {
        let hidden = run.len() - head - tail;
        pieces.push(
            Piece::new(PieceKind::Text, run[..head].iter().collect(), head).blank(is_whitespace),
        );
        let what = if is_whitespace {
            plural(hidden, "whitespace char", "whitespace chars")
        } else {
            plural(hidden, "char", "chars")
        };
        pieces.push(
            Piece::marker(
                format!("…[{} more {what}]…", group_thousands(hidden)),
                hidden,
            )
            .blank(is_whitespace),
        );
        pieces.push(
            Piece::new(
                PieceKind::Text,
                run[run.len() - tail..].iter().collect(),
                tail,
            )
            .blank(is_whitespace),
        );
    } else if mode == PreviewMode::Collapsed || is_whitespace {
        pieces.push(
            Piece::new(PieceKind::Text, run.iter().collect(), run.len()).blank(is_whitespace),
        );
    } else {
        // Full view: the whole word. Break opportunities are added when the pieces are joined,
        // in `command_approval_preview`, because a word that is too long for the shaper can span
        // several pieces.
        pieces.push(Piece::new(PieceKind::Text, run.iter().collect(), run.len()));
    }
}

/// Operators that reorder visually around RTL text and change what the shell runs.
fn is_reorderable_operator(c: char) -> bool {
    matches!(c, ';' | '&' | '|' | '<' | '>' | '(' | ')' | '$' | '`')
}

/// For each char of the command, whether its line contains a shell operator outside quotes. A
/// quote opened on one line carries over to the next, as it does in the shell.
fn rtl_escape_flags(chars: &[char]) -> Vec<bool> {
    let mut flags = vec![false; chars.len()];
    let (mut in_single, mut in_double, mut escaped) = (false, false, false);
    let mut line_start = 0;
    let mut line_has_operator = false;
    for (i, &c) in chars.iter().enumerate() {
        if escaped {
            escaped = false;
        } else if in_single {
            in_single = c != '\'';
        } else if c == '\\' {
            escaped = true;
        } else if c == '\'' && !in_double {
            in_single = true;
        } else if c == '"' {
            in_double = !in_double;
        } else if is_reorderable_operator(c) && (!in_double || matches!(c, '$' | '`')) {
            // `$` and backticks stay live inside double quotes (expansion, substitution).
            line_has_operator = true;
        }
        if c == '\n' || i + 1 == chars.len() {
            flags[line_start..=i].fill(line_has_operator);
            line_start = i + 1;
            line_has_operator = false;
        }
    }
    flags
}

fn tokenize(command: &str, mode: PreviewMode) -> Vec<Piece> {
    let chars: Vec<char> = command.chars().collect();
    // Right-to-left letters reorder the operators, neutrals and brackets around them, so the
    // displayed order would differ from the order the shell reads. The text element has no
    // base-direction override, so the RTL letters on a line with an unquoted shell operator are
    // escaped instead. RTL text on other lines, or only next to quoted operators, is drawn
    // as-is.
    let escape_rtl = rtl_escape_flags(&chars);
    let mut pieces = Vec::new();
    let mut run: Vec<char> = Vec::new();
    let mut run_is_whitespace = false;
    let mut combining_run = 0usize;

    for (i, &c) in chars.iter().enumerate() {
        combining_run = if is_combining_mark(c) {
            combining_run + 1
        } else {
            0
        };
        let poses_as_escape =
            c == '<' && chars.get(i + 1) == Some(&'U') && chars.get(i + 2) == Some(&'+');
        let escape = must_escape(c)
            || combining_run > 2
            || poses_as_escape
            || (escape_rtl[i] && is_strong_rtl(c));

        if !escape && c != '\n' && !is_operator_char(c) {
            let is_whitespace = c.is_whitespace();
            if !run.is_empty() && run_is_whitespace != is_whitespace {
                push_run(&mut pieces, &run, run_is_whitespace, mode);
                run.clear();
            }
            run_is_whitespace = is_whitespace;
            run.push(c);
            continue;
        }

        push_run(&mut pieces, &run, run_is_whitespace, mode);
        run.clear();
        if escape {
            pieces.push(Piece::new(PieceKind::Marker, escape_char(c), 1));
        } else if c == '\n' {
            pieces.push(Piece::new(PieceKind::Newline, "\n".to_owned(), 1));
        } else {
            pieces.push(Piece::new(PieceKind::Operator, c.to_string(), 1));
        }
    }
    push_run(&mut pieces, &run, run_is_whitespace, mode);
    pieces
}

/// Collapses every run of [`BLANK_LINE_RUN_COLLAPSE`] or more blank lines into
/// `\n…[N blank lines]…\n`.
fn collapse_blank_lines(pieces: Vec<Piece>) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::with_capacity(pieces.len());
    let mut pieces = pieces.into_iter().peekable();
    while let Some(piece) = pieces.next() {
        if piece.kind != PieceKind::Newline {
            out.push(piece);
            continue;
        }
        // A window that starts at this newline and holds only blank pieces and newlines.
        let mut window = vec![piece];
        while let Some(next) = pieces.peek() {
            if next.kind == PieceKind::Newline || next.blank {
                window.push(pieces.next().expect("peeked"));
            } else {
                break;
            }
        }
        // Trailing blank pieces after the last newline belong to the next line's indentation.
        let last_newline = window
            .iter()
            .rposition(|p| p.kind == PieceKind::Newline)
            .expect("window starts with a newline");
        let trailing = window.split_off(last_newline + 1);
        let blank_lines = window
            .iter()
            .filter(|p| p.kind == PieceKind::Newline)
            .count()
            - 1;
        if blank_lines >= BLANK_LINE_RUN_COLLAPSE {
            let last = window.pop().expect("last newline");
            let middle = window.split_off(1);
            out.append(&mut window);
            let hidden: usize = middle.iter().map(|p| p.command_chars).sum();
            out.push(Piece::marker(
                format!(
                    "…[{} blank {} hidden]…",
                    group_thousands(blank_lines),
                    plural(blank_lines, "line", "lines")
                ),
                hidden,
            ));
            out.push(last);
        } else {
            out.append(&mut window);
        }
        out.extend(trailing);
    }
    out
}

fn newline_count(pieces: &[Piece]) -> usize {
    pieces
        .iter()
        .filter(|p| p.kind == PieceKind::Newline)
        .count()
}

/// Replaces the middle of `pieces` with a single marker when the displayed text is longer than
/// [`COLLAPSED_PREVIEW_MAX_CHARS`] or spans more than [`COLLAPSED_PREVIEW_MAX_LINES`] lines. The
/// head gets about two thirds of the budget and the tail one third. So the end of the command,
/// where an appended payload sits, is always shown.
fn cut_middle(mut pieces: Vec<Piece>) -> Vec<Piece> {
    let total: usize = pieces.iter().map(Piece::display_chars).sum();
    if total <= COLLAPSED_PREVIEW_MAX_CHARS && newline_count(&pieces) < COLLAPSED_PREVIEW_MAX_LINES
    {
        return pieces;
    }
    let head_budget = COLLAPSED_PREVIEW_MAX_CHARS * 2 / 3;
    let tail_budget = COLLAPSED_PREVIEW_MAX_CHARS - head_budget;

    let fits = |used_chars: usize, used_lines: usize, piece: &Piece, chars: usize, lines: usize| {
        let piece_lines = usize::from(piece.kind == PieceKind::Newline);
        used_chars + piece.display_chars() <= chars && used_lines + piece_lines <= lines
    };

    let (mut head_end, mut chars, mut lines) = (0, 0, 0);
    while head_end < pieces.len()
        && fits(chars, lines, &pieces[head_end], head_budget, CUT_HEAD_LINES)
    {
        chars += pieces[head_end].display_chars();
        lines += usize::from(pieces[head_end].kind == PieceKind::Newline);
        head_end += 1;
    }
    let mut tail_start = pieces.len();
    (chars, lines) = (0, 0);
    while tail_start > head_end
        && fits(
            chars,
            lines,
            &pieces[tail_start - 1],
            tail_budget,
            CUT_TAIL_LINES,
        )
    {
        chars += pieces[tail_start - 1].display_chars();
        lines += usize::from(pieces[tail_start - 1].kind == PieceKind::Newline);
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
    let mut marker = Piece::marker(
        format!(
            " …[{} {} hidden here{separator_note}]… ",
            group_thousands(hidden),
            plural(hidden, "char", "chars")
        ),
        hidden,
    );
    marker.separators = separators;
    pieces.push(marker);
    pieces.extend(tail);
    pieces
}

/// Builds the approval-card display of `command`. See the module docs for the guarantees.
pub(crate) fn command_approval_preview(command: &str, mode: PreviewMode) -> CommandApprovalPreview {
    // Blank-line runs collapse in both modes: they hide no command content, and in the full
    // view they would otherwise push the rest of the command below the fold.
    let mut pieces = collapse_blank_lines(tokenize(command, mode));
    if mode == PreviewMode::Collapsed {
        pieces = cut_middle(pieces);
    }

    let mut preview = CommandApprovalPreview::default();
    let mut char_index = 0;
    // Display chars since the last guaranteed line break: a newline, or an inserted zero-width
    // space (UAX #14 LB8, `ZW SP* ÷`, always breaks after it). The shaper treats everything
    // between two breaks as one word, and in the full view a word can be any length. So a break
    // is inserted whenever this reaches `FULL_VIEW_BREAK_EVERY_CHARS`.
    //
    // Whitespace does NOT reset the count. A space is not always a break opportunity: LB14
    // `OP SP* ×`, LB15 `QU SP* × OP`, and LB17 `B2 SP* × B2` (em dashes) keep `( ( ( …`,
    // `"( "( …` and `— — …` unbroken across their spaces. cosmic-text also keeps interior
    // spaces inside such a segment. Nor do quotes, operators, escapes or markers glued to letters
    // break anything (`a"a"…`, `a;a;…`, #864). A break is never put between a base character
    // and its combining mark; it waits for the next base character.
    //
    // The collapsed preview needs no breaks: its whole display is bounded by
    // `COLLAPSED_PREVIEW_MAX_CHARS` plus a marker, far below the renderer's 16,384-char limit.
    let mut since_break = 0usize;
    for piece in &pieces {
        preview.hidden_chars += piece.hidden_chars;
        for c in piece.text.chars() {
            if c == '\n' || c == ZERO_WIDTH_BREAK {
                since_break = 0;
            } else {
                if mode == PreviewMode::Full
                    && since_break >= FULL_VIEW_BREAK_EVERY_CHARS
                    && !is_combining_mark(c)
                {
                    preview.text.push(ZERO_WIDTH_BREAK);
                    char_index += 1;
                    since_break = 0;
                }
                since_break += 1;
            }
            match piece.kind {
                PieceKind::Operator => preview.operator_char_indices.push(char_index),
                PieceKind::Marker => preview.marker_char_indices.push(char_index),
                PieceKind::Text | PieceKind::Newline => {}
            }
            preview.text.push(c);
            char_index += 1;
        }
    }
    preview
}

/// Whether a user's accept must be refused for `command`. True when the collapsed preview hides
/// part of it and the user has not yet chosen to see the full command. This is the check the
/// action model applies to every user-initiated accept of a `RequestCommandOutput`.
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

/// How many trailing chars of the command the "Ends with" note shows.
const ENDS_WITH_CHARS: usize = 120;

/// Whether the word mixes ASCII Latin letters with Cyrillic or Greek letters. Many of those look
/// exactly like ASCII letters (`а` U+0430 / `a`, `ο` U+03BF / `o`).
fn mixes_latin_with_lookalike_script(word: &str) -> bool {
    let has_latin = word.chars().any(|c| c.is_ascii_alphabetic());
    let has_lookalike = word
        .chars()
        .any(|c| matches!(c, '\u{0370}'..='\u{03FF}' | '\u{0400}'..='\u{052F}'));
    has_latin && has_lookalike
}

/// The "Ends with" note: the last [`ENDS_WITH_CHARS`] chars of the collapsed preview, with
/// newlines drawn as `↵`. It is taken from the preview of the whole command, so the same escapes
/// apply, and the collapsed preview always keeps the command's end. `None` for a short
/// single-line command, which the preview always shows whole.
fn ends_with_note(command: &str, collapsed: &CommandApprovalPreview) -> Option<String> {
    let trimmed = command.trim_end();
    if trimmed.chars().count() <= ENDS_WITH_CHARS && !trimmed.contains('\n') {
        return None;
    }
    let text = collapsed.text.trim_end();
    let chars = text.chars().count();
    let tail_start = text
        .char_indices()
        .nth(chars.saturating_sub(ENDS_WITH_CHARS))
        .map_or(0, |(i, _)| i);
    let tail = text[tail_start..].replace('\n', " ↵ ");
    let ellipsis = if tail_start > 0 { "…" } else { "" };
    Some(format!("Ends with: {ellipsis}{tail}"))
}

/// Everything the approval card draws for one command text, computed once per text change rather
/// than on every frame. The full preview is built lazily, the first time it is needed.
#[derive(Debug, Default)]
pub(crate) struct ApprovalPreviewCache {
    command: String,
    collapsed: CommandApprovalPreview,
    full: OnceCell<CommandApprovalPreview>,
    shape: CommandShapeSummary,
    command_chars: usize,
    has_non_ascii: bool,
    mixes_scripts: bool,
    has_escaped_rtl: bool,
    ends_with: Option<String>,
}

impl ApprovalPreviewCache {
    pub fn new(command: &str) -> Self {
        let collapsed = command_approval_preview(command, PreviewMode::Collapsed);
        let chars: Vec<char> = command.chars().collect();
        let has_escaped_rtl = rtl_escape_flags(&chars)
            .iter()
            .zip(&chars)
            .any(|(escape, c)| *escape && is_strong_rtl(*c));
        Self {
            command: command.to_owned(),
            ends_with: ends_with_note(command, &collapsed),
            collapsed,
            full: OnceCell::new(),
            shape: command_shape_summary(command),
            command_chars: command.chars().count(),
            has_non_ascii: !command.is_ascii(),
            mixes_scripts: command
                .split(|c: char| c.is_whitespace() || is_operator_char(c))
                .any(mixes_latin_with_lookalike_script),
            has_escaped_rtl,
        }
    }

    /// The command text this cache was built from.
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Whether the collapsed preview elides part of the command.
    pub fn is_truncated(&self) -> bool {
        self.collapsed.is_truncated()
    }

    /// Same answer as [`approval_requires_full_view`] for this command, without re-tokenizing.
    pub fn requires_full_view(&self, full_command_shown: bool) -> bool {
        !full_command_shown && self.is_truncated()
    }

    /// The preview to draw: the full one once the user asked for it and anything was elided,
    /// otherwise the collapsed one.
    pub fn preview(&self, full_command_shown: bool) -> &CommandApprovalPreview {
        if full_command_shown && self.is_truncated() {
            self.full
                .get_or_init(|| command_approval_preview(&self.command, PreviewMode::Full))
        } else {
            &self.collapsed
        }
    }

    /// One-line notes shown above the command on the approval card, in display order.
    pub fn notes(&self, full_command_shown: bool) -> Vec<String> {
        let mut notes = Vec::new();
        if self.is_truncated() {
            let hidden = self.collapsed.hidden_chars;
            if full_command_shown {
                notes.push(format!(
                    "Showing the full command ({} chars; long blank runs stay collapsed). Scroll \
                     to its end to enable Run.",
                    group_thousands(self.command_chars)
                ));
            } else {
                notes.push(format!(
                    "{} {} of this command {} hidden. Press Enter or Show full command to see \
                     all of it; Run is enabled after that.",
                    group_thousands(hidden),
                    plural(hidden, "char", "chars"),
                    plural(hidden, "is", "are"),
                ));
            }
        }
        if let Some(ends_with) = &self.ends_with {
            notes.push(ends_with.clone());
        }
        if self.shape.commands > 1 {
            notes.push(format!("This runs {} commands.", self.shape.commands));
        }
        if self.shape.has_command_substitution {
            notes.push("This contains command substitution ($(…) or backticks).".to_owned());
        }
        if self.mixes_scripts {
            notes.push(
                "This contains words that mix Latin with Cyrillic or Greek letters, which can \
                 look like ASCII."
                    .to_owned(),
            );
        } else if self.has_non_ascii {
            notes.push("This contains non-ASCII characters.".to_owned());
        }
        if self.has_escaped_rtl {
            notes.push(
                "Right-to-left letters are shown as <U+XXXX> so they cannot reorder the command."
                    .to_owned(),
            );
        }
        notes
    }
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
