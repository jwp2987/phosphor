//! Batch `ReadFileContext` (parity family a): what to fetch for each file, how to fetch it on a
//! POSIX or PowerShell host, and how to turn what was fetched into the response.
//!
//! The helper (`daemon/file_context.rs`) and the shell backend (`backend/shell/read.rs`) differ
//! only in HOW they gather bytes: the helper reads its own filesystem, the shell backend runs the
//! builders below over the out-of-band channel. Everything that decides what the model sees (the
//! per-file and batch byte caps, line ranges, the truncation marker `line_range_*` that
//! `apply_diff` relies on, line-ending normalisation, binary detection, `failed_files` wording)
//! is [`assemble`], one function both feed. That is what keeps the two arms identical, and what
//! the contract tests check.
//!
//! The text semantics are those of the app's `read_local_file_context` (`FileModel::
//! read_text_file` plus its accumulator): lines are split on `\n`, `\r\n` is normalised to `\n`,
//! a byte budget cuts the read at a LINE boundary and marks the segment with the range that was
//! actually returned, and `line_count` is the whole file's. Differences, all deliberate:
//!
//! * Only a bounded prefix of a big file is fetched ([`GatherPlan`]), so invalid UTF-8 beyond that
//!   prefix is not noticed (the file is returned as truncated text, not demoted to binary).
//! * A line range deeper than [`RANGE_SCAN_CAP_BYTES`] into a file is a per-file failure.
//! * Images come back as plain binary: resizing for the model is the app's job and happens there.
//! * Modification times of shell-read files have one-second resolution.

use std::ops::Range;

use super::base64::Base64Tool;
use super::frame::ParsedSection;
use super::sh_quote;
use crate::proto::{
    FailedFileRead, FileContextProto, FileOperationError, ReadFileContextFile,
    ReadFileContextRequest, ReadFileContextResponse, file_context_proto,
};

/// Per-file byte limit when the request names none (the app's `MAX_FILE_READ_BYTES`).
pub const DEFAULT_MAX_FILE_BYTES: usize = 1_000_000;
/// The most bytes fetched for one file's content, whatever the caps ask for: the shell channel
/// returns at most [`super::MAX_OUTPUT_BYTES`] and base64 inflates by a third.
pub const MAX_GATHER_BYTES: usize = 6 * 1024 * 1024;
/// The most bytes scanned to reach the end of a requested line range.
pub const RANGE_SCAN_CAP_BYTES: usize = 6 * 1024 * 1024;
/// How much of a file's start decides text vs binary for an extensionless name.
pub const SNIFF_BYTES: usize = 1024;

/// What to fetch for one requested file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GatherPlan {
    /// Fetch at most this many leading bytes.
    pub prefix_bytes: usize,
    /// A ranged read: stop after this many lines (the end of the last requested range) instead.
    pub through_line: Option<usize>,
}

/// The per-file and batch caps a request implies.
pub fn caps(request: &ReadFileContextRequest) -> (usize, Option<usize>) {
    (
        request
            .max_file_bytes
            .map_or(DEFAULT_MAX_FILE_BYTES, |b| b as usize),
        request.max_batch_bytes.map(|b| b as usize),
    )
}

/// What to fetch for `file`. A whole-file read needs `2 * cap + 4` bytes to be sure of seeing
/// the line that crosses the cap (a CRLF line costs 2 file bytes but 1 budget byte).
pub fn plan(file: &ReadFileContextFile, request: &ReadFileContextRequest) -> GatherPlan {
    // A line range means nothing for a file read as binary, which is returned whole.
    let ranged = !has_binary_extension(&file.path);
    if let Some(end) = ranged
        .then(|| file.line_ranges.iter().map(|r| r.end as usize).max())
        .flatten()
    {
        return GatherPlan {
            prefix_bytes: RANGE_SCAN_CAP_BYTES,
            through_line: Some(end),
        };
    }
    let (per_file, batch) = caps(request);
    let bound = per_file.min(batch.unwrap_or(usize::MAX));
    GatherPlan {
        prefix_bytes: bound
            .saturating_mul(2)
            .saturating_add(4)
            .min(MAX_GATHER_BYTES),
        through_line: None,
    }
}

