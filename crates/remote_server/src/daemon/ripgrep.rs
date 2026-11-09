//! `RipgrepSearch` for the helper: an in-process search over the same `grep` and `ignore`
//! crates the app's `warp_ripgrep` uses. The app re-executes itself as a ripgrep subprocess;
//! the helper has no second personality to re-execute, so it searches on worker threads.
//! Response caps and request validation are the app daemon's.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use anyhow::anyhow;
use grep::matcher::Matcher as _;
use grep::regex::{RegexMatcher, RegexMatcherBuilder};
use grep::searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::{WalkBuilder, WalkState};

use crate::proto::{
    RipgrepSearchError, RipgrepSearchMatch, RipgrepSearchRequest, RipgrepSearchResponse,
    RipgrepSearchSubmatch, RipgrepSearchSuccess, ripgrep_search_response,
};

use super::jobs::Cancel;

/// Server-side cap on the number of matched lines returned by `RipgrepSearch`.
const MAX_RIPGREP_SEARCH_MATCH_CAP: usize = 5_000;
/// Approximate payload budget for one remote search response (8 MB, well under the protocol's
/// 64 MB frame limit). Individual matches are never truncated: cutting one could remove a late
/// submatch and corrupt its preview and click location.
const MAX_RIPGREP_SEARCH_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Longest line tolerated in single-line mode; files with longer lines are skipped, bounding
/// memory on minified or generated files.
const SEARCHER_LINE_HEAP_LIMIT: usize = 64 * 1024;

pub(super) struct RipgrepSearchParams {
    pattern: String,
    roots: Vec<PathBuf>,
    ignore_case: bool,
    multiline: bool,
    match_cap: usize,
}

pub(super) fn validate_request(msg: RipgrepSearchRequest) -> Result<RipgrepSearchParams, String> {
    if msg.pattern.is_empty() || msg.roots.is_empty() {
        return Err("RipgrepSearch requires a pattern and at least one root".to_string());
    }
    if let Some(root) = msg.roots.iter().find(|root| !Path::new(root).is_absolute()) {
        return Err(format!("RipgrepSearch root must be absolute: {root}"));
    }
    let roots = msg.roots.iter().map(PathBuf::from).collect();
    let match_cap = match msg.max_matches as usize {
        0 => MAX_RIPGREP_SEARCH_MATCH_CAP,
        requested => requested.min(MAX_RIPGREP_SEARCH_MATCH_CAP),
    };
    Ok(RipgrepSearchParams {
        pattern: msg.pattern,
        roots,
        ignore_case: msg.ignore_case,
        multiline: msg.multiline,
        match_cap,
    })
}

#[derive(Default)]
struct Collected {
    matches: Vec<RipgrepSearchMatch>,
    response_bytes: usize,
    capped: bool,
}

/// Shared by every walker thread.
struct Shared<'a> {
    collected: Mutex<Collected>,
    stop: AtomicBool,
    visited: AtomicUsize,
    match_cap: usize,
    cancel: &'a Cancel,
}

impl Shared<'_> {
    fn should_stop(&self) -> bool {
        self.stop.load(Ordering::Acquire) || self.cancel.is_cancelled()
    }

    /// Adds one match; false once the caps stop the search.
    fn push(&self, m: RipgrepSearchMatch) -> bool {
        let Ok(mut collected) = self.collected.lock() else {
            self.stop.store(true, Ordering::Release);
            return false;
        };
        if collected.capped {
            return false;
        }
        let match_bytes = m
            .file_path
            .len()
            .saturating_add(m.line_text.len())
            .saturating_add(
                m.submatches
                    .len()
                    .saturating_mul(2 * std::mem::size_of::<u64>()),
            );
        if collected.matches.len() >= self.match_cap
            || collected.response_bytes.saturating_add(match_bytes)
                > MAX_RIPGREP_SEARCH_RESPONSE_BYTES
        {
            collected.capped = true;
            self.stop.store(true, Ordering::Release);
            return false;
        }
        collected.response_bytes += match_bytes;
        collected.matches.push(m);
        true
    }
}

