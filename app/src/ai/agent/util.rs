use super::{
    AIAgentTextSection, AgentOutputImage, AgentOutputImageLayout, AgentOutputMermaidDiagram,
    AgentOutputTable, AgentOutputText, FormattedTextWrapper, ProgrammingLanguage,
};
use crate::code::editor_management::CodeSource;
use crate::features::FeatureFlag;
use ai::gfm_table::{format_gfm_table, maybe_collect_gfm_table_lines};
use itertools::Itertools;
use lazy_static::lazy_static;
use markdown_parser::{
    parse_image_run_line, parse_markdown_tail, parse_markdown_with_gfm_tables, FormattedImage,
    FormattedText, FormattedTextLine, MarkdownResumePoint, MarkdownTail,
};
use mermaid_to_svg::is_mermaid_diagram;
use regex::Regex;
use std::{collections::HashMap, path::PathBuf};
use warp_util::path::LineAndColumnArg;

lazy_static! {
    /// Markdown prefix for code blocks. Matches on triple backticks followed by a language.
    /// Importantly, parameters for linked code blocks are captured into their own group.
    static ref CODE_START_REGEX: Regex = Regex::new(r"^\s*```([\w+-]*)(.*)$").expect("Regex is valid");

    /// Markdown suffix for code blocks.
    static ref CODE_END_REGEX: Regex = Regex::new(r"^\s*```\s*$").expect("Regex is valid");

    /// Extracts key-value parameters from text in the format: key=value, used for code block metadata.
    /// Expects to match on text with format path=/path/to/file start=<line_number>
    static ref CODE_PARAMS_REGEX: Regex = Regex::new(r"(\w+)=([^\s]+)").expect("Regex is valid");
}

/// Converts the given `markdown_text` into corresponding `Text` and `Code` `AIAgentOutputStep`s.
pub(super) fn parse_markdown_into_text_and_code_sections(
    markdown_text: &str,
) -> Vec<AIAgentTextSection> {
    let mut sections = vec![];
    let mut current_section = CurrentSection::PlainText(String::new());

    let mut lines = markdown_text.lines().peekable();
    while let Some(line) = lines.next() {
        match &mut current_section {
            CurrentSection::PlainText(text) => {
                // Detect tables and render them as formatted table sections.
                if let Some(table_lines) = maybe_collect_gfm_table_lines(line, &mut lines, |l| {
                    CODE_START_REGEX.is_match(l)
                }) {
                    let markdown_source = table_lines.join("\n");
                    let table_section = if FeatureFlag::BlocklistMarkdownTableRendering.is_enabled()
                    {
                        parse_agent_output_table(&markdown_source)
                    } else {
                        Some(AgentOutputTable::legacy(format_gfm_table(&table_lines)))
                    };
                    if let Some(table_section) = table_section {
                        if !text.is_empty() {
                            flush_plain_text_sections(text, &mut sections);
                            text.clear();
                        }
                        sections.push(AIAgentTextSection::Table {
                            table: table_section,
                        });
                        continue;
                    }

                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(&markdown_source);
                    continue;
                }

                if let Some((_, [language, param_str])) = CODE_START_REGEX
                    .captures(line)
                    .map(|capture_group| capture_group.extract())
                {
                    if !text.is_empty() {
                        flush_plain_text_sections(text, &mut sections);
                    }

                    let source = code_source_from_params(param_str);
                    current_section = CurrentSection::Code {
                        code: String::new(),
                        language_token: Some(language.to_owned()).filter(|l| !l.is_empty()),
                        language: Some(language)
                            .filter(|l| !l.is_empty())
                            .map(|l| l.to_owned().into()),
                        source,
                    };
                } else {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(line);
                }
            }
            CurrentSection::Code {
                code,
                language,
                language_token,
                source,
            } => {
                if CODE_END_REGEX.is_match(line) {
                    if !code.is_empty() {
                        push_closed_code_section(
                            std::mem::take(code),
                            language.clone(),
                            language_token.as_deref(),
                            source.take(),
                            &mut sections,
                        );
                    }
                    current_section = CurrentSection::PlainText(String::new());
                } else {
                    if !code.is_empty() {
                        code.push('\n');
                    }
                    code.push_str(line);
                }
            }
        }
    }

    match current_section {
        CurrentSection::PlainText(text) => {
            flush_plain_text_sections(&text, &mut sections);
        }
        CurrentSection::Code {
            code,
            language,
            language_token,
            source,
        } => {
            push_code_or_mermaid_section(
                code,
                language,
                language_token.as_deref(),
                source,
                &mut sections,
            );
        }
    }

    if sections.is_empty() {
        sections.push(AIAgentTextSection::PlainText {
            text: String::new().into(),
        });
    }
    sections
}