/// What a backend learned about one file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileProbe {
    NotFound,
    /// The reason, as the OS words it (`Permission denied (os error 13)`).
    Failed(String),
    Loaded(LoadedFile),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedFile {
    pub size: u64,
    pub mtime_ms: Option<u64>,
    /// The leading bytes the [`GatherPlan`] asked for (all of the file when it is small).
    pub bytes: Vec<u8>,
    /// `bytes` reaches the end of the file.
    pub complete: bool,
    /// Lines in the whole file; required when `!complete`.
    pub total_lines: Option<usize>,
}

const BINARY_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "gif", "bmp", "tiff", "tif", "webp", "ico", "pdf", "doc", "docx", "xls",
    "xlsx", "ppt", "pptx", "odt", "ods", "odp", "zip", "tar", "gz", "bz2", "xz", "7z", "rar",
    "dmg", "iso", "img", "exe", "msi", "deb", "rpm", "app", "pkg", "bin", "so", "dll", "dylib",
    "mp3", "mp4", "avi", "mov", "wmv", "flv", "mkv", "wav", "flac", "ogg", "woff", "woff2", "ttf",
    "otf", "eot", "db", "sqlite", "sqlite3", "pyc", "pyo", "class", "jar",
];

fn extension_of(path: &str) -> Option<String> {
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    std::path::Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
}

fn has_binary_extension(path: &str) -> bool {
    extension_of(path).is_some_and(|ext| BINARY_EXTENSIONS.contains(&ext.as_str()))
}

/// Whether `path` is read through the binary path first. A known binary extension says yes, any
/// other extension says no (a text read that meets invalid UTF-8 falls back to binary anyway),
/// and an extensionless name is decided by its first bytes.
pub fn should_read_as_binary(path: &str, head: &[u8]) -> bool {
    match extension_of(path) {
        Some(ext) => BINARY_EXTENSIONS.contains(&ext.as_str()),
        None => content_inspector::inspect(&head[..head.len().min(SNIFF_BYTES)]).is_binary(),
    }
}

/// One text segment: a requested range, or the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Segment {
    content: String,
    line_range: Option<Range<usize>>,
}

/// Accumulates lines into segments for a set of (possibly empty) line ranges under a byte
/// budget. A port of the app's `TextFileAccumulator`; see the module docs.
struct Accumulator {
    effective_ranges: Vec<Range<usize>>,
    whole_file: bool,
    max_bytes: usize,
    segments: Vec<Segment>,
    total_bytes_read: usize,
    range_idx: usize,
    buf: Vec<String>,
    buf_bytes: usize,
    truncated: bool,
    ever_truncated: bool,
    last_line: usize,
    current_line: usize,
    last_line_had_newline: bool,
}

impl Accumulator {
    fn new(requested_ranges: &[Range<usize>], max_bytes: usize) -> Self {
        let whole_file = requested_ranges.is_empty();
        let effective_ranges = if whole_file {
            #[allow(clippy::single_range_in_vec_init)]
            vec![1..usize::MAX]
        } else {
            let mut sorted = requested_ranges.to_vec();
            sorted.sort_by_key(|r| r.start);
            sorted
        };
        Self {
            effective_ranges,
            whole_file,
            max_bytes,
            segments: Vec::new(),
            total_bytes_read: 0,
            range_idx: 0,
            buf: Vec::new(),
            buf_bytes: 0,
            truncated: false,
            ever_truncated: false,
            last_line: 0,
            current_line: 0,
            last_line_had_newline: false,
        }
    }

    fn push_line(&mut self, line: String, has_trailing_newline: bool) {
        self.current_line += 1;
        self.last_line_had_newline = has_trailing_newline;

        if self.range_idx >= self.effective_ranges.len() {
            return;
        }
        if self.current_line >= self.effective_ranges[self.range_idx].end {
            self.flush_range(false);
            self.range_idx += 1;
        }
        if self.range_idx < self.effective_ranges.len() {
            let range = &self.effective_ranges[self.range_idx];
            if self.current_line >= range.start && self.current_line < range.end && !self.truncated
            {
                let line_bytes = line.len() + if self.buf.is_empty() { 0 } else { 1 };
                if self.total_bytes_read + self.buf_bytes + line_bytes > self.max_bytes {
                    self.truncated = true;
                    self.ever_truncated = true;
                } else {
                    self.buf_bytes += line_bytes;
                    self.last_line = self.current_line;
                    self.buf.push(line);
                }
            }
        }
    }

