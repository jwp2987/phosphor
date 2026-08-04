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
    /// Tail of captured stderr/stdout, included only on failure to aid
    /// triage without re-running.
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
                for line in detail.lines().rev().take(10).collect::<Vec<_>>().into_iter().rev() {
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
