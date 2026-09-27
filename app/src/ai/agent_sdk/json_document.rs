//! The single JSON document `agent run --output-format json` prints (#637).
//!
//! `json` promises one document, but a run's records arrive over time, so they are
//! collected here and printed as one array when the process terminates. `ndjson` is the
//! streaming format and never touches this module.
//!
//! The document is printed from the app's will-terminate hook ([`finish`], called from
//! `on_will_terminate` in `app/src/lib.rs`), not from the driver's completion callback.
//! Every way the CLI process ends normally passes through that hook: run completion and
//! fatal errors (`terminate_app`), and Ctrl-C in headless mode (the SIGINT handler posts a
//! `ForceTerminate`, which leaves the event loop without completing the driver's future).
//! SIGTERM/SIGHUP take the same route once they are handled. A completion callback alone
//! printed nothing on any of those interruptions.
//!
//! The run's outcome is recorded as a final `system` record so a consumer can tell a
//! complete run from an interrupted or failed one:
//! - completed: no extra record;
//! - failed: `{"type":"system","event_type":"run_failed","error":"..."}`;
//! - interrupted (terminated before the run reported an outcome):
//!   `{"type":"system","event_type":"run_interrupted"}`.
//!
//! Records are kept as the compact JSON text the formatters already produced, not as
//! parsed values, so a long run holds one string per record rather than a tree.

use std::io::{self, Write};

use parking_lot::Mutex;
use serde_json::json;

/// How a run ended, as far as the document is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    Completed,
    Failed(String),
}

/// Records collected for one run.
#[derive(Debug, Default)]
struct JsonRunDocument {
    /// Each element is one compact JSON value, as written by the NDJSON formatters.
    records: Vec<String>,
    /// `None` until the run reports how it ended; still `None` at termination means the
    /// run was interrupted.
    outcome: Option<Outcome>,
}

impl JsonRunDocument {
    /// Append the records in one batch of NDJSON output.
    ///
    /// The batch is validated before anything is appended. A record that is not valid
    /// JSON is a formatter bug; rather than silently dropping it (and every record after
    /// it in the batch), the batch is replaced by an `output_error` record that says so,
    /// and the error is returned so the caller reports it.
    fn push_ndjson(&mut self, bytes: &[u8]) -> io::Result<()> {
        let text = std::str::from_utf8(bytes)
            .map_err(|e| io::Error::other(format!("run output is not UTF-8: {e}")));
        let result = text.and_then(|text| {
            let mut batch = Vec::new();
            // The formatters write compact JSON (no raw newlines) plus `\n` per record.
            for line in text.lines().filter(|line| !line.trim().is_empty()) {
                serde_json::from_str::<serde::de::IgnoredAny>(line).map_err(|e| {
                    io::Error::other(format!("run output record is not valid JSON: {e}"))
                })?;
                batch.push(line.to_string());
            }
            Ok(batch)
        });
        match result {
            Ok(batch) => {
                self.records.extend(batch);
                Ok(())
            }
            Err(err) => {
                self.records.push(
                    json!({
                        "type": "system",
                        "event_type": "output_error",
                        "error": err.to_string(),
                    })
                    .to_string(),
                );
                Err(err)
            }
        }
    }

    /// Record how the run ended. The first outcome wins: a failure reported while the
    /// process is already shutting down after completion does not rewrite it.
    fn set_outcome(&mut self, outcome: Outcome) {
        self.outcome.get_or_insert(outcome);
    }

    /// Render the whole document: an array of every record, then the outcome record,
    /// terminated by a newline.
    fn render<W: Write>(self, w: &mut W) -> io::Result<()> {
        let outcome_record = match self.outcome {
            Some(Outcome::Completed) => None,
            Some(Outcome::Failed(error)) => Some(
                json!({ "type": "system", "event_type": "run_failed", "error": error }).to_string(),
            ),
            None => Some(json!({ "type": "system", "event_type": "run_interrupted" }).to_string()),
        };
        let mut records = self
            .records
            .iter()
            .map(String::as_str)
            .chain(outcome_record.as_deref());

        match records.next() {
            None => writeln!(w, "[]"),
            Some(first) => {
                write!(w, "[\n  {first}")?;
                for record in records {
                    write!(w, ",\n  {record}")?;
                }
                writeln!(w, "\n]")
            }
        }
    }
}

/// The document for this process's run. `None` unless `agent run --output-format json`
/// armed it; a process runs at most one agent from the CLI.
static DOCUMENT: Mutex<Option<JsonRunDocument>> = Mutex::new(None);

/// Start collecting a JSON document for this process's `agent run`.
pub(crate) fn arm() {
    *DOCUMENT.lock() = Some(JsonRunDocument::default());
}

/// Append one batch of NDJSON records. A no-op when no document is armed.
pub(crate) fn push_ndjson(bytes: &[u8]) -> io::Result<()> {
    match DOCUMENT.lock().as_mut() {
        Some(document) => document.push_ndjson(bytes),
        None => Ok(()),
    }
}

/// Record that the run completed. A no-op when no document is armed.
pub(crate) fn record_completed() {
    if let Some(document) = DOCUMENT.lock().as_mut() {
        document.set_outcome(Outcome::Completed);
    }
}

/// Record that the run (or its setup) failed. A no-op when no document is armed.
pub(crate) fn record_failed(error: &str) {
    if let Some(document) = DOCUMENT.lock().as_mut() {
        document.set_outcome(Outcome::Failed(error.to_string()));
    }
}

/// Print the document to stdout, exactly once. Called from the app's will-terminate
/// hook; a no-op when no document is armed or it was already printed.
pub fn finish() {
    let Some(document) = DOCUMENT.lock().take() else {
        return;
    };
    let mut stdout = io::stdout().lock();
    if let Err(err) = document.render(&mut stdout).and_then(|()| stdout.flush()) {
        log::warn!("Failed to write the --output-format json document: {err}");
    }
}

#[cfg(test)]
#[path = "json_document_tests.rs"]
mod tests;