    fn flush_range(&mut self, final_flush: bool) {
        let range = self.effective_ranges[self.range_idx].clone();
        let line_range = if self.whole_file && !self.truncated {
            None
        } else if self.truncated {
            // Clamp so a segment that buffered nothing is `start..start`, not a reversed range.
            Some(range.start..self.last_line.max(range.start))
        } else {
            Some(range)
        };
        self.total_bytes_read += self.buf_bytes;
        let mut content = std::mem::take(&mut self.buf).join("\n");
        // A whole, untruncated read keeps the file's final newline so writing the content back
        // (remote diff application) does not drop it.
        if final_flush && self.whole_file && !self.truncated && self.last_line_had_newline {
            content.push('\n');
            self.total_bytes_read += 1;
        }
        self.segments.push(Segment {
            content,
            line_range,
        });
        self.buf.clear();
        self.buf_bytes = 0;
        self.truncated = false;
        self.last_line = 0;
    }

    /// (segments, bytes spent, lines seen, whether the budget ever cut a read).
    fn finalize(mut self) -> (Vec<Segment>, usize, usize, bool) {
        if self.range_idx < self.effective_ranges.len() {
            self.flush_range(true);
        }
        (
            self.segments,
            self.total_bytes_read,
            self.current_line,
            self.ever_truncated,
        )
    }
}

enum TextRead {
    Segments {
        segments: Vec<Segment>,
        bytes_read: usize,
        line_count: usize,
    },
    /// Not valid UTF-8: the binary path takes over.
    NotText,
    /// The fetched bytes cannot answer this read (see the message).
    Unsound(String),
}

fn read_text(file: &LoadedFile, ranges: &[Range<usize>], max_bytes: usize) -> TextRead {
    let text = match std::str::from_utf8(&file.bytes) {
        Ok(text) => text,
        // A prefix may end in the middle of a character; that is not invalid UTF-8.
        Err(e) if e.error_len().is_none() && !file.complete => {
            std::str::from_utf8(&file.bytes[..e.valid_up_to()]).unwrap_or_default()
        }
        Err(_) => return TextRead::NotText,
    };
    let mut accumulator = Accumulator::new(ranges, max_bytes);
    let mut rest = text;
    while !rest.is_empty() {
        let (line, tail, has_newline) = match rest.find('\n') {
            Some(i) => (&rest[..i], &rest[i + 1..], true),
            None => (rest, "", false),
        };
        let line = line.strip_suffix('\r').unwrap_or(line);
        accumulator.push_line(line.to_owned(), has_newline);
        rest = tail;
    }
    let (segments, bytes_read, lines_seen, ever_truncated) = accumulator.finalize();
    if file.complete {
        return TextRead::Segments {
            segments,
            bytes_read,
            line_count: lines_seen,
        };
    }
    // Only part of the file was fetched. Whole-file reads must have been cut by the byte budget
    // inside what was fetched, and ranged reads must have had every requested line fetched.
    let sound = if ranges.is_empty() {
        ever_truncated
    } else {
        let wanted = ranges.iter().map(|r| r.end).max().unwrap_or(0);
        lines_seen >= wanted
    };
    if !sound {
        return TextRead::Unsound(if ranges.is_empty() {
            "File is too large to read in this mode. Read a part of it with a line range, or \
             with `run_shell_command` (`sed -n '1,200p' path`)."
                .to_string()
        } else {
            "The requested line range is too far into this large file to read in this mode. \
             Use `run_shell_command` (`sed -n 'START,ENDp' path`)."
                .to_string()
        });
    }
    match file.total_lines {
        Some(line_count) => TextRead::Segments {
            segments,
            bytes_read,
            line_count,
        },
        None => TextRead::Unsound("the number of lines in the file is unknown".to_string()),
    }
}

fn format_mb(bytes: usize) -> String {
    format!("{:.1} MB", bytes as f64 / 1_000_000.0)
}

/// Separates segments the byte budget cut down to nothing from real ones (#1326): a truncated
/// segment that buffered no line is empty with an empty `start..start` range.
fn split_off_size_cut_segments(segments: Vec<Segment>) -> (Vec<Segment>, Vec<usize>) {
    let mut kept = Vec::with_capacity(segments.len());
    let mut cut = Vec::new();
    for seg in segments {
        match &seg.line_range {
            Some(r) if seg.content.is_empty() && r.start == r.end => cut.push(r.start),
            _ => kept.push(seg),
        }
    }
    (kept, cut)
}

