//! Per-scenario logs for a CI run (#2122).
//!
//! When `WARP_INTEGRATION_EVIDENCE_DIR` is set (the `integration-linux` jobs set it), the
//! scenario's whole output goes to `<dir>/logs/<scenario>.<attempt>.log` as it is produced, and
//! everything except `[DEBUG]` lines is also passed on to this process's own stdout / stderr,
//! which is what nextest shows for a failed test. So:
//!
//! * the job trace reads as before (the INFO log of a failed scenario), and stays small enough
//!   for GitLab's trace limit even though the run records the `warp::input_order` debug target
//!   (`hop=` lines: every key, pty write, menu and hover state, by length and name, no text);
//! * the log of a scenario that FAILED is kept, complete, for the job's artifacts;
//! * the log of a scenario that passed is removed, which is what bounds the artifact.
//!
//! The file is written line by line with no buffering, so a scenario killed by nextest's
//! timeout leaves everything it printed up to that point.
//!
//! Without the variable nothing here runs and the scenario inherits stdout / stderr as before.

use std::fs::File;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ExitStatus};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long the copy threads may still run after the scenario process has exited.
const DRAIN_AFTER_EXIT: Duration = Duration::from_secs(3);

/// Same name as `warpui::integration::EVIDENCE_DIR_ENV_VAR` (the driver writes its skip
/// markers under the same directory).
pub const EVIDENCE_DIR_ENV: &str = "WARP_INTEGRATION_EVIDENCE_DIR";

/// The evidence directory of this run, if it keeps one.
pub fn evidence_dir() -> Option<PathBuf> {
    std::env::var_os(EVIDENCE_DIR_ENV)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
}

/// The log filter a run that keeps evidence adds for the scenario: the app's input-order
/// target at debug, on top of the INFO default.
pub const DEFAULT_EVIDENCE_LOG: &str = "warp::input_order=debug";

/// Overrides [`DEFAULT_EVIDENCE_LOG`]: another `RUST_LOG` filter, or `off` / `0` / empty for
/// none (the logs are still kept, at INFO).
pub const EVIDENCE_LOG_ENV: &str = "WARP_INTEGRATION_EVIDENCE_LOG";

/// The `RUST_LOG` to start the scenario with, given what the caller set. `None` leaves the
/// environment alone: a `RUST_LOG` of the caller's own always wins (it is passed on as is).
pub fn evidence_rust_log(caller_rust_log: Option<&str>, choice: Option<&str>) -> Option<String> {
    if caller_rust_log.is_some_and(|value| !value.is_empty()) {
        return None;
    }
    match choice.map(str::trim) {
        Some("" | "off" | "0") => None,
        Some(filter) => Some(filter.to_owned()),
        None => Some(DEFAULT_EVIDENCE_LOG.to_owned()),
    }
}

/// [`evidence_rust_log`] of this process's environment.
pub fn evidence_rust_log_from_env() -> Option<String> {
    evidence_rust_log(
        std::env::var("RUST_LOG").ok().as_deref(),
        std::env::var(EVIDENCE_LOG_ENV).ok().as_deref(),
    )
}

/// The log file name of one start of a scenario. `attempt` is nextest's try number when it
/// gives one, else this process's id; `run` counts the driver's own re-runs (exit code 127).
pub fn log_file_name(scenario: &str, attempt: &str, run: usize) -> String {
    if run == 0 {
        format!("{scenario}.{attempt}.log")
    } else {
        format!("{scenario}.{attempt}.rerun{run}.log")
    }
}

fn attempt_label() -> String {
    match std::env::var("NEXTEST_ATTEMPT") {
        Ok(attempt) if !attempt.is_empty() => format!("try{attempt}"),
        _ => format!("pid{}", std::process::id()),
    }
}

/// Whether a line belongs to the log file only. A `[DEBUG]` line of the app's logger:
/// `HH:MM:SS.mmm [DEBUG] [target] ...`.
pub fn is_debug_line(line: &[u8]) -> bool {
    const MARK: &[u8] = b" [DEBUG] ";
    // The level comes right after the time stamp: look near the start only, so a line that
    // merely quotes the word further in is not hidden.
    let head = &line[..line.len().min(40)];
    head.windows(MARK.len()).any(|window| window == MARK)
}

/// An open per-scenario log.
pub struct ScenarioLog {
    path: PathBuf,
    file: Arc<Mutex<File>>,
}