struct MatchSink<'a, 'b> {
    matcher: &'a RegexMatcher,
    path: &'a Path,
    shared: &'a Shared<'b>,
}

impl Sink for MatchSink<'_, '_> {
    type Error = std::io::Error;

    fn matched(&mut self, _searcher: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, Self::Error> {
        if self.shared.should_stop() {
            return Ok(false);
        }
        let bytes = mat.bytes();
        let mut submatches = Vec::new();
        // Offsets are relative to the start of the matched line(s), as ripgrep's JSON printer
        // reports them, which is what the client indexes `line_text` with.
        let _ = self.matcher.find_iter(bytes, |m| {
            submatches.push(RipgrepSearchSubmatch {
                byte_start: m.start() as u64,
                byte_end: m.end() as u64,
            });
            true
        });
        let keep_going = self.shared.push(RipgrepSearchMatch {
            file_path: self.path.to_string_lossy().to_string(),
            line_number: mat.line_number().unwrap_or(0).min(u64::from(u32::MAX)) as u32,
            line_text: String::from_utf8_lossy(bytes).into_owned(),
            submatches,
        });
        Ok(keep_going)
    }
}

/// Runs the search on the calling thread (with ripgrep's own parallel walker underneath).
pub(super) fn run_search(
    params: RipgrepSearchParams,
    cancel: &Cancel,
) -> anyhow::Result<RipgrepSearchSuccess> {
    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder.case_insensitive(params.ignore_case);
    if params.multiline {
        matcher_builder.line_terminator(None);
    }
    let matcher = matcher_builder
        .build(&params.pattern)
        .map_err(|e| anyhow!("{e}"))?;

    let shared = Shared {
        collected: Mutex::new(Collected::default()),
        stop: AtomicBool::new(false),
        visited: AtomicUsize::new(0),
        match_cap: params.match_cap,
        cancel,
    };

    let mut walker_builder = WalkBuilder::new(&params.roots[0]);
    for root in params.roots.iter().skip(1) {
        walker_builder.add(root);
    }
    let multiline = params.multiline;
    walker_builder.build_parallel().run(|| {
        let matcher = matcher.clone();
        let shared = &shared;
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .multi_line(multiline)
            .heap_limit((!multiline).then_some(SEARCHER_LINE_HEAP_LIMIT))
            .build();
        Box::new(move |entry| {
            if shared.should_stop() {
                return WalkState::Quit;
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    log::warn!("ripgrep walk error: {err}");
                    return WalkState::Continue;
                }
            };
            if !entry.file_type().is_some_and(|ft| ft.is_file()) {
                return WalkState::Continue;
            }
            shared.visited.fetch_add(1, Ordering::Relaxed);
            let mut sink = MatchSink {
                matcher: &matcher,
                path: entry.path(),
                shared,
            };
            if let Err(err) = searcher.search_path(&matcher, entry.path(), &mut sink) {
                log::warn!(
                    "ripgrep search error for {}: {}",
                    entry.path().display(),
                    err
                );
            }
            if shared.should_stop() {
                WalkState::Quit
            } else {
                WalkState::Continue
            }
        })
    });

    let collected = shared
        .collected
        .into_inner()
        .map_err(|_| anyhow!("a search thread panicked"))?;
    Ok(RipgrepSearchSuccess {
        matches: collected.matches,
        capped: collected.capped,
    })
}

pub(super) fn error_response(message: String) -> RipgrepSearchResponse {
    RipgrepSearchResponse {
        result: Some(ripgrep_search_response::Result::Error(RipgrepSearchError {
            message,
        })),
    }
}

pub(super) fn search_result_to_response(
    result: anyhow::Result<RipgrepSearchSuccess>,
) -> RipgrepSearchResponse {
    match result {
        Ok(success) => RipgrepSearchResponse {
            result: Some(ripgrep_search_response::Result::Success(success)),
        },
        Err(err) => error_response(format!("{err:#}")),
    }
}

#[cfg(test)]
#[path = "ripgrep_tests.rs"]
mod tests;