fn failed(path: &str, message: impl Into<String>) -> FailedFileRead {
    FailedFileRead {
        path: path.to_string(),
        error: Some(FileOperationError {
            message: message.into(),
        }),
    }
}

/// The whole-request error shape: one failure with an empty path.
pub fn whole_request_error(message: impl Into<String>) -> ReadFileContextResponse {
    ReadFileContextResponse {
        file_contexts: Vec::new(),
        failed_files: vec![failed("", message)],
    }
}

/// Builds the response from what was gathered. `probes[i]` belongs to `request.files[i]`; the
/// probe of a file the batch budget never reaches is ignored.
pub fn assemble(
    request: &ReadFileContextRequest,
    probes: Vec<FileProbe>,
) -> ReadFileContextResponse {
    let (per_file_limit, max_batch) = caps(request);
    let mut batch_remaining = max_batch;
    let mut response = ReadFileContextResponse::default();

    for (file, probe) in request.files.iter().zip(probes) {
        let path = file.path.as_str();
        // The batch budget is spent: say so instead of returning an empty or silently cut entry.
        if batch_remaining == Some(0) {
            response.failed_files.push(failed(
                path,
                "Not read: this call's total size limit was reached. Request this file in a \
                 separate read_files call.",
            ));
            continue;
        }
        let loaded = match probe {
            FileProbe::NotFound => {
                response
                    .failed_files
                    .push(failed(path, "File does not exist"));
                continue;
            }
            FileProbe::Failed(reason) => {
                response
                    .failed_files
                    .push(failed(path, format!("Could not read file: {reason}")));
                continue;
            }
            FileProbe::Loaded(loaded) => loaded,
        };
        let effective_max = match batch_remaining {
            Some(remaining) => per_file_limit.min(remaining),
            None => per_file_limit,
        };
        let ranges: Vec<Range<usize>> = file
            .line_ranges
            .iter()
            .map(|r| r.start as usize..r.end as usize)
            .collect();

        if !should_read_as_binary(path, &loaded.bytes) {
            match read_text(&loaded, &ranges, effective_max) {
                TextRead::Segments {
                    segments,
                    bytes_read,
                    line_count,
                } => {
                    if let Some(remaining) = &mut batch_remaining {
                        *remaining = remaining.saturating_sub(bytes_read);
                    }
                    let (segments, cut_lines) = split_off_size_cut_segments(segments);
                    if !cut_lines.is_empty() {
                        response.failed_files.push(failed(
                            path,
                            format!(
                                "Not read: size limit. Line {} is longer than the {} bytes \
                                 allowed for this read, so no content was returned (the file is \
                                 NOT empty). Request it with a narrower line range or in a \
                                 separate read_files call.",
                                cut_lines[0], effective_max
                            ),
                        ));
                    }
                    response
                        .file_contexts
                        .extend(segments.into_iter().map(|seg| FileContextProto {
                            file_name: path.to_string(),
                            content: Some(file_context_proto::Content::TextContent(seg.content)),
                            line_range_start: seg.line_range.as_ref().map(|r| r.start as u32),
                            line_range_end: seg.line_range.as_ref().map(|r| r.end as u32),
                            last_modified_epoch_millis: loaded.mtime_ms,
                            line_count: line_count as u32,
                        }));
                    continue;
                }
                TextRead::Unsound(message) => {
                    response.failed_files.push(failed(path, message));
                    continue;
                }
                // Fall through to the binary path.
                TextRead::NotText => {}
            }
        }

        // Binary path (detected as binary, or the text read met invalid UTF-8).
        if !ranges.is_empty() && !has_binary_extension(path) {
            // The bytes were fetched as a line scan; they are not the file.
            response.failed_files.push(failed(
                path,
                "Not read: this is a binary file, so a line range does not apply. Request it \
                 without one.",
            ));
            continue;
        }
        let size = loaded.size as usize;
        if size > effective_max {
            response.failed_files.push(failed(
                path,
                format!(
                    "File is too large to read ({} > {} limit). Downscale/compress it or read a \
                     smaller copy.",
                    format_mb(size),
                    format_mb(effective_max)
                ),
            ));
            continue;
        }
        if !loaded.complete {
            response.failed_files.push(failed(
                path,
                "File is too large to read in this mode. Read it with `run_shell_command`.",
            ));
            continue;
        }
        if let Some(remaining) = &mut batch_remaining {
            *remaining = remaining.saturating_sub(loaded.bytes.len());
        }
        response.file_contexts.push(FileContextProto {
            file_name: path.to_string(),
            content: Some(file_context_proto::Content::BinaryContent(loaded.bytes)),
            line_range_start: None,
            line_range_end: None,
            last_modified_epoch_millis: loaded.mtime_ms,
            line_count: 0,
        });
    }
    response
}