/// A cheap fingerprint of `text[..len]`: its length and its last 64 bytes. It is not a proof
/// that the text is unchanged — callers also invalidate on every path that replaces a message —
/// but it catches a replaced message without hashing the whole answer on every chunk.
fn source_fingerprint(text: &str, len: usize) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    len.hash(&mut hasher);
    text.as_bytes()[len.saturating_sub(64)..len].hash(&mut hasher);
    hasher.finish()
}

/// Reads the `path=... start=...` parameters of a linked code block's opening fence.
fn code_source_from_params(param_str: &str) -> Option<CodeSource> {
    let mut params = HashMap::new();
    for (_, [key, value]) in CODE_PARAMS_REGEX
        .captures_iter(param_str)
        .map(|c| c.extract())
    {
        params.insert(key, value);
    }
    match (params.get("path"), params.get("start")) {
        (Some(path), Some(start)) => start
            .parse::<usize>()
            .ok()
            .map(|line_num| CodeSource::Link {
                path: PathBuf::from(path),
                range_start: Some(LineAndColumnArg {
                    line_num,
                    column_num: None,
                }),
                range_end: None,
            }),
        _ => None,
    }
}

/// Pushes a code block whose closing fence was reached, completing a linked block's line range.
fn push_closed_code_section(
    code: String,
    language: Option<ProgrammingLanguage>,
    language_token: Option<&str>,
    mut source: Option<CodeSource>,
    sections: &mut Vec<AIAgentTextSection>,
) {
    if let Some(CodeSource::Link {
        range_start: Some(start),
        range_end,
        ..
    }) = source.as_mut()
    {
        *range_end = Some(LineAndColumnArg {
            line_num: start.line_num + code.lines().count() - 1,
            column_num: None,
        });
    }
    push_code_or_mermaid_section(code, language, language_token, source, sections);
}

fn parse_agent_output_table(markdown_source: &str) -> Option<AgentOutputTable> {
    let formatted_text = parse_markdown_with_gfm_tables(markdown_source).ok()?;
    let table = formatted_text
        .lines
        .into_iter()
        .exactly_one()
        .ok()
        .and_then(|line| match line {
            FormattedTextLine::Table(table) => Some(table),
            _ => None,
        })?;
    Some(AgentOutputTable::structured(
        markdown_source.to_owned(),
        table,
    ))
}

enum CurrentSection {
    PlainText(String),
    Code {
        code: String,
        language_token: Option<String>,
        language: Option<ProgrammingLanguage>,
        source: Option<CodeSource>,
    },
}

fn flush_plain_text_sections(markdown_text: &str, sections: &mut Vec<AIAgentTextSection>) {
    if markdown_text.is_empty() {
        return;
    }

    let mut plain_text = String::new();
    for line in markdown_text.split_inclusive('\n') {
        if let Some(images) = parse_image_run_line(line) {
            if !plain_text.is_empty() {
                sections.push(AIAgentTextSection::PlainText {
                    text: std::mem::take(&mut plain_text).into(),
                });
            }

            if images.len() == 1 {
                if let Some(image) = images.into_iter().next() {
                    sections.push(image_section(image, AgentOutputImageLayout::Block));
                }
            } else {
                sections.extend(
                    images
                        .into_iter()
                        .map(|image| image_section(image, AgentOutputImageLayout::Inline)),
                );
            }
        } else {
            plain_text.push_str(line);
        }
    }

    if !plain_text.is_empty() {
        sections.push(AIAgentTextSection::PlainText {
            text: plain_text.into(),
        });
    }
}
fn image_section(image: FormattedImage, layout: AgentOutputImageLayout) -> AIAgentTextSection {
    AIAgentTextSection::Image {
        image: AgentOutputImage {
            markdown_source: markdown_source_for_image(&image),
            alt_text: image.alt_text,
            source: image.source,
            title: image.title,
            layout,
        },
    }
}

