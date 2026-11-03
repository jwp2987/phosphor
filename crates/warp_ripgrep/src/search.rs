use std::path::PathBuf;

use anyhow::anyhow;
use grep::regex::RegexMatcherBuilder;
use grep::{
    printer::JSONBuilder,
    searcher::{BinaryDetection, SearcherBuilder},
};
use ignore::{WalkBuilder, WalkState};
use std::ops::Not;
use string_offset::ByteOffset;

/// Maximum line length (in bytes) the ripgrep searcher will tolerate.
/// Files containing lines longer than this are skipped, preventing
/// unbounded memory growth from minified or generated files.
/// Only applied in single-line mode; multiline search needs the full file
/// in memory so the limit would be per-file rather than per-line.
const SEARCHER_LINE_HEAP_LIMIT: usize = 64 * 1024;

/// A single submatch span within a matched line.
#[derive(Clone, Debug)]
pub struct Submatch {
    /// Byte offset into `line_text` where this submatch starts.
    pub byte_start: ByteOffset,
    /// Byte offset into `line_text` where this submatch ends (exclusive).
    pub byte_end: ByteOffset,
}

/// A single search match: one line in one file, with submatch highlights.
#[derive(Clone, Debug)]
pub struct Match {
    pub file_path: PathBuf,
    pub line_number: u32,
    pub line_text: String,
    pub submatches: Vec<Submatch>,
}

/// Entry point for the ripgrep subprocess.
///
/// Runs a ripgrep search in-process and writes JSON results to stdout.
/// The main Zap process spawns this via the `ripgrep-search` CLI
/// subcommand and reads the JSON output.
pub fn run_search_subprocess(
    patterns: &[String],
    paths: Vec<PathBuf>,
    ignore_case: bool,
    multiline: bool,
    #[cfg_attr(not(unix), allow(unused_variables))] parent_pid: Option<u32>,
) -> anyhow::Result<()> {
    #[cfg(unix)]
    crate::monitor_parent_and_exit_on_change(parent_pid);

    if patterns.is_empty() {
        return Err(anyhow!("No patterns specified"));
    }
    if paths.is_empty() {
        return Err(anyhow!("No paths specified"));
    }

    let mut matcher_builder = RegexMatcherBuilder::new();
    matcher_builder.case_insensitive(ignore_case);
    if multiline {
        matcher_builder.line_terminator(None);
    }
    let matcher = matcher_builder.build_many(patterns)?;

    let stdout = std::sync::Mutex::new(std::io::stdout());

    let mut walker_builder = WalkBuilder::new(&paths[0]);
    for path in paths.iter().skip(1) {
        walker_builder.add(path);
    }
    let walker = walker_builder.build_parallel();

    walker.run(|| {
        let matcher = matcher.clone();
        let stdout = &stdout;

        // Allocate once per thread and reuse across entries.
        let mut buf = Vec::new();
        let mut searcher = SearcherBuilder::new()
            .binary_detection(BinaryDetection::quit(b'\x00'))
            .line_number(true)
            .multi_line(multiline)
            .heap_limit(multiline.not().then_some(SEARCHER_LINE_HEAP_LIMIT))
            .build();

        Box::new(move |entry| {
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

            // Search into the thread-local buffer, then flush the
            // complete JSON lines to stdout under a lock so that
            // output from parallel threads never interleaves.
            buf.clear();
            let mut printer = JSONBuilder::new().build(std::io::Cursor::new(&mut buf));

            if let Err(err) = searcher.search_path(
                &matcher,
                entry.path(),
                printer.sink_with_path(&matcher, entry.path()),
            ) {
                log::warn!(
                    "ripgrep search error for {}: {}",
                    entry.path().display(),
                    err
                );
            }

            if !buf.is_empty() {
                let Ok(mut out) = stdout.lock() else {
                    // Mutex poisoned — another search thread panicked.
                    return WalkState::Quit;
                };
                use std::io::Write;
                if out.write_all(&buf).is_err() {
                    // Stdout pipe is broken (parent likely cancelled the
                    // search), so stop walking.
                    return WalkState::Quit;
                }
            }

            WalkState::Continue
        })
    });

    Ok(())
}

mod process_impl {
    use std::path::PathBuf;
    use std::process::Stdio;

    use futures::io::{AsyncBufReadExt as _, BufReader};
    use futures::stream::Stream;
    use futures::StreamExt as _;

    use super::{Match, Submatch};
    use crate::types::RipgrepMessage;

    /// Searches `paths` for lines matching `patterns` and returns all results.
    ///
    /// Spawns a child process that runs the ripgrep search, collects every
    /// match, and returns them once the search is complete.
    ///
    /// For incremental results, use [`search_streaming`] instead.
    pub async fn search(
        patterns: &[String],
        paths: &[PathBuf],
        ignore_case: bool,
        multiline: bool,
    ) -> anyhow::Result<Vec<Match>> {
        let mut child = spawn_search_process(patterns, paths, ignore_case, multiline, true)?;
        let stdout = child
            .stdout
            .take()
            .expect("spawn_search_process must pipe stdout");
        let mut stderr = child
            .stderr
            .take()
            .expect("spawn_search_process pipes stderr when asked to");
        let matches: Vec<Match> = match_stream(BufReader::new(stdout), None).collect().await;
        // The search writes stderr only to explain a failure, so it is short; read after
        // stdout closed, which means the process is exiting.
        let mut stderr_text = String::new();
        let _ = futures::io::AsyncReadExt::read_to_string(&mut stderr, &mut stderr_text).await;
        let status = child.status().await?;
        match search_failure(status.success(), status.code(), &stderr_text) {
            Some(message) => Err(anyhow::anyhow!(message)),
            None => Ok(matches),
        }
    }