impl ScenarioLog {
    /// Creates the log of this start of `scenario`, or `None` when the run keeps no evidence
    /// (or the file cannot be made: the scenario then runs as it always did).
    pub fn create(scenario: &str, run: usize) -> Option<Self> {
        let dir = evidence_dir()?.join("logs");
        std::fs::create_dir_all(&dir).ok()?;
        let path = dir.join(log_file_name(scenario, &attempt_label(), run));
        let file = File::create(&path).ok()?;
        Some(Self {
            path,
            file: Arc::new(Mutex::new(file)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Copies both pipes of `child` into the log, passing everything but `[DEBUG]` lines on to
    /// this process's stdout / stderr, and waits for the child.
    ///
    /// The copying normally ends when the scenario exits and its pipes close. A process the
    /// scenario left behind (it inherits the pipes) would keep them open; the wait for the
    /// copy threads is therefore bounded once the scenario itself has exited, so the verdict
    /// is never held up by a leftover process. With inherited stdio (no evidence directory)
    /// such a process never delayed the shim either.
    pub fn follow(&self, mut child: Child) -> std::io::Result<ExitStatus> {
        let out = child.stdout.take().map(|pipe| {
            let file = self.file.clone();
            std::thread::spawn(move || copy_lines(pipe, &file, &mut std::io::stdout()))
        });
        let err = child.stderr.take().map(|pipe| {
            let file = self.file.clone();
            std::thread::spawn(move || copy_lines(pipe, &file, &mut std::io::stderr()))
        });
        let status = child.wait();
        let threads: Vec<_> = [out, err].into_iter().flatten().collect();
        // Give-up net, not a pace: the threads finish as soon as the pipes close.
        let deadline = Instant::now() + DRAIN_AFTER_EXIT;
        while threads.iter().any(|thread| !thread.is_finished()) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if threads.iter().any(|thread| !thread.is_finished()) {
            let note = format!(
                "INTEG-EVIDENCE the scenario exited but something still holds its output open \
                 after {} ms (a process it left behind?); the log ends here\n",
                DRAIN_AFTER_EXIT.as_millis()
            );
            if let Ok(mut file) = self.file.lock() {
                let _ = file.write_all(note.as_bytes());
            }
            eprint!("{note}");
        }
        status
    }

    /// The scenario passed: its log is not evidence of anything and is removed.
    pub fn discard(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Copies `pipe` line by line into `file` and, for lines that are not `[DEBUG]`, into
/// `passthrough`. Bytes, not text: a scenario may print anything.
fn copy_lines(pipe: impl Read, file: &Mutex<File>, passthrough: &mut dyn Write) {
    let mut reader = BufReader::new(pipe);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if let Ok(mut file) = file.lock() {
            let _ = file.write_all(&line);
        }
        if !is_debug_line(&line) {
            let _ = passthrough.write_all(&line);
            let _ = passthrough.flush();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_apps_debug_lines_stay_out_of_the_trace() {
        assert!(is_debug_line(
            b"22:20:13.710 [DEBUG] [warp::input_order] input_order hop=pty_write_bytes n=3\n"
        ));
        assert!(!is_debug_line(
            b"22:20:13.710 [INFO] [warpui_core::integration::step] integ_event seq=1\n"
        ));
        // A line that only mentions the level further in is shown.
        assert!(!is_debug_line(
            b"22:20:13.710 [ERROR] [warp] the remote printed: see the [DEBUG] lines\n"
        ));
        assert!(!is_debug_line(b"INTEG-SKIP test_x NEEDS: sshd\n"));
        assert!(!is_debug_line(b""));
    }

    #[test]
    fn the_run_records_the_input_order_target_unless_told_otherwise() {
        assert_eq!(
            evidence_rust_log(None, None).as_deref(),
            Some("warp::input_order=debug")
        );
        assert_eq!(
            evidence_rust_log(Some(""), None).as_deref(),
            Some("warp::input_order=debug")
        );
        // The caller's own RUST_LOG is passed on untouched.
        assert_eq!(evidence_rust_log(Some("debug"), None), None);
        assert_eq!(evidence_rust_log(Some("debug"), Some("trace")), None);
        for off in ["off", "0", "", " off "] {
            assert_eq!(evidence_rust_log(None, Some(off)), None, "{off:?}");
        }
        assert_eq!(
            evidence_rust_log(
                None,
                Some("warp::input_order=debug,warpui::frame_cause=debug")
            )
            .as_deref(),
            Some("warp::input_order=debug,warpui::frame_cause=debug")
        );
    }

    #[test]
    fn every_start_of_a_scenario_has_its_own_log() {
        assert_eq!(log_file_name("test_x", "try2", 0), "test_x.try2.log");
        assert_eq!(log_file_name("test_x", "try2", 3), "test_x.try2.rerun3.log");
    }

    #[test]
    fn the_log_gets_every_line_and_the_trace_all_but_debug() {
        let dir = std::env::temp_dir().join(format!("phosphor-integ-ev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("copy.log");
        let file = Mutex::new(File::create(&path).expect("log file"));
        let input: &[u8] = b"12:00:00.000 [INFO] [a] one\n12:00:00.001 [DEBUG] [warp::input_order] two\nlast line without an end";
        let mut shown = Vec::new();
        copy_lines(input, &file, &mut shown);
        assert_eq!(std::fs::read(&path).expect("read back"), input);
        assert_eq!(
            shown,
            b"12:00:00.000 [INFO] [a] one\nlast line without an end"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
