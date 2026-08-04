//! `usage-suite` — orchestrator for the GUI + TUI usage/acceptance smoke
//! suite. See `specs/usage-test-suite/SCOPE.md` for the full design and
//! `specs/usage-test-suite/README.md` for a short how-to.
//!
//! The runner reads the scenario manifest (`manifest.rs`) — the single source
//! of truth for which GUI/TUI scenarios exist and how they're tagged — and
//! dispatches each to its surface (the `integration` binary for GUI, a nextest
//! `usage_tui_*` filter for TUI).
//!
//! Deliberately dependency-light: `clap` + `serde` + `serde_json` + `anyhow`
//! only, **no** dependency on `warp`/`warpui`/`integration` or any other
//! heavy workspace crate — that independence keeps this crate's own build
//! fast and out of the way of the rest of the workspace (SCOPE.md §2).

mod manifest;
mod report;

use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::Result;
use clap::{Parser, ValueEnum};

use manifest::{all_scenarios, Scenario, Tag, GUI_SCENARIOS, TUI_SCENARIOS};
use report::{print_human_table, print_ndjson, ScenarioReport, Status, Summary};

/// `warpui_core::integration::driver::RERUN_EXIT_CODE` — the exit code the
/// `integration` binary uses to signal "flaked, retry" (see
/// `crates/warpui_core/src/integration/driver.rs`). Duplicated here as a
/// plain constant rather than a dependency so this crate stays dependency-
/// light; the `integration` binary's exit-code contract is effectively
/// public API for its callers (this runner and
/// `crates/integration/tests/common/mod.rs` both rely on it).
const GUI_RERUN_EXIT_CODE: i32 = 127;

/// Mirrors `MAX_TEST_RUNS` in `crates/integration/tests/common/mod.rs`.
const MAX_GUI_RETRIES: u32 = 10;

/// Max bytes of captured output kept in a failure report.
const FAILURE_DETAIL_MAX_BYTES: usize = 4000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[value(rename_all = "lowercase")]
enum SurfaceArg {
    Gui,
    Tui,
    Both,
}

/// Orchestrates the GUI + TUI usage/acceptance smoke suite.
#[derive(Debug, Parser)]
#[command(name = "usage-suite", version)]
struct Args {
    /// Which surface(s) to run.
    #[arg(long, value_enum, default_value = "both")]
    surface: SurfaceArg,

    /// Also run `needs-real-shell` scenarios (drives a real PTY shell to
    /// command completion; subject to the sandbox's shell-preexec race —
    /// auto-retried via the integration binary's RERUN_EXIT_CODE loop).
    #[arg(long)]
    include_flaky: bool,

    /// Also run `needs-byop-provider` scenarios (a real agent round-trip;
    /// requires a real provider key + network).
    #[arg(long)]
    include_byop: bool,

    /// Restrict the run to a comma-separated list of scenario names.
    #[arg(long, value_delimiter = ',')]
    only: Option<Vec<String>>,

    /// Suppress the human-readable table on stderr; NDJSON on stdout only.
    #[arg(long)]
    json: bool,

    /// List every manifest scenario (surface, name, tags) and exit without
    /// running anything. Useful for Chunk B/C to sanity-check newly
    /// appended manifest rows.
    #[arg(long)]
    list: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    if args.list {
        for scenario in all_scenarios() {
            println!(
                "{:<5} {:<40} [{}]",
                scenario.surface.as_str(),
                scenario.name,
                scenario.tag_strs().join(", ")
            );
        }
        return Ok(());
    }

    let run_gui = matches!(args.surface, SurfaceArg::Gui | SurfaceArg::Both);
    let run_tui = matches!(args.surface, SurfaceArg::Tui | SurfaceArg::Both);

    let mut reports = Vec::new();

    if run_gui {
        for scenario in GUI_SCENARIOS {
            if !is_selected(scenario, &args.only) {
                continue;
            }
            reports.push(evaluate_gui_scenario(scenario, &args));
        }
    }

    if run_tui {
        let selected_tui: Vec<&Scenario> = TUI_SCENARIOS
            .iter()
            .filter(|s| is_selected(s, &args.only))
            .collect();
        reports.extend(run_tui_scenarios(&selected_tui, &args));
    }

