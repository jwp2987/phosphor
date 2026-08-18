//! Output formatting: one NDJSON object per scenario on stdout, a final
//! `summary` NDJSON object, and a human-readable table on stderr.
//!
//! Format matches `specs/usage-test-suite/SCOPE.md` §3 exactly so downstream
//! consumers (an agent, a CI log scraper) can rely on it.

use serde::Serialize;
use serde_json::{Map, Value};

use crate::manifest::Surface;

/// Outcome of a single scenario run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Skip,
}

impl Status {
    fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Skip => "skip",
        }
    }
}

/// One reported scenario outcome. Serialized as a single NDJSON line.
#[derive(Debug, Clone)]
pub struct ScenarioReport {
    pub surface: Surface,
    pub scenario: String,
    pub status: Status,
    /// Absent (omitted) for skipped scenarios that never ran.
    pub duration_ms: Option<u64>,
    pub tags: Vec<&'static str>,
    /// Set only when `status == Skip`.
    pub reason: Option<String>,
    /// Number of automatic retries consumed (only meaningful for
    /// `needs-real-shell` scenarios using the `RERUN_EXIT_CODE` loop).
    pub retries: Option<u32>,
    /// Captured output for THIS scenario, included only on failure to aid
    /// triage without re-running. For a TUI scenario this is the failing
    /// test's own output section sliced out of the shared batch run (see
    /// `tui_failure_detail`), not the tail of the whole batch — a batch tail
    /// holds the runner's summary rather than the assertion that failed.
    pub failure_detail: Option<String>,
}

impl ScenarioReport {
    /// Render this scenario as a single-line NDJSON `serde_json::Value`
    /// object, matching the field set/order shown in SCOPE.md §3.
    pub fn to_json(&self) -> Value {
        let mut obj = Map::new();
        obj.insert(
            "surface".into(),
            Value::String(self.surface.as_str().into()),
        );
        obj.insert("scenario".into(), Value::String(self.scenario.clone()));
        obj.insert("status".into(), Value::String(self.status.as_str().into()));
        if let Some(ms) = self.duration_ms {
            obj.insert("duration_ms".into(), Value::from(ms));
        }
        obj.insert(
            "tags".into(),
            Value::Array(self.tags.iter().map(|t| Value::String((*t).into())).collect()),
        );
        if let Some(retries) = self.retries {
            obj.insert("retries".into(), Value::from(retries));
        }
        if let Some(reason) = &self.reason {
            obj.insert("reason".into(), Value::String(reason.clone()));
        }
        if let Some(detail) = &self.failure_detail {
            obj.insert("failure_detail".into(), Value::String(detail.clone()));
        }
        Value::Object(obj)
    }
}

/// Aggregate counts, printed as the final NDJSON `summary` line.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub surfaces: SurfaceCounts,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SurfaceCounts {
    pub gui: usize,
    pub tui: usize,
}

impl Summary {
    pub fn from_reports(reports: &[ScenarioReport]) -> Self {
        let mut summary = Summary {
            total: reports.len(),
            passed: 0,
            failed: 0,
            skipped: 0,
            surfaces: SurfaceCounts::default(),
        };
        for report in reports {
            match report.status {
                Status::Pass => summary.passed += 1,
                Status::Fail => summary.failed += 1,
                Status::Skip => summary.skipped += 1,
            }
            match report.surface {
                Surface::Gui => summary.surfaces.gui += 1,
                Surface::Tui => summary.surfaces.tui += 1,
            }
        }
        summary
    }

    /// Render as `{"summary": {...}}`, matching SCOPE.md §3.
    pub fn to_json(&self) -> Value {
        let mut wrapper = Map::new();
        wrapper.insert("summary".into(), serde_json::to_value(self).expect("Summary always serializes"));
        Value::Object(wrapper)
    }
}

/// Print the NDJSON stream (one scenario per line, then the summary line) to
/// stdout.
pub fn print_ndjson(reports: &[ScenarioReport], summary: &Summary) {
    for report in reports {
        println!("{}", report.to_json());
    }
    println!("{}", summary.to_json());
}