fn markdown_source_for_image(image: &FormattedImage) -> String {
    warp_editor::content::text::format_image_markdown(
        &image.alt_text,
        &image.source,
        image.title.as_deref(),
    )
}

fn markdown_source_for_mermaid(source: &str) -> String {
    format!("```mermaid\n{source}\n```")
}

fn push_code_or_mermaid_section(
    code: String,
    language: Option<ProgrammingLanguage>,
    language_token: Option<&str>,
    source: Option<CodeSource>,
    sections: &mut Vec<AIAgentTextSection>,
) {
    if code.is_empty() {
        return;
    }

    if language_token.is_some_and(is_mermaid_diagram) {
        sections.push(AIAgentTextSection::MermaidDiagram {
            diagram: AgentOutputMermaidDiagram {
                markdown_source: markdown_source_for_mermaid(&code),
                source: code,
            },
        });
    } else {
        sections.push(AIAgentTextSection::Code {
            code,
            language,
            source,
        });
    }
}

/// The append-only, incremental counterpart of [`parse_markdown_into_text_and_code_sections`],
/// for an agent answer that is still streaming in (#849).
///
/// Converting the whole accumulated answer on every chunk is quadratic: a 10 MB answer streamed
/// in 4 KB chunks froze the main thread for minutes, re-parsing Markdown that had not changed. An
/// answer only ever grows at its end, so almost all of what the previous chunk produced is still
/// right. This keeps a *commit point* — a line boundary before which nothing can change however
/// the answer continues — together with the splitter's state there, and each update re-derives
/// only what follows it:
///
/// - sections finished before the commit point are kept as they are;
/// - the plain-text piece still open at the commit point keeps its parsed Markdown lines up to a
///   [`MarkdownResumePoint`], and only its tail is re-parsed;
/// - the source lines after the commit point (normally the last complete line and the partial one
///   being streamed) go through the same steps as the full conversion.
///
/// The result after every update equals `parse_markdown_into_text_and_code_sections(text)`.
/// [`Self::update`] edits the previous result in place, so the parsing work per chunk is bounded
/// by the chunk and the lines around it, not by the answer. Other O(answer) costs remain outside
/// it (#872): when a renderer still holds the open piece's parsed text, extending it copies it
/// once (`Arc::make_mut`), and a table or a single line that is still streaming is re-parsed
/// whole on every chunk. For a table that includes its Markdown: while its last row is partial
/// the full conversion rejects the table and renders its lines as plain text, which is parsed
/// whole (a table's verdict depends on its last row, so its rows cannot be committed early
/// without changing what is shown).
///
/// The feature flags that shape the result (`BlocklistMarkdownTableRendering`,
/// `MarkdownTables`) are read on each update. Toggling one mid-stream leaves the sections
/// committed before the toggle as they were converted; the full re-conversion when the
/// exchange finishes (`Task::finalize_streamed_text`) brings them in line.
#[derive(Debug, Clone, Default)]
pub(crate) struct IncrementalTextSections {
    commit: SectionsCommit,
    /// Bytes handed to the Markdown block parser across all updates. Only tests read it.
    #[cfg_attr(not(test), allow(dead_code))]
    markdown_bytes_parsed: usize,
    /// Source lines handed to the section splitter across all updates. Only tests read it.
    #[cfg_attr(not(test), allow(dead_code))]
    lines_split: usize,
}

/// The splitter's state at a line boundary that later text cannot reinterpret.
#[derive(Debug, Clone, Default)]
struct SectionsCommit {
    /// Source bytes before the commit point; always ends with `\n`.
    source_len: usize,
    /// [`source_fingerprint`] of the text before `source_len`. A caller that hands over text
    /// that is not an extension of the last one — a message replaced under the same id — is
    /// caught by this rather than having stale sections spliced into its new text.
    fingerprint: u64,
    /// Leading output sections that are final.
    sections: usize,
    mode: CommittedMode,
}