    /// Why a finished search process is a failure, if it is. The subprocess exits non-zero
    /// for a pattern that does not compile, a path that cannot be walked, or a crash; an empty
    /// stdout with exit 0 is a successful search with no matches. Reporting the failure is what
    /// keeps "no matches" from being the answer to a bad regex.
    pub(crate) fn search_failure(
        success: bool,
        exit_code: Option<i32>,
        stderr: &str,
    ) -> Option<String> {
        if success {
            return None;
        }
        let how = match exit_code {
            Some(code) => format!("exited with status {code}"),
            None => "was killed by a signal".to_owned(),
        };
        let detail = stderr.trim();
        Some(if detail.is_empty() {
            format!("the search process {how}")
        } else {
            format!("the search process {how}: {detail}")
        })
    }

    /// Searches `paths` for lines matching `patterns`, returning a stream
    /// of matches as they are found.
    ///
    /// This is the preferred entry point when responsiveness matters (e.g.
    /// the global search UI). The caller controls batching and throttling.
    pub fn search_streaming(
        patterns: &[String],
        paths: &[PathBuf],
        ignore_case: bool,
        multiline: bool,
    ) -> anyhow::Result<impl Stream<Item = Match> + use<>> {
        let child = spawn_search_process(patterns, paths, ignore_case, multiline, false)?;
        Ok(match_stream_from_child(child))
    }

    /// Spawns the warp CLI with the `ripgrep-search` subcommand. `capture_stderr` pipes the
    /// child's stderr for the caller to read; otherwise it is inherited.
    fn spawn_search_process(
        patterns: &[String],
        paths: &[PathBuf],
        ignore_case: bool,
        multiline: bool,
        capture_stderr: bool,
    ) -> Result<async_process::Child, std::io::Error> {
        let current_exe = std::env::current_exe()?;
        let mut cmd = command::r#async::Command::new(current_exe);

        cmd.arg(warp_cli::ripgrep_search_subcommand())
            .arg(warp_cli::parent_flag());

        if ignore_case {
            cmd.arg("--ignore-case");
        }

        if multiline {
            cmd.arg("--multiline");
        }

        for pattern in patterns {
            cmd.arg(pattern);
        }

        for path in paths {
            cmd.arg(path);
        }

        cmd.kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(if capture_stderr {
                Stdio::piped()
            } else {
                Stdio::inherit()
            });

        cmd.spawn()
    }

    /// Turns a child process (with piped stdout) into a stream of parsed matches. The child is
    /// owned by the stream, so it is killed when the stream is dropped.
    fn match_stream_from_child(mut child: async_process::Child) -> impl Stream<Item = Match> {
        let stdout = child
            .stdout
            .take()
            .expect("spawn_search_process must pipe stdout");
        match_stream(BufReader::new(stdout), Some(child))
    }

    /// Parses the ripgrep JSON lines of `reader` into matches. `keep_alive` is held for the
    /// stream's lifetime: the child whose stdout this is, when the caller does not need it back
    /// (to await its exit status) once the output ends.
    fn match_stream<R: futures::io::AsyncBufRead + Unpin>(
        reader: R,
        keep_alive: Option<async_process::Child>,
    ) -> impl Stream<Item = Match> {
        futures::stream::unfold((reader, keep_alive), |(mut reader, child)| async move {
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) | Err(_) => return None,
                    Ok(_) => {}
                }
                if line.trim().is_empty() {
                    continue;
                }
                match serde_json::from_str::<RipgrepMessage>(&line) {
                    Ok(RipgrepMessage::Match { data }) => {
                        let submatches = data
                            .submatches
                            .into_iter()
                            .map(|s| Submatch {
                                byte_start: s.start,
                                byte_end: s.end,
                            })
                            .collect();
                        let m = Match {
                            file_path: PathBuf::from(data.path.text),
                            line_number: data.line_number,
                            line_text: data.lines.text,
                            submatches,
                        };
                        return Some((m, (reader, child)));
                    }
                    Ok(RipgrepMessage::Begin | RipgrepMessage::End) => continue,
                    Err(err) => {
                        log::warn!("ripgrep: failed to parse JSON line: {err}");
                        continue;
                    }
                }
            }
        })
    }
}

pub use process_impl::{search, search_streaming};

#[cfg(all(test, not(target_family = "wasm")))]
mod search_failure_tests {
    use super::process_impl::search_failure;

    /// A clean exit is a successful search, with or without matches: an empty stdout is "no
    /// matches", never an error.
    #[test]
    fn a_clean_exit_is_not_a_failure() {
        assert_eq!(search_failure(true, Some(0), ""), None);
    }

    /// A pattern that does not compile (or a path that cannot be walked) exits non-zero with
    /// the reason on stderr; the caller gets that reason instead of an empty result. Fails on
    /// the old code, which never looked at the exit status and returned `Ok(vec![])`.
    #[test]
    fn a_non_zero_exit_reports_stderr() {
        let failure = search_failure(false, Some(1), "regex parse error: unclosed group\n")
            .expect("a non-zero exit is a failure");
        assert!(failure.contains("status 1"), "{failure}");
        assert!(failure.contains("unclosed group"), "{failure}");
        assert_eq!(
            search_failure(false, None, "").as_deref(),
            Some("the search process was killed by a signal")
        );
    }
}