/// Lines of `detail` to show under a failing row in the human table.
///
/// This used to be `detail.lines().rev().take(10)` — the LAST ten lines, which
/// for a panic is the backtrace note and the tail of whatever value was
/// printed, never the assertion itself. A rendered-frame comparison (see
/// `usage_tui_transcript_render`) prints ~20 lines below its panic header, so
/// the header was always the first thing dropped. Keep both ends instead, and
/// say how many lines were skipped so nobody reads the excerpt as the whole
/// thing; the untruncated detail is on the scenario's NDJSON line either way.
fn detail_lines_for_table(detail: &str) -> Vec<String> {
    const HEAD: usize = 24;
    const TAIL: usize = 8;

    let lines: Vec<&str> = detail.lines().collect();
    if lines.len() <= HEAD + TAIL + 1 {
        return lines.into_iter().map(str::to_string).collect();
    }

    let skipped = lines.len() - HEAD - TAIL;
    let mut out: Vec<String> = lines[..HEAD].iter().map(|l| (*l).to_string()).collect();
    out.push(format!(
        "... {skipped} more lines (full detail on this scenario's NDJSON line) ..."
    ));
    out.extend(lines[lines.len() - TAIL..].iter().map(|l| (*l).to_string()));
    out
}

/// Print the human-readable table + totals line to stderr, matching the
/// layout shown in SCOPE.md §3.
pub fn print_human_table(reports: &[ScenarioReport], summary: &Summary) {
    eprintln!("{:<8} {:<40} {:<6} {:>8}", "SURFACE", "SCENARIO", "STATUS", "MS");
    for report in reports {
        let status = match report.status {
            Status::Pass => "PASS",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
        };
        let ms = report
            .duration_ms
            .map(|ms| ms.to_string())
            .unwrap_or_else(|| "-".to_string());
        eprintln!(
            "{:<8} {:<40} {:<6} {:>8}",
            report.surface.as_str(),
            report.scenario,
            status,
            ms
        );
        if report.status == Status::Skip {
            if let Some(reason) = &report.reason {
                eprintln!("           reason: {reason}");
            }
        }
        if report.status == Status::Fail {
            if let Some(detail) = &report.failure_detail {
                for line in detail_lines_for_table(detail) {
                    eprintln!("           | {line}");
                }
            }
        }
    }
    eprintln!("-----------------------------------------------------");
    let exit_code = if summary.failed == 0 { 0 } else { 1 };
    eprintln!(
        "{} total | {} passed | {} failed | {} skipped   \u{2192} EXIT {}",
        summary.total, summary.passed, summary.failed, summary.skipped, exit_code
    );
}

#[cfg(test)]
mod table_tests {
    use super::detail_lines_for_table;

    #[test]
    fn a_short_detail_is_shown_whole() {
        let detail = "thread 'x' panicked\nassertion failed\n  left: a\n right: b";
        assert_eq!(detail_lines_for_table(detail).len(), 4);
    }

    /// The panic header is the first line of a failure detail and the single
    /// most useful one; a tail-only excerpt dropped it.
    #[test]
    fn a_long_detail_keeps_its_first_line_and_its_last() {
        let mut detail = String::from("thread 'x' panicked at src/x.rs:1:1:\n");
        for i in 0..200 {
            detail.push_str(&format!("frame line {i}\n"));
        }
        detail.push_str("LAST LINE");

        let shown = detail_lines_for_table(&detail);
        let first = shown.first().expect("excerpt is never empty").as_str();
        let last = shown.last().expect("excerpt is never empty").as_str();
        assert_eq!(first, "thread 'x' panicked at src/x.rs:1:1:");
        assert_eq!(last, "LAST LINE");
        assert!(shown.iter().any(|line| line.contains("more lines")));
        assert!(shown.len() < 40, "excerpt should stay table-sized");
    }
}