#[derive(Debug, Clone)]
enum CommittedMode {
    Plain(CommittedPlain),
    Code(CommittedCode),
}

impl Default for CommittedMode {
    fn default() -> Self {
        CommittedMode::Plain(CommittedPlain::default())
    }
}

/// Plain text at the commit point. Its open piece is not stored: it is the start of the
/// `PlainText` section right after the final sections, which every update leaves in the output.
#[derive(Debug, Clone, Default)]
struct CommittedPlain {
    /// Whether any line has been added to the plain-text buffer since it was last flushed.
    buffer_nonempty: bool,
    /// Whether the last buffered line went into the open piece (rather than being an image).
    last_line_plain: bool,
    /// Length of the open piece's Markdown at the commit point.
    piece_len: usize,
    /// Where re-parsing the open piece's Markdown may resume.
    piece_resume: MarkdownResumePoint,
}

/// A code block open at the commit point. Its text so far is the start of the code section
/// right after the final sections.
#[derive(Debug, Clone)]
struct CommittedCode {
    code_len: usize,
    language_token: Option<String>,
    language: Option<ProgrammingLanguage>,
    source: Option<CodeSource>,
}

/// The splitter's working state while an update runs.
enum WorkingMode {
    Plain(WorkingPlain),
    Code(WorkingCode),
}

struct WorkingPlain {
    buffer_nonempty: bool,
    last_line_plain: bool,
    piece: OpenPiece,
}

/// The plain text accumulated since the last image line of the current plain-text run.
struct OpenPiece {
    /// Distinguishes the pieces of one update, to tell whether the commit point is in this one.
    id: usize,
    text: String,
    /// Parsed lines of an earlier, shorter version of `text`, valid up to `resume`.
    reuse: Option<FormattedTextWrapper>,
    resume: MarkdownResumePoint,
}

impl OpenPiece {
    fn new(id: usize) -> Self {
        Self {
            id,
            text: String::new(),
            reuse: None,
            resume: MarkdownResumePoint::default(),
        }
    }
}

struct WorkingCode {
    code: String,
    language_token: Option<String>,
    language: Option<ProgrammingLanguage>,
    source: Option<CodeSource>,
}

/// The newest commit point of an update, and which piece its open plain text belongs to.
struct PendingCommit {
    commit: SectionsCommit,
    piece_id: usize,
}

impl IncrementalTextSections {
    /// Brings `sections` up to date with `text`.
    ///
    /// `text` must extend the text of the previous update, and `sections` must be what that
    /// update left (or anything, on the first update). When `sections` turns out not to match,
    /// this starts over and converts `text` from scratch, which is never wrong, only slower.
    pub(crate) fn update(&mut self, text: &str, sections: &mut Vec<AIAgentTextSection>) {
        if self.try_update(text, sections) {
            return;
        }
        self.commit = SectionsCommit::default();
        sections.clear();
        let restarted = self.try_update(text, sections);
        debug_assert!(restarted, "updating from the start cannot fail");
    }

    /// Bytes handed to the Markdown block parser so far.
    #[cfg(test)]
    pub(crate) fn markdown_bytes_parsed(&self) -> usize {
        self.markdown_bytes_parsed
    }

    /// Source lines handed to the section splitter so far.
    #[cfg(test)]
    pub(crate) fn lines_split(&self) -> usize {
        self.lines_split
    }