    let summary = Summary::from_reports(&reports);

    print_ndjson(&reports, &summary);
    if !args.json {
        print_human_table(&reports, &summary);
    }

    if summary.failed == 0 {
        Ok(())
    } else {
        std::process::exit(1);
    }
}

fn is_selected(scenario: &Scenario, only: &Option<Vec<String>>) -> bool {
    match only {
        None => true,
        Some(names) => names.iter().any(|n| n == scenario.name),
    }
}

/// Decides whether a tag-gated scenario should be skipped, returning the
/// skip reason to report when it should not run. `None` means "run it".
fn skip_reason(scenario: &Scenario, args: &Args) -> Option<String> {
    if scenario.has_tag(Tag::NeedsDesktop) {
        return Some("needs-desktop (no real GPU display in this runner)".into());
    }
    if scenario.has_tag(Tag::NeedsRealShell) && !args.include_flaky {
        return Some("needs-real-shell (no --include-flaky)".into());
    }
    if scenario.has_tag(Tag::NeedsByopProvider) && !args.include_byop {
        return Some("needs-byop-provider (no --include-byop)".into());
    }
    None
}

fn skip_report(scenario: &Scenario, reason: String) -> ScenarioReport {
    ScenarioReport {
        surface: scenario.surface,
        scenario: scenario.name.into(),
        status: Status::Skip,
        duration_ms: None,
        tags: scenario.tag_strs(),
        reason: Some(reason),
        retries: None,
        failure_detail: None,
    }
}

// ---------------------------------------------------------------------------
// GUI: spawns the `integration` binary per scenario (SCOPE.md §1.1, §2).
// ---------------------------------------------------------------------------

fn evaluate_gui_scenario(scenario: &Scenario, args: &Args) -> ScenarioReport {
    match skip_reason(scenario, args) {
        Some(reason) => skip_report(scenario, reason),
        None => run_gui_scenario(scenario),
    }
}