// ── Shell side: builders and the section parser ────────────────────────────────────────────

/// `f=...` for `path`, expanding a leading `~` / `~/` on the host (not in the app).
pub(super) fn posix_assign_path(path: &str) -> String {
    if path == "~" {
        "f=\"$HOME\"".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("f=\"$HOME\"{}", sh_quote(&format!("/{rest}")))
    } else {
        format!("f={}", sh_quote(path))
    }
}

/// The POSIX section that reads one file. Prints five header lines then the payload:
///
/// ```text
/// OK | NOTFOUND | DIR | DENIED
/// <size in bytes>
/// <mtime, seconds since the epoch, or ->
/// <lines in the file, or ->
/// <the plan's bytes, encoded with `encoder`>
/// ```
///
/// (The four lines after the status exist only for `OK`.) `encoder` is the host base64 tool's
/// filter ([`Base64Tool::encoder_filter`]).
pub fn posix_file_section(path: &str, plan: &GatherPlan, encoder: &str) -> String {
    let needs_lines = match plan.through_line {
        Some(_) => "true".to_string(),
        None => format!("[ \"$size\" -gt {} ]", plan.prefix_bytes),
    };
    let payload = match plan.through_line {
        Some(line) => format!(
            "sed -n '1,{line}p;{line}q' < \"$f\" | head -c {} | {encoder}",
            plan.prefix_bytes
        ),
        None => format!("head -c {} < \"$f\" | {encoder}", plan.prefix_bytes),
    };
    format!(
        "{assign}\n\
         if [ ! -e \"$f\" ]; then echo NOTFOUND\n\
         elif [ -d \"$f\" ]; then echo DIR\n\
         elif [ ! -r \"$f\" ]; then echo DENIED\n\
         else\n\
         echo OK\n\
         size=$(wc -c < \"$f\" | tr -d ' \\t')\n\
         echo \"$size\"\n\
         (stat -c %Y -- \"$f\" 2>/dev/null || stat -f %m -- \"$f\" 2>/dev/null || echo -) | head -n 1\n\
         if {needs_lines}; then grep -c '' < \"$f\"; else echo -; fi\n\
         {payload}\n\
         fi",
        assign = posix_assign_path(path),
    )
}