    fn try_update(&mut self, text: &str, sections: &mut Vec<AIAgentTextSection>) -> bool {
        let commit = std::mem::take(&mut self.commit);
        if text.len() < commit.source_len
            || !text.is_char_boundary(commit.source_len)
            || sections.len() < commit.sections
            || (commit.source_len > 0
                && source_fingerprint(text, commit.source_len) != commit.fingerprint)
        {
            return false;
        }
        let Some(mut mode) = Self::reopen(&commit, sections) else {
            return false;
        };
        debug_assert_eq!(sections.len(), commit.sections);

        let start = commit.source_len;
        let tail = &text[start..];
        // `str::lines` is `split_inclusive('\n')` with the line ending stripped, so the two
        // iterators stay in step; the first gives the content, the second where each line ends.
        let lines: Vec<&str> = tail.lines().collect();
        let line_ends: Vec<usize> = tail
            .split_inclusive('\n')
            .scan(start, |end, segment| {
                *end += segment.len();
                Some(*end)
            })
            .collect();
        debug_assert_eq!(lines.len(), line_ends.len());
        let is_complete = |index: usize| {
            line_ends
                .get(index)
                .is_some_and(|end| text.as_bytes()[end - 1] == b'\n')
        };
        self.lines_split += lines.len();

        let mut pending = PendingCommit {
            commit,
            piece_id: 0,
        };
        let mut next_piece_id = 1;
        let mut index = 0;
        while index < lines.len() {
            let line = lines[index];
            let mut consumed = 1;
            let mut next_mode = None;
            match &mut mode {
                WorkingMode::Plain(plain) => {
                    let mut rest = lines[index + 1..].iter().copied().peekable();
                    let rest_before = rest.len();
                    let table_lines = maybe_collect_gfm_table_lines(line, &mut rest, |l| {
                        CODE_START_REGEX.is_match(l)
                    });
                    // Lines the table detection took, including a separator row it consumed
                    // before rejecting the table — which the full conversion skips too.
                    consumed += rest_before - rest.len();

                    if let Some(table_lines) = table_lines {
                        let markdown_source = table_lines.join("\n");
                        let table_section =
                            if FeatureFlag::BlocklistMarkdownTableRendering.is_enabled() {
                                parse_agent_output_table(&markdown_source)
                            } else {
                                Some(AgentOutputTable::legacy(format_gfm_table(&table_lines)))
                            };
                        if let Some(table_section) = table_section {
                            if plain.buffer_nonempty {
                                let piece = std::mem::replace(
                                    &mut plain.piece,
                                    OpenPiece::new(next_piece_id),
                                );
                                next_piece_id += 1;
                                self.finish_piece(piece, sections, &mut pending);
                                plain.buffer_nonempty = false;
                                plain.last_line_plain = false;
                            }
                            sections.push(AIAgentTextSection::Table {
                                table: table_section,
                            });
                        } else {
                            for table_line in &table_lines {
                                self.push_plain_line(
                                    plain,
                                    table_line,
                                    sections,
                                    &mut pending,
                                    &mut next_piece_id,
                                );
                            }
                        }
                    } else if let Some((_, [language, param_str])) = CODE_START_REGEX
                        .captures(line)
                        .map(|capture_group| capture_group.extract())
                    {
                        if plain.buffer_nonempty {
                            let piece =
                                std::mem::replace(&mut plain.piece, OpenPiece::new(next_piece_id));
                            next_piece_id += 1;
                            self.finish_piece(piece, sections, &mut pending);
                        }
                        next_mode = Some(WorkingMode::Code(WorkingCode {
                            code: String::new(),
                            language_token: Some(language.to_owned()).filter(|l| !l.is_empty()),
                            language: Some(language)
                                .filter(|l| !l.is_empty())
                                .map(|l| l.to_owned().into()),
                            source: code_source_from_params(param_str),
                        }));
                    } else {
                        self.push_plain_line(
                            plain,
                            line,
                            sections,
                            &mut pending,
                            &mut next_piece_id,
                        );
                    }
                }
                WorkingMode::Code(code) => {
                    if CODE_END_REGEX.is_match(line) {
                        if !code.code.is_empty() {
                            push_closed_code_section(
                                std::mem::take(&mut code.code),
                                code.language.clone(),
                                code.language_token.as_deref(),
                                code.source.take(),
                                sections,
                            );
                        }
                        next_mode = Some(WorkingMode::Plain(WorkingPlain {
                            buffer_nonempty: false,
                            last_line_plain: false,
                            piece: OpenPiece::new(next_piece_id),
                        }));
                        next_piece_id += 1;
                    } else {
                        if !code.code.is_empty() {
                            code.code.push('\n');
                        }
                        code.code.push_str(line);
                    }
                }
            }
            if let Some(next_mode) = next_mode {
                mode = next_mode;
            }
            index += consumed;

            // A new commit point needs every line this step read to be complete, and the line
            // after them too: table detection peeks at it, and whether it ends a table depends
            // on all of it.
            if is_complete(index - 1) && is_complete(index) {
                pending = PendingCommit {
                    commit: SectionsCommit {
                        source_len: line_ends[index - 1],
                        fingerprint: source_fingerprint(text, line_ends[index - 1]),
                        sections: sections.len(),
                        mode: match &mode {
                            WorkingMode::Plain(plain) => CommittedMode::Plain(CommittedPlain {
                                buffer_nonempty: plain.buffer_nonempty,
                                last_line_plain: plain.last_line_plain,
                                piece_len: plain.piece.text.len(),
                                // Refined when the piece is parsed, which is after this point.
                                piece_resume: plain.piece.resume.clone(),
                            }),
                            WorkingMode::Code(code) => CommittedMode::Code(CommittedCode {
                                code_len: code.code.len(),
                                language_token: code.language_token.clone(),
                                language: code.language.clone(),
                                source: code.source.clone(),
                            }),
                        },
                    },
                    piece_id: match &mode {
                        WorkingMode::Plain(plain) => plain.piece.id,
                        WorkingMode::Code(_) => usize::MAX,
                    },
                };
            }
        }

        match mode {
            WorkingMode::Plain(plain) => self.finish_piece(plain.piece, sections, &mut pending),
            WorkingMode::Code(code) => push_code_or_mermaid_section(
                code.code,
                code.language,
                code.language_token.as_deref(),
                code.source,
                sections,
            ),
        }
        if sections.is_empty() {
            sections.push(AIAgentTextSection::PlainText {
                text: String::new().into(),
            });
        }

        self.commit = pending.commit;
        true
    }