/// Spawns `cargo run -q -p integration --bin integration -- <name>`, the
/// same entrypoint `crates/integration/tests/common/mod.rs` uses to drive
/// the GUI, including its `RERUN_EXIT_CODE` retry loop.
///
/// Note: this crate intentionally spawns `cargo`/subprocess commands via
/// `std::process::Command` directly rather than the `crates/command`
/// wrapper (AGENTS.md §5.7 normally requires the wrapper to avoid stray
/// windows popping up on Windows for the shipped app). That rule targets
/// app-shipped code; this crate's dependency budget is deliberately capped
/// at `clap`/`serde`/`serde_json`/`anyhow` by design (SCOPE.md Chunk A), and
/// it is dev/CI tooling only, never bundled into a Zap build.
fn run_gui_scenario(scenario: &Scenario) -> ScenarioReport {
    let start = Instant::now();
    let mut retries = 0u32;
    loop {
        let output = Command::new("cargo")
            .args([
                "run",
                "-q",
                "-p",
                "integration",
                "--bin",
                "integration",
                "--",
                scenario.name,
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let duration_ms = start.elapsed().as_millis() as u64;
        let retries_reported = if retries > 0 { Some(retries) } else { None };

        match output {
            Ok(output) => match output.status.code() {
                Some(0) => {
                    return ScenarioReport {
                        surface: scenario.surface,
                        scenario: scenario.name.into(),
                        status: Status::Pass,
                        duration_ms: Some(duration_ms),
                        tags: scenario.tag_strs(),
                        reason: None,
                        retries: retries_reported,
                        failure_detail: None,
                    };
                }
                Some(GUI_RERUN_EXIT_CODE) if retries < MAX_GUI_RETRIES => {
                    retries += 1;
                    continue;
                }
                other => {
                    let mut detail = String::from_utf8_lossy(&output.stderr).to_string();
                    if detail.trim().is_empty() {
                        detail = String::from_utf8_lossy(&output.stdout).to_string();
                    }
                    let code_note = match other {
                        Some(code) => format!("exit code {code}"),
                        None => "terminated by signal".to_string(),
                    };
                    return ScenarioReport {
                        surface: scenario.surface,
                        scenario: scenario.name.into(),
                        status: Status::Fail,
                        duration_ms: Some(duration_ms),
                        tags: scenario.tag_strs(),
                        reason: None,
                        retries: retries_reported,
                        failure_detail: Some(format!(
                            "{code_note}\n{}",
                            tail(&detail, FAILURE_DETAIL_MAX_BYTES)
                        )),
                    };
                }
            },
            Err(err) => {
                return ScenarioReport {
                    surface: scenario.surface,
                    scenario: scenario.name.into(),
                    status: Status::Fail,
                    duration_ms: Some(duration_ms),
                    tags: scenario.tag_strs(),
                    reason: None,
                    retries: retries_reported,
                    failure_detail: Some(format!("failed to spawn cargo: {err}")),
                };
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TUI: runs `cargo nextest run -p warp_tui -E 'test(/(^|::)usage_tui_/)'` once for
// the whole batch of selected TUI scenarios (SCOPE.md §1.2, §2), falling
// back to `cargo test` if nextest isn't installed.
// ---------------------------------------------------------------------------

fn run_tui_scenarios(scenarios: &[&Scenario], args: &Args) -> Vec<ScenarioReport> {
    let mut reports = Vec::new();
    let mut runnable = Vec::new();

    for scenario in scenarios {
        match skip_reason(scenario, args) {
            Some(reason) => reports.push(skip_report(scenario, reason)),
            None => runnable.push(*scenario),
        }
    }

    if runnable.is_empty() {
        return reports;
    }

    let nextest_available = command_succeeds("cargo", &["nextest", "--version"]);
    if !nextest_available {
        eprintln!(
            "usage-suite: cargo-nextest not found on PATH; falling back to `cargo test` for TUI scenarios"
        );
    }

    let discovered = if nextest_available {
        discover_tui_tests_nextest()
    } else {
        discover_tui_tests_cargo_test()
    };

    let discovered = match discovered {
        Ok(names) => names,
        Err(err) => {
            // Discovery itself failed (e.g. warp_tui doesn't compile). Treat
            // as a real failure rather than silently skipping breakage.
            for scenario in &runnable {
                reports.push(ScenarioReport {
                    surface: scenario.surface,
                    scenario: scenario.name.into(),
                    status: Status::Fail,
                    duration_ms: None,
                    tags: scenario.tag_strs(),
                    reason: None,
                    retries: None,
                    failure_detail: Some(tail(&err.to_string(), FAILURE_DETAIL_MAX_BYTES)),
                });
            }
            return reports;
        }
    };

    // Discovered nextest/libtest names include the module path
    // (`usage_smoke_tests::usage_tui_foo`); the manifest lists the bare function
    // name (`usage_tui_foo`). Compare on the final `::`-segment so the two line
    // up regardless of the module the tests live in.
    let (to_run, not_found): (Vec<&Scenario>, Vec<&Scenario>) =
        runnable.into_iter().partition(|s| {
            discovered
                .iter()
                .any(|name| name.rsplit("::").next().unwrap_or(name.as_str()) == s.name)
        });

    for scenario in &not_found {
        reports.push(skip_report(
            scenario,
            "no matching usage_tui_* test found yet (surface not landed)".into(),
        ));
    }

    if to_run.is_empty() {
        return reports;
    }

    let start = Instant::now();
    let run_result = if nextest_available {
        run_tui_tests_nextest()
    } else {
        run_tui_tests_cargo_test()
    };
    let duration_ms = start.elapsed().as_millis() as u64;

    // A single batch process covers every discovered `usage_tui_*` test, so
    // (for now) all matched-and-selected scenarios share one duration/outcome.
    // True per-test granularity would need parsing nextest's experimental
    // libtest-json event stream; deferred as a follow-up once Chunk C lands
    // enough real tests to make that worth the added parsing surface.
    match run_result {
        Ok(()) => {
            for scenario in &to_run {
                reports.push(ScenarioReport {
                    surface: scenario.surface,
                    scenario: scenario.name.into(),
                    status: Status::Pass,
                    duration_ms: Some(duration_ms),
                    tags: scenario.tag_strs(),
                    reason: None,
                    retries: None,
                    failure_detail: None,
                });
            }
        }
        Err(detail) => {
            for scenario in &to_run {
                reports.push(ScenarioReport {
                    surface: scenario.surface,
                    scenario: scenario.name.into(),
                    status: Status::Fail,
                    duration_ms: Some(duration_ms),
                    tags: scenario.tag_strs(),
                    reason: None,
                    retries: None,
                    failure_detail: Some(tail(&detail, FAILURE_DETAIL_MAX_BYTES)),
                });
            }
        }
    }

    reports
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Runs `cargo nextest list -p warp_tui -E 'test(/(^|::)usage_tui_/)'` with JSON
/// output and returns the set of matched test names.
///
/// The `(^|::)` anchor matches the `usage_tui_` function-name prefix whether the
/// tests sit at the crate root or (as they do today) inside the
/// `usage_smoke_tests` module — a plain `^usage_tui_` would miss the latter
/// because nextest test names include the module path.
fn discover_tui_tests_nextest() -> Result<Vec<String>> {
    let output = Command::new("cargo")
        .args([
            "nextest",
            "list",
            "-p",
            "warp_tui",
            "-E",
            "test(/(^|::)usage_tui_/)",
            "--message-format",
            "json",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "cargo nextest list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let parsed: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|err| anyhow::anyhow!("failed to parse `cargo nextest list` JSON: {err}"))?;

    let mut names = Vec::new();
    if let Some(suites) = parsed.get("rust-suites").and_then(|v| v.as_object()) {
        for suite in suites.values() {
            let Some(testcases) = suite.get("testcases").and_then(|v| v.as_object()) else {
                continue;
            };
            for (name, meta) in testcases {
                let matches = meta
                    .get("filter-match")
                    .and_then(|m| m.get("status"))
                    .and_then(|s| s.as_str())
                    .map(|s| s == "matches")
                    .unwrap_or(true);
                if matches {
                    names.push(name.clone());
                }
            }
        }
    }
    Ok(names)
}

/// Fallback discovery when `cargo-nextest` isn't installed: `cargo test`'s
/// libtest `--list` output, lines of the form `usage_tui_foo: test`.
fn discover_tui_tests_cargo_test() -> Result<Vec<String>> {
    let output = Command::new("cargo")
        .args([
            "test", "-p", "warp_tui", "--lib", "usage_tui_", "--", "--list",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()?;

    if !output.status.success() {
        anyhow::bail!(
            "cargo test --list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let names = stdout
        .lines()
        .filter_map(|line| line.strip_suffix(": test"))
        .map(|name| name.to_string())
        .collect();
    Ok(names)
}

fn run_tui_tests_nextest() -> std::result::Result<(), String> {
    run_and_capture(
        "cargo",
        &[
            "nextest",
            "run",
            "-p",
            "warp_tui",
            "-E",
            "test(/(^|::)usage_tui_/)",
            "--no-fail-fast",
        ],
    )
}

fn run_tui_tests_cargo_test() -> std::result::Result<(), String> {
    run_and_capture(
        "cargo",
        &["test", "-p", "warp_tui", "--lib", "usage_tui_"],
    )
}

/// Runs a command to completion, capturing stdout+stderr.
///
/// Uses `Command::output()` (not manual `spawn()` + sequential pipe reads)
/// specifically because `output()` drains both pipes concurrently — reading
/// one to completion before touching the other risks a classic deadlock if
/// the child fills the other pipe's OS buffer while blocked (a real
/// possibility for verbose `cargo`/`nextest` output).
fn run_and_capture(program: &str, args: &[&str]) -> std::result::Result<(), String> {
    let output = match Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(output) => output,
        Err(err) => return Err(format!("failed to spawn `{program}`: {err}")),
    };

    if output.status.success() {
        Ok(())
    } else {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("{stdout}\n{stderr}"))
    }
}

/// Returns the last `max_bytes` of `s`, truncated on a char boundary.
fn tail(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let cut = s.len() - max_bytes;
    let boundary = (cut..s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());
    format!("...{}", &s[boundary..])
}