/// The PowerShell section that reads one file; the same five lines as [`posix_file_section`],
/// with the payload as standard base64 on one line.
pub fn pwsh_file_section(path: &str, plan: &GatherPlan) -> String {
    let quoted = format!("'{}'", path.replace('\'', "''"));
    let assign = if path == "~" {
        "$f = $HOME".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("$f = Join-Path $HOME '{}'", rest.replace('\'', "''"))
    } else {
        format!("$f = {quoted}")
    };
    let (lines, payload) = match plan.through_line {
        Some(line) => (
            "([System.IO.File]::ReadLines($f) | Measure-Object).Count".to_string(),
            format!(
                // Byte-exact, like `sed -n 1,Np | head -c`: scan for the Nth LF in the raw bytes,
                // never decode and re-encode (that altered non-UTF-8 files and dropped a BOM).
                "$fs = [System.IO.File]::OpenRead($f)\n\
                 $ms = New-Object System.IO.MemoryStream\n\
                 $buf = New-Object byte[] 65536\n\
                 $nl = 0\n\
                 $done = $false\n\
                 while (-not $done -and $ms.Length -lt {cap}) {{\n\
                 $r = $fs.Read($buf, 0, $buf.Length)\n\
                 if ($r -le 0) {{ break }}\n\
                 $p = 0\n\
                 while ($p -lt $r) {{\n\
                 $k = [Array]::IndexOf($buf, [byte]10, $p, $r - $p)\n\
                 if ($k -lt 0) {{ break }}\n\
                 $nl++\n\
                 $p = $k + 1\n\
                 if ($nl -ge {line}) {{ $done = $true; break }}\n\
                 }}\n\
                 if ($done) {{ $ms.Write($buf, 0, $p) }} else {{ $ms.Write($buf, 0, $r) }}\n\
                 }}\n\
                 $fs.Dispose()\n\
                 $b = $ms.ToArray()\n\
                 [Convert]::ToBase64String($b, 0, [Math]::Min($b.Length, {cap}))",
                cap = plan.prefix_bytes
            ),
        ),
        None => (
            format!(
                "if ($i.Length -gt {}) {{ ([System.IO.File]::ReadLines($f) | Measure-Object).Count }} else {{ '-' }}",
                plan.prefix_bytes
            ),
            format!(
                "$fs = [System.IO.File]::OpenRead($f)\n\
                 $b = New-Object byte[] {cap}\n\
                 $n = 0\n\
                 while ($n -lt {cap}) {{ $r = $fs.Read($b, $n, {cap} - $n); if ($r -le 0) {{ break }}; $n += $r }}\n\
                 $fs.Dispose()\n\
                 [Convert]::ToBase64String($b, 0, $n)",
                cap = plan.prefix_bytes
            ),
        ),
    };
    format!(
        "{assign}\n\
         if (-not (Test-Path -LiteralPath $f)) {{ 'NOTFOUND' }}\n\
         elseif (Test-Path -LiteralPath $f -PathType Container) {{ 'DIR' }}\n\
         else {{\n\
         'OK'\n\
         $i = Get-Item -LiteralPath $f\n\
         $i.Length\n\
         [DateTimeOffset]::new($i.LastWriteTimeUtc).ToUnixTimeSeconds()\n\
         {lines}\n\
         {payload}\n\
         }}"
    )
}

/// The most one range read returns (a multiple of [`RANGE_BLOCK_BYTES`]). Its base64 stays well
/// under [`super::MAX_OUTPUT_BYTES`].
pub const RANGE_CHUNK_BYTES: u64 = 4 * RANGE_BLOCK_BYTES;
/// The `dd` block size of a range read; offsets are multiples of it.
pub const RANGE_BLOCK_BYTES: u64 = 1024 * 1024;

/// The POSIX section that reads `len` bytes of the file from `offset` (a multiple of
/// [`RANGE_BLOCK_BYTES`]; `len` at most [`RANGE_CHUNK_BYTES`]) with `dd skip/count`: `OK` then the
/// bytes encoded with `encoder`, or `NOTFOUND`, `DIR`, `DENIED`. Used to read what the first
/// section's prefix cap left behind, in sections that each fit one command's output (#1873).
pub fn posix_range_section(path: &str, offset: u64, len: u64, encoder: &str) -> String {
    let skip = offset / RANGE_BLOCK_BYTES;
    let count = len.div_ceil(RANGE_BLOCK_BYTES).max(1);
    format!(
        "{assign}\n\
         if [ ! -e \"$f\" ]; then echo NOTFOUND\n\
         elif [ -d \"$f\" ]; then echo DIR\n\
         elif [ ! -r \"$f\" ]; then echo DENIED\n\
         else\n\
         echo OK\n\
         dd if=\"$f\" bs={RANGE_BLOCK_BYTES} skip={skip} count={count} 2>/dev/null | head -c {len} | {encoder}\n\
         fi",
        assign = posix_assign_path(path),
    )
}

/// The PowerShell form of [`posix_range_section`]: seek, read `len` bytes, one line of base64.
pub fn pwsh_range_section(path: &str, offset: u64, len: u64) -> String {
    let assign = if path == "~" {
        "$f = $HOME".to_string()
    } else if let Some(rest) = path.strip_prefix("~/") {
        format!("$f = Join-Path $HOME '{}'", rest.replace('\'', "''"))
    } else {
        format!("$f = '{}'", path.replace('\'', "''"))
    };
    format!(
        "{assign}\n\
         if (-not (Test-Path -LiteralPath $f)) {{ 'NOTFOUND' }}\n\
         elseif (Test-Path -LiteralPath $f -PathType Container) {{ 'DIR' }}\n\
         else {{\n\
         'OK'\n\
         $fs = [System.IO.File]::OpenRead($f)\n\
         [void]$fs.Seek({offset}, [System.IO.SeekOrigin]::Begin)\n\
         $b = New-Object byte[] {len}\n\
         $n = 0\n\
         while ($n -lt {len}) {{ $r = $fs.Read($b, $n, {len} - $n); if ($r -le 0) {{ break }}; $n += $r }}\n\
         $fs.Dispose()\n\
         [Convert]::ToBase64String($b, 0, $n)\n\
         }}"
    )
}