    /// Takes the section that holds the state open at the commit point out of `sections`,
    /// dropping everything after the final sections. `None` when `sections` does not have the
    /// shape the commit point expects.
    fn reopen(
        commit: &SectionsCommit,
        sections: &mut Vec<AIAgentTextSection>,
    ) -> Option<WorkingMode> {
        match &commit.mode {
            CommittedMode::Plain(plain) => {
                let piece = if plain.piece_len == 0 {
                    sections.truncate(commit.sections);
                    OpenPiece::new(0)
                } else {
                    sections.truncate(commit.sections + 1);
                    if sections.len() != commit.sections + 1 {
                        return None;
                    }
                    let Some(AIAgentTextSection::PlainText { text }) = sections.pop() else {
                        return None;
                    };
                    let AgentOutputText {
                        formatted_lines,
                        mut markdown_text,
                    } = text;
                    if !markdown_text.is_char_boundary(plain.piece_len) {
                        return None;
                    }
                    markdown_text.truncate(plain.piece_len);
                    // `parse_markdown` trims a trailing line break, so the kept lines may
                    // include one more than the stored text has; `replace_tail` restores it.
                    let (reuse, resume) = match formatted_lines {
                        Some(formatted)
                            if formatted.lines().len() + 1 >= plain.piece_resume.line_count() =>
                        {
                            (Some(formatted), plain.piece_resume.clone())
                        }
                        _ => (None, MarkdownResumePoint::default()),
                    };
                    OpenPiece {
                        id: 0,
                        text: markdown_text,
                        reuse,
                        resume,
                    }
                };
                Some(WorkingMode::Plain(WorkingPlain {
                    buffer_nonempty: plain.buffer_nonempty,
                    last_line_plain: plain.last_line_plain,
                    piece,
                }))
            }
            CommittedMode::Code(code) => {
                let mut code_text = if code.code_len == 0 {
                    sections.truncate(commit.sections);
                    String::new()
                } else {
                    sections.truncate(commit.sections + 1);
                    if sections.len() != commit.sections + 1 {
                        return None;
                    }
                    match sections.pop() {
                        Some(AIAgentTextSection::Code { code, .. }) => code,
                        Some(AIAgentTextSection::MermaidDiagram { diagram }) => diagram.source,
                        _ => return None,
                    }
                };
                if !code_text.is_char_boundary(code.code_len) {
                    return None;
                }
                code_text.truncate(code.code_len);
                Some(WorkingMode::Code(WorkingCode {
                    code: code_text,
                    language_token: code.language_token.clone(),
                    language: code.language.clone(),
                    source: code.source.clone(),
                }))
            }
        }
    }

    /// Adds one line to the plain-text buffer — the incremental form of the full conversion
    /// appending it to its buffer and `flush_plain_text_sections` later splitting the buffer at
    /// image lines. Images are split off as their lines arrive, which yields the same sections
    /// in the same order: nothing else is pushed while a plain-text run is open.
    fn push_plain_line(
        &mut self,
        plain: &mut WorkingPlain,
        line: &str,
        sections: &mut Vec<AIAgentTextSection>,
        pending: &mut PendingCommit,
        next_piece_id: &mut usize,
    ) {
        if !plain.buffer_nonempty {
            // The full conversion only adds a newline between lines of a non-empty buffer, so
            // an empty line arriving at an empty buffer leaves no trace.
            if line.is_empty() {
                return;
            }
            plain.buffer_nonempty = true;
        } else if plain.last_line_plain {
            // The previous line was not the buffer's last after all; it gets its newline now.
            plain.piece.text.push('\n');
        }

        // The full conversion classifies `line` with its newline unless it is the buffer's last
        // line; `parse_image_run_line` ignores trailing whitespace, so that makes no difference.
        if let Some(images) = parse_image_run_line(line) {
            let piece = std::mem::replace(&mut plain.piece, OpenPiece::new(*next_piece_id));
            *next_piece_id += 1;
            self.finish_piece(piece, sections, pending);
            if images.len() == 1 {
                if let Some(image) = images.into_iter().next() {
                    sections.push(image_section(image, AgentOutputImageLayout::Block));
                }
            } else {
                sections.extend(
                    images
                        .into_iter()
                        .map(|image| image_section(image, AgentOutputImageLayout::Inline)),
                );
            }
            plain.last_line_plain = false;
        } else {
            plain.piece.text.push_str(line);
            plain.last_line_plain = true;
        }
    }

    /// Pushes a finished plain-text piece as a `PlainText` section, parsing only the part of its
    /// Markdown after its resume point. Equals `AgentOutputText::from(piece.text)`.
    fn finish_piece(
        &mut self,
        piece: OpenPiece,
        sections: &mut Vec<AIAgentTextSection>,
        pending: &mut PendingCommit,
    ) {
        let OpenPiece {
            id,
            text,
            reuse,
            resume,
        } = piece;
        if text.is_empty() {
            return;
        }

        // Only the Markdown the commit point vouches for may be resumed from next time.
        let committed_plain = match &mut pending.commit.mode {
            CommittedMode::Plain(plain) if pending.piece_id == id => Some(plain),
            _ => None,
        };
        let final_len = committed_plain.as_ref().map_or(0, |plain| plain.piece_len);

        self.markdown_bytes_parsed += text.len().saturating_sub(resume.offset());
        let (formatted_lines, next_resume) = match parse_markdown_tail(
            &text,
            &resume,
            super::agent_markdown_parses_gfm_tables(),
            final_len,
        ) {
            Ok(MarkdownTail {
                lines,
                resume: next_resume,
            }) => {
                let formatted = match reuse {
                    Some(mut formatted) => {
                        formatted.replace_tail(resume.line_count(), lines);
                        formatted
                    }
                    None => {
                        let mut lines = lines;
                        if let Some(FormattedTextLine::LineBreak) = lines.last() {
                            lines.pop();
                        }
                        FormattedText::new(lines).into()
                    }
                };
                (Some(formatted), next_resume)
            }
            Err(_) => (None, MarkdownResumePoint::default()),
        };
        if let Some(plain) = committed_plain {
            plain.piece_resume = next_resume;
        }

        sections.push(AIAgentTextSection::PlainText {
            text: AgentOutputText {
                formatted_lines,
                markdown_text: text,
            },
        });
    }
}

#[cfg(test)]
#[path = "util_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "util_incremental_tests.rs"]
mod incremental_tests;