/// Reads a range section back: the decoded bytes, or why the host could not read them.
pub fn parse_range_section(
    section: &ParsedSection,
    tool: Option<Base64Tool>,
) -> Result<Vec<u8>, String> {
    let mut lines = section.output.splitn(2, |&b| b == b'\n');
    let status = lines
        .next()
        .map(|l| String::from_utf8_lossy(l).trim().to_string())
        .unwrap_or_default();
    match status.as_str() {
        "OK" => {}
        "NOTFOUND" => return Err("File does not exist".to_string()),
        "DIR" => return Err("Is a directory (os error 21)".to_string()),
        "DENIED" => return Err("Permission denied (os error 13)".to_string()),
        _ => {
            return Err(format!(
                "the host's read command printed nothing usable (exit {})",
                section.exit
            ));
        }
    }
    let payload = lines.next().unwrap_or_default();
    let decoded = match tool {
        Some(tool) => tool.decode(payload),
        None => super::base64::decode(payload),
    };
    decoded.map_err(|e| format!("unreadable transfer from the host: {e}"))
}

impl Base64Tool {
    /// The pipeline stage that encodes stdin in this tool's format.
    pub fn encoder_filter(self) -> &'static str {
        match self {
            Self::GnuW0 => "base64 -w0",
            Self::Plain => "base64",
            Self::Openssl => "openssl base64",
            Self::Od => "od -An -v -tx1",
        }
    }
}

/// Reads one file's section back. `tool` decodes the payload (`None` for PowerShell, which
/// prints plain base64).
pub fn parse_file_section(
    section: &ParsedSection,
    plan: &GatherPlan,
    tool: Option<Base64Tool>,
) -> FileProbe {
    let output = &section.output;
    let mut lines = output.splitn(5, |&b| b == b'\n');
    let mut next_line = || -> Option<String> {
        lines
            .next()
            .map(|l| String::from_utf8_lossy(l).trim().to_string())
    };
    let status = next_line().unwrap_or_default();
    match status.as_str() {
        "NOTFOUND" => return FileProbe::NotFound,
        "DIR" => return FileProbe::Failed("Is a directory (os error 21)".to_string()),
        "DENIED" => return FileProbe::Failed("Permission denied (os error 13)".to_string()),
        "OK" => {}
        _ => {
            let text = section.text();
            let text = text.trim();
            return FileProbe::Failed(if text.is_empty() {
                format!(
                    "the host's read command printed nothing (exit {})",
                    section.exit
                )
            } else {
                text.lines().next().unwrap_or(text).to_string()
            });
        }
    }
    let size = next_line().and_then(|s| s.parse::<u64>().ok());
    let mtime_seconds = next_line().and_then(|s| s.parse::<u64>().ok());
    let total_lines = next_line().and_then(|s| s.parse::<usize>().ok());
    let payload = lines.next().unwrap_or_default();
    let Some(size) = size else {
        return FileProbe::Failed("the host did not report the file's size".to_string());
    };
    let decoded = match tool {
        Some(tool) => tool.decode(payload),
        None => super::base64::decode(payload),
    };
    let bytes = match decoded {
        Ok(bytes) => bytes,
        Err(e) => return FileProbe::Failed(format!("unreadable transfer from the host: {e}")),
    };
    let complete = match plan.through_line {
        // The scan stopped after `through_line` lines; the file ended there or earlier.
        Some(through) => total_lines.is_some_and(|n| n <= through),
        None => size <= bytes.len() as u64,
    };
    FileProbe::Loaded(LoadedFile {
        size,
        mtime_ms: mtime_seconds.map(|s| s * 1000),
        bytes,
        complete,
        total_lines: if complete { None } else { total_lines },
    })
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
