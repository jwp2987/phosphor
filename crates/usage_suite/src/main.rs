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

/// Max bytes of captured output kept in a failure report when the detail is
/// the WHOLE batch's output — every TUI test's noise plus cargo's build lines.
/// Deliberately small: that blob is mostly irrelevant to any one scenario, and
/// it would otherwise be repeated verbatim on every failing scenario's NDJSON
/// line.
const FAILURE_DETAIL_MAX_BYTES: usize = 4000;

/// Max bytes kept when the detail is ONE test's own failure section, sliced
/// out of the batch output by [`failure_section_for`].
///
/// Four times the batch cap, because this budget is spent entirely on the
/// failing test rather than shared with five passing ones. It is sized for the
/// payload that motivated it: `usage_tui_transcript_render` asserts on a
/// rendered frame and prints the whole thing in its panic message — 20 lines ×
/// 80 columns ≈ 1.7 KB, and the widest TUI scenario presents 40 × 120 ≈ 5 KB.
/// A doubled 8 KB cap would still clip a frame printed twice (`left`/`right`);
/// 16 KB holds one comfortably, and [`clamp_preserving_ends`] keeps both ends
/// of anything larger.
const PER_TEST_FAILURE_DETAIL_MAX_BYTES: usize = 16_000;

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

    /// Deprecated no-op: real-shell scenarios now run by DEFAULT. Kept so
    /// existing invocations and CI workflows keep working.
    #[arg(long)]
    include_flaky: bool,

    /// Skip `needs-real-shell` scenarios (they drive a real PTY shell to
    /// command completion).
    ///
    /// These were opt-in until 2026-08-12 because of a shell-preexec race.
    /// Measured across CI on that date: all five pass cleanly in ~3.5s each,
    /// no retries needed — the race is a property of the maintainer's sandbox,
    /// not of the scenarios. Gating them meant the DEFAULT suite for a
    /// terminal emulator never once ran a shell command, which is the single
    /// most fundamental thing it does. Now default-on, with this escape hatch
    /// for hosts where the race does bite (the integration binary's
    /// RERUN_EXIT_CODE loop already retries up to 10 times before failing).
    #[arg(long)]
    exclude_real_shell: bool,

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

/// Whether this host actually has a desktop session a GPU-window scenario can
/// use.
///
/// This used to be assumed absent and `NeedsDesktop` was hard-skipped
/// everywhere. That was true only while Linux-headless was the sole runner;
/// since 2026-08-11 the suite also runs on macOS and Windows CI runners, which
/// have real desktop sessions, and it has always run on maintainer machines
/// that do too. Hard-skipping there reported "skipped" for scenarios that
/// would have run — a false negative that hides breakage.
fn has_desktop_session() -> bool {
    if cfg!(target_os = "macos") || cfg!(target_os = "windows") {
        // A logged-in macOS/Windows runner or workstation always has a window
        // server; there is no env var to consult that means anything more.
        return true;
    }
    // X11 or Wayland, including the `xvfb-run` wrapper CI uses on Linux —
    // xvfb is a real X server, so a scenario that only needs a window (rather
    // than a physical GPU) genuinely can run under it.
    // KNOWN LIMIT: this checks that a display is CONFIGURED, not that it is
    // LIVE. `DISPLAY=:0` on a host with no X server passes, and the scenario
    // then fails rather than skipping. Deliberate: a set-but-broken DISPLAY is
    // a misconfiguration worth surfacing, and probing liveness would mean
    // opening a connection from a gate that has to stay cheap. CI is
    // unaffected — `xvfb-run` sets a DISPLAY that works.
    //
    // Non-EMPTY, not merely present: `DISPLAY=` exports an empty value, and
    // `var_os(..).is_some()` is true for it — which would report a desktop on
    // a host that has none. Caught by testing the negative case.
    let set = |k: &str| {
        std::env::var_os(k)
            .map(|v| !v.is_empty())
            .unwrap_or(false)
    };
    set("DISPLAY") || set("WAYLAND_DISPLAY")
}

/// Decides whether a tag-gated scenario should be skipped, returning the
/// skip reason to report when it should not run. `None` means "run it".
fn skip_reason(scenario: &Scenario, args: &Args) -> Option<String> {
    if scenario.has_tag(Tag::NeedsDesktop) && !has_desktop_session() {
        return Some("needs-desktop (no DISPLAY/WAYLAND_DISPLAY on this host)".into());
    }
    if scenario.has_tag(Tag::NeedsRealShell) && args.exclude_real_shell {
        return Some("needs-real-shell (--exclude-real-shell)".into());
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
    // Per-test attribution when nextest is available. Previously ONE batch
    // outcome was stamped onto every scenario, so a run where 2 passed and 1
    // failed reported all 6 as failed. That is not cosmetic: on the first
    // Windows run (2026-08-11) it turned a partial result into "0/13", which
    // reads as "the platform is entirely broken" and hides which scenarios
    // actually work.
    let (batch_ok, captured) = if nextest_available {
        run_tui_tests_nextest_capturing()
    } else {
        match run_tui_tests_cargo_test() {
            Ok(()) => (true, String::new()),
            Err(detail) => (false, detail),
        }
    };
    let per_test = if nextest_available {
        parse_nextest_outcomes(&captured)
    } else {
        std::collections::HashMap::new()
    };
    let run_result: std::result::Result<(), String> = if batch_ok {
        Ok(())
    } else {
        Err(captured.clone())
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
                // Trust the parsed per-test verdict when we have one; fall
                // back to the batch outcome only for tests nextest never
                // reported (e.g. the run died before reaching them).
                let passed = per_test.get(scenario.name).copied().unwrap_or(false);
                reports.push(ScenarioReport {
                    surface: scenario.surface,
                    scenario: scenario.name.into(),
                    status: if passed { Status::Pass } else { Status::Fail },
                    duration_ms: Some(duration_ms),
                    tags: scenario.tag_strs(),
                    reason: None,
                    retries: None,
                    failure_detail: if passed {
                        None
                    } else {
                        Some(tui_failure_detail(&detail, scenario.name))
                    },
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

/// Like [`run_tui_tests_nextest`] but returns the combined output whether or
/// not the batch succeeded, so per-test outcomes can be attributed.
fn run_tui_tests_nextest_capturing() -> (bool, String) {
    match Command::new("cargo")
        .args([
            "nextest",
            "run",
            "-p",
            "warp_tui",
            "-E",
            "test(/(^|::)usage_tui_/)",
            "--no-fail-fast",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
    {
        Ok(o) => {
            let combined = format!(
                "{}\n{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            (o.status.success(), combined)
        }
        Err(err) => (false, format!("failed to spawn `cargo`: {err}")),
    }
}

fn run_tui_tests_cargo_test() -> std::result::Result<(), String> {
    run_and_capture(
        "cargo",
        &["test", "-p", "warp_tui", "--lib", "usage_tui_"],
    )
}

/// Strips ANSI SGR escapes so nextest's coloured output can be parsed.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Consume through the final byte of a CSI sequence.
            for c2 in chars.by_ref() {
                if c2.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Extracts per-test outcomes from nextest's human output.
///
/// nextest prints one line per test result, e.g.
///   `        PASS [   0.106s] warp_tui usage_smoke_tests::usage_tui_foo`
///   `  TRY 1 FAIL [   0.120s] warp_tui usage_smoke_tests::usage_tui_bar`
///
/// Returns a map keyed on the FINAL `::` segment (the bare test-fn name the
/// manifest uses). A test appearing both FAIL-then-PASS across retries ends up
/// `true`, which matches nextest's own final verdict.
///
/// Parsing the human output rather than nextest's experimental
/// `libtest-json` stream: the latter needs an unstable opt-in env var and
/// changes shape between releases, while these two words have been stable for
/// years and a miss is safe — unmatched scenarios fall back to the batch
/// outcome.
fn parse_nextest_outcomes(output: &str) -> std::collections::HashMap<String, bool> {
    let mut map = std::collections::HashMap::new();
    for raw in strip_ansi(output).lines() {
        let line = raw.trim();
        let passed = if line.starts_with("PASS ") || line.contains(" PASS ") {
            true
        } else if line.contains("FAIL ") {
            false
        } else {
            continue;
        };
        let Some(last) = line.split_whitespace().last() else {
            continue;
        };
        if !last.contains("usage_tui_") {
            continue;
        }
        let name = last.rsplit("::").next().unwrap_or(last).to_string();
        map.entry(name)
            .and_modify(|v| *v = *v || passed)
            .or_insert(passed);
    }
    map
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

/// The `failure_detail` to report for ONE failing TUI scenario, given the
/// whole batch's captured output.
///
/// One nextest process runs every `usage_tui_*` test, so `captured` holds the
/// build lines, six status lines, and every failing test's captured output.
/// Reporting `tail(captured, ..)` for each failing scenario keeps the LAST few
/// KB of that blob — which is nextest's trailing summary, not the assertion
/// that failed. That is how `usage_tui_transcript_render`'s Windows failure
/// stayed unreadable: the run said which scenario failed and then handed back
/// a clipped tail with no panic message in it, and the only machine that
/// reproduces it is a CI runner (TODO.md, Windows TUI entry).
///
/// So slice out the failing test's own section first and spend the (larger)
/// per-test budget on that. The whole-batch tail survives only as the fallback
/// for when no section can be found — a run that died before the test printed
/// anything, say — and says so, so a clipped tail is never again mistaken for
/// "this is all the detail that exists".
fn tui_failure_detail(captured: &str, test_name: &str) -> String {
    let section = failure_section_for(captured, test_name)
        // Second chance, independent of how the runner decorates its banners:
        // a Rust test panic names the test in its THREAD name
        // (`thread 'usage_smoke_tests::usage_tui_foo' panicked at ...`), which
        // libtest has printed that way for years and nextest passes through
        // verbatim. This is what keeps the fix working if nextest restyles the
        // banner again.
        .or_else(|| panic_section_for(captured, test_name));

    match section {
        Some(section) => clamp_preserving_ends(&section, PER_TEST_FAILURE_DETAIL_MAX_BYTES),
        None => format!(
            "(no per-test output section for `{test_name}` in the batch output; \
             showing the tail of the whole batch)\n{}",
            tail(captured, FAILURE_DETAIL_MAX_BYTES)
        ),
    }
}

/// Extracts the captured-output sections belonging to `test_name` from a
/// batch runner's output, or `None` when it printed none.
///
/// Handles both shapes the suite can produce:
///   * nextest — `--- STDOUT: warp_tui usage_smoke_tests::usage_tui_foo ---`
///     followed by the same for `STDERR` (which is where the panic lands);
///   * libtest, used by the `cargo test` fallback when nextest is absent —
///     `---- usage_smoke_tests::usage_tui_foo stdout ----`.
///
/// A header is recognized by decoration + the word STDOUT/STDERR + the test
/// name on the same line, rather than by an exact prefix, because nextest has
/// changed the decoration and the column padding of that banner between
/// releases and CI installs whatever is latest. A section runs until the next
/// banner or the next runner status line; missing the end of a section costs
/// some extra context in the report, missing the start would cost the panic.
fn failure_section_for(captured: &str, test_name: &str) -> Option<String> {
    let plain = strip_ansi(captured);
    let mut collected: Vec<&str> = Vec::new();
    let mut in_section = false;

    for line in plain.lines() {
        if is_output_banner(line) {
            in_section = banner_names_test(line, test_name);
            if in_section {
                collected.push(line);
            }
            continue;
        }
        if in_section {
            if is_runner_status_line(line) {
                in_section = false;
                continue;
            }
            collected.push(line);
        }
    }

    if collected.is_empty() {
        return None;
    }
    // Trailing blank lines carry nothing; the leading banner is kept because
    // it names which stream the text below came from.
    while collected.last().is_some_and(|line| line.trim().is_empty()) {
        collected.pop();
    }
    Some(collected.join("\n"))
}

/// Extracts the panic block whose panicking THREAD is `test_name`, for runner
/// output whose banners [`failure_section_for`] did not recognize.
///
/// libtest names the test's thread after the test, so the panic line reads
/// `thread 'usage_smoke_tests::usage_tui_foo' panicked at <file>:<line>:` and
/// the assertion message follows it. Runs to the next banner or runner status
/// line, with a hard line cap so a runner that prints no terminator at all
/// cannot swallow the rest of the batch.
fn panic_section_for(captured: &str, test_name: &str) -> Option<String> {
    /// Generous next to any real assertion message (a 40-line rendered frame
    /// printed twice is 80), small enough to bound the damage.
    const MAX_PANIC_SECTION_LINES: usize = 400;

    let plain = strip_ansi(captured);
    let lines: Vec<&str> = plain.lines().collect();
    let start = lines.iter().position(|line| {
        line.contains("panicked at")
            && line
                .split(['\'', '"'])
                .any(|segment| segment.rsplit("::").next().unwrap_or(segment) == test_name)
    })?;

    let mut collected: Vec<&str> = Vec::new();
    for &line in &lines[start..] {
        if is_output_banner(line) || is_runner_status_line(line) {
            break;
        }
        collected.push(line);
        if collected.len() >= MAX_PANIC_SECTION_LINES {
            break;
        }
    }
    while collected.last().is_some_and(|line| line.trim().is_empty()) {
        collected.pop();
    }
    (!collected.is_empty()).then(|| collected.join("\n"))
}

/// Whether `line` is a captured-output banner (`--- STDERR: … ---`,
/// `---- … stdout ----`, and the unicode-ruled variants nextest has shipped).
fn is_output_banner(line: &str) -> bool {
    let trimmed = line.trim_start();
    let decorated = trimmed.starts_with(['-', '=', '\u{2500}', '\u{2015}', '\u{2501}']);
    if !decorated {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.contains("stdout") || lower.contains("stderr")
}

/// Whether an output banner names `test_name`. Test paths on the banner carry
/// their module (`usage_smoke_tests::usage_tui_foo`); the manifest stores the
/// bare function name, so compare on the final `::` segment as the outcome
/// parser does.
fn banner_names_test(line: &str, test_name: &str) -> bool {
    line.split_whitespace().any(|word| {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != ':');
        word.rsplit("::").next().unwrap_or(word) == test_name
    })
}

/// Whether `line` is the runner talking rather than a test's own output —
/// nextest's per-test verdicts and its final summary. Used only to close a
/// section that is already open.
fn is_runner_status_line(line: &str) -> bool {
    let trimmed = line.trim();
    let Some(first) = trimmed.split_whitespace().next() else {
        return false;
    };
    let is_status_word = matches!(
        first,
        "PASS" | "FAIL" | "TRY" | "SLOW" | "LEAK" | "TIMEOUT" | "SIGSEGV" | "SIGABRT" | "Summary"
    );
    // Every one of those lines carries a bracketed duration; requiring it
    // keeps a panic message that happens to open with the word FAIL from
    // silently ending the section.
    is_status_word && trimmed.contains('[')
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

/// Truncates from the MIDDLE, keeping both ends of `s`.
///
/// [`tail`] is wrong for an assertion failure: the panic header, the source
/// location and the `left:` value all sit at the top of the section and the
/// `right:` value at the bottom, so keeping only one end loses half of any
/// diff. Two thirds of the budget goes to the head (panic message plus the
/// start of the expectation), one third to the tail.
fn clamp_preserving_ends(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let elided = s.len() - max_bytes;
    let head_budget = max_bytes * 2 / 3;
    let tail_budget = max_bytes - head_budget;

    let head_end = (0..=head_budget)
        .rev()
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(0);
    let tail_start = (s.len() - tail_budget..s.len())
        .find(|&i| s.is_char_boundary(i))
        .unwrap_or(s.len());

    format!(
        "{}\n... [{elided} bytes elided from the middle] ...\n{}",
        &s[..head_end],
        &s[tail_start..]
    )
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    /// Real nextest output shape, including the ANSI colouring it emits in CI
    /// and the `TRY n FAIL` prefix it uses for retried tests.
    const SAMPLE: &str = concat!(
        "        \u{1b}[32;1mPASS\u{1b}[0m [   0.106s] \u{1b}[35;1mwarp_tui\u{1b}[0m ",
        "usage_smoke_tests::usage_tui_completions_menu\n",
        "        PASS [   0.182s] warp_tui usage_smoke_tests::usage_tui_slash_command_palette\n",
        "  TRY 1 FAIL [   0.120s] warp_tui usage_smoke_tests::usage_tui_transcript_render\n",
        "     Summary [   0.3s] 3 tests run: 2 passed, 1 failed\n",
    );

    #[test]
    fn strip_ansi_removes_sgr_sequences() {
        assert_eq!(strip_ansi("\u{1b}[32;1mPASS\u{1b}[0m x"), "PASS x");
    }

    #[test]
    fn parses_per_test_pass_and_fail() {
        let got = parse_nextest_outcomes(SAMPLE);
        assert_eq!(got.get("usage_tui_completions_menu"), Some(&true));
        assert_eq!(got.get("usage_tui_slash_command_palette"), Some(&true));
        assert_eq!(
            got.get("usage_tui_transcript_render"),
            Some(&false),
            "a failing test must not be reported as passing"
        );
    }

    #[test]
    fn summary_line_is_not_mistaken_for_a_test_result() {
        // "2 passed, 1 failed" contains neither a usage_tui_ name nor a
        // PASS/FAIL token in test-result position; it must not add entries.
        let got = parse_nextest_outcomes(SAMPLE);
        assert_eq!(got.len(), 3, "expected exactly the 3 real tests, got {got:?}");
    }

    #[test]
    fn a_retried_test_that_eventually_passes_counts_as_passing() {
        let retried = concat!(
            "  TRY 1 FAIL [ 0.1s] warp_tui usage_smoke_tests::usage_tui_flaky\n",
            "        PASS [ 0.1s] warp_tui usage_smoke_tests::usage_tui_flaky\n",
        );
        assert_eq!(parse_nextest_outcomes(retried).get("usage_tui_flaky"), Some(&true));
    }

    /// A batch run where one of six TUI tests fails, in nextest's real
    /// shape: the failing test's captured streams are printed as banner-led
    /// sections, and the run's own summary follows them.
    const BATCH_WITH_ONE_FAILURE: &str = concat!(
        "   Compiling warp_tui v0.1.0\n",
        "    Starting 6 tests across 1 binary\n",
        "        PASS [   0.106s] warp_tui usage_smoke_tests::usage_tui_completions_menu\n",
        "        FAIL [   0.120s] warp_tui usage_smoke_tests::usage_tui_transcript_render\n",
        "\n",
        "--- STDOUT:              warp_tui usage_smoke_tests::usage_tui_transcript_render ---\n",
        "\n",
        "running 1 test\n",
        "test usage_smoke_tests::usage_tui_transcript_render ... FAILED\n",
        "\n",
        "--- STDERR:              warp_tui usage_smoke_tests::usage_tui_transcript_render ---\n",
        "\n",
        "thread 'usage_tui_transcript_render' panicked at crates/warp_tui/src/usage_smoke_tests.rs:139:9:\n",
        "transcript should render the command input:\n",
        "  > echo\r\n",
        "  hello\r\n",
        "\n",
        "   Summary [   0.300s] 6 tests run: 5 passed, 1 failed, 0 skipped\n",
    );

    #[test]
    fn per_test_detail_carries_the_panic_not_the_batch_summary() {
        let detail = tui_failure_detail(BATCH_WITH_ONE_FAILURE, "usage_tui_transcript_render");

        // The whole point: the assertion message survives.
        assert!(
            detail.contains("transcript should render the command input"),
            "the panic message must be in the reported detail, got:\n{detail}"
        );
        assert!(detail.contains("usage_smoke_tests.rs:139:9"));
        assert!(detail.contains("hello"));

        // And the runner's own chatter around it does not crowd it out.
        assert!(!detail.contains("Compiling warp_tui"));
        assert!(!detail.contains("6 tests run: 5 passed"));
        assert!(!detail.contains("usage_tui_completions_menu"));
    }

    #[test]
    fn a_test_with_no_section_falls_back_to_the_batch_tail_and_says_so() {
        let detail = tui_failure_detail(BATCH_WITH_ONE_FAILURE, "usage_tui_never_reported");
        assert!(
            detail.starts_with("(no per-test output section"),
            "a fallback tail must announce itself, got:\n{detail}"
        );
        assert!(detail.contains("6 tests run: 5 passed"));
    }

    /// The `cargo test` fallback path (no nextest installed) uses libtest's
    /// banner shape instead. Both must resolve to the same panic text.
    #[test]
    fn libtest_banner_shape_is_also_recognized() {
        let libtest = concat!(
            "running 6 tests\n",
            "test usage_smoke_tests::usage_tui_transcript_render ... FAILED\n",
            "\n",
            "---- usage_smoke_tests::usage_tui_transcript_render stdout ----\n",
            "thread 'main' panicked at usage_smoke_tests.rs:139:9:\n",
            "transcript should render the command input\n",
            "\n",
            "failures:\n",
            "    usage_smoke_tests::usage_tui_transcript_render\n",
        );
        let section = failure_section_for(libtest, "usage_tui_transcript_render")
            .expect("libtest prints a `---- <test> stdout ----` banner");
        assert!(section.contains("transcript should render the command input"));
        assert!(!section.contains("running 6 tests"));
    }

    #[test]
    fn ansi_coloured_banners_are_still_matched() {
        let coloured = concat!(
            "\u{1b}[31;1m--- STDERR:\u{1b}[0m warp_tui usage_smoke_tests::usage_tui_x ---\n",
            "panicked: boom\n",
        );
        let section =
            failure_section_for(coloured, "usage_tui_x").expect("colour must not hide the banner");
        assert!(section.contains("panicked: boom"));
    }

    /// A section longer than the budget keeps BOTH ends — a `left`/`right`
    /// comparison is useless with either half missing.
    #[test]
    fn oversized_detail_keeps_both_ends() {
        let body = "x".repeat(50_000);
        let section = format!("assertion `left == right` failed\n{body}\nright: THE-TAIL");
        let clamped = clamp_preserving_ends(&section, 4000);

        assert!(clamped.starts_with("assertion `left == right` failed"));
        assert!(clamped.ends_with("right: THE-TAIL"));
        assert!(clamped.contains("bytes elided from the middle"));
        // The marker adds a little; the payload itself stays within budget.
        assert!(clamped.len() < 4000 + 100, "got {} bytes", clamped.len());
    }

    /// If a future nextest restyles its banner past recognition, the panic's
    /// thread name still identifies the test, and THAT is what has to keep the
    /// assertion out of the whole-batch tail.
    #[test]
    fn an_unrecognized_banner_still_yields_the_panic_via_the_thread_name() {
        let restyled = concat!(
            "        FAIL [   0.120s] warp_tui usage_smoke_tests::usage_tui_transcript_render\n",
            "\u{2501}\u{2501}\u{2501} output \u{2501}\u{2501}\u{2501}\n",
            "thread 'usage_smoke_tests::usage_tui_transcript_render' panicked at src/x.rs:1:1:\n",
            "transcript should render the command input:\n",
            "  > echo\n",
            "   Summary [   0.300s] 6 tests run: 5 passed, 1 failed, 0 skipped\n",
        );
        // The banner says neither STDOUT nor STDERR, so the banner scan misses.
        assert!(failure_section_for(restyled, "usage_tui_transcript_render").is_none());

        let detail = tui_failure_detail(restyled, "usage_tui_transcript_render");
        assert!(
            detail.contains("transcript should render the command input"),
            "the thread-name fallback must still find the panic, got:\n{detail}"
        );
        assert!(!detail.starts_with("(no per-test output section"));
        assert!(!detail.contains("6 tests run: 5 passed"));
    }

    #[test]
    fn a_short_detail_is_passed_through_untouched() {
        let short = "assertion failed: left != right";
        let clamped = clamp_preserving_ends(short, FAILURE_DETAIL_MAX_BYTES);
        assert_eq!(clamped, short);
    }

    #[test]
    fn clamping_never_splits_a_multibyte_char() {
        let wide = "\u{2500}".repeat(4000);
        let clamped = clamp_preserving_ends(&wide, 1000);
        // Reaching here without a panic is the assertion; check it stayed sane.
        assert!(clamped.contains("bytes elided from the middle"));
    }
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::manifest::Surface;

    fn args(argv: &[&str]) -> Args {
        let mut full = vec!["usage-suite"];
        full.extend_from_slice(argv);
        Args::parse_from(full)
    }

    fn scenario(name: &'static str, tags: &'static [Tag]) -> Scenario {
        Scenario {
            surface: Surface::Gui,
            name,
            tags,
        }
    }

    #[test]
    fn without_only_every_scenario_is_selected() {
        let scenario = scenario("usage_launch_bootstrap", &[Tag::ReliableHere]);
        assert!(is_selected(&scenario, &None));
    }

    /// `--only` matches whole names, not prefixes or substrings — otherwise
    /// `--only usage_run_command_exit_code` would also drag in
    /// `usage_run_command_output_block`.
    #[test]
    fn only_matches_whole_names_and_nothing_else() {
        let only = Some(vec![
            "usage_run_command_exit_code".to_string(),
            "usage_tui_transcript_render".to_string(),
        ]);
        assert!(is_selected(
            &scenario("usage_run_command_exit_code", &[Tag::NeedsRealShell]),
            &only
        ));
        assert!(is_selected(
            &scenario("usage_tui_transcript_render", &[Tag::ReliableHere]),
            &only
        ));
        assert!(!is_selected(
            &scenario("usage_run_command", &[Tag::NeedsRealShell]),
            &only
        ));
        assert!(!is_selected(
            &scenario("usage_run_command_exit_code_extra", &[Tag::NeedsRealShell]),
            &only
        ));
    }

    /// An `--only` naming nothing real selects nothing — a typo produces an
    /// empty (exit 0) run, which is worth knowing when a "green" suite ran no
    /// scenarios at all.
    #[test]
    fn an_only_list_that_matches_nothing_selects_nothing() {
        let only = Some(vec!["usage_typo".to_string()]);
        assert!(
            all_scenarios()
                .iter()
                .all(|scenario| !is_selected(scenario, &only))
        );
    }

    #[test]
    fn only_splits_on_commas() {
        let parsed = args(&["--only", "usage_find_in_block,usage_tui_transcript_render"]);
        assert_eq!(
            parsed.only,
            Some(vec![
                "usage_find_in_block".to_string(),
                "usage_tui_transcript_render".to_string(),
            ])
        );
    }

    /// A `reliable-here` scenario is never gated by anything; if it ever
    /// starts skipping, the default suite has quietly stopped running.
    #[test]
    fn a_reliable_here_scenario_is_never_skipped() {
        let scenario = scenario("usage_launch_bootstrap", &[Tag::ReliableHere]);
        assert!(skip_reason(&scenario, &args(&[])).is_none());
        assert!(skip_reason(&scenario, &args(&["--exclude-real-shell"])).is_none());
        assert!(skip_reason(&scenario, &args(&["--include-byop"])).is_none());
    }

    /// Real-shell scenarios have run by default since 2026-08-12; the opt-out
    /// is `--exclude-real-shell`, and its reason names the flag so a reader of
    /// the report knows how the skip was requested.
    #[test]
    fn real_shell_scenarios_run_by_default_and_skip_only_on_the_opt_out() {
        let scenario = scenario("usage_run_command_output_block", &[Tag::NeedsRealShell]);
        assert!(skip_reason(&scenario, &args(&[])).is_none());

        let reason = skip_reason(&scenario, &args(&["--exclude-real-shell"]))
            .expect("--exclude-real-shell skips real-shell scenarios");
        assert!(reason.contains("needs-real-shell"));
        assert!(reason.contains("--exclude-real-shell"));
    }

    /// `--include-flaky` is a retained no-op. It must not resurrect the old
    /// gate, and it must not accidentally *become* the opt-out either.
    #[test]
    fn include_flaky_is_still_accepted_and_still_does_nothing() {
        let scenario = scenario("usage_run_command_output_block", &[Tag::NeedsRealShell]);
        assert!(skip_reason(&scenario, &args(&["--include-flaky"])).is_none());
        assert!(args(&["--include-flaky"]).include_flaky);
    }

    /// BYOP scenarios need a real provider key and network, so they are the
    /// one category gated *off* by default; running them unasked would spend
    /// the user's own money.
    #[test]
    fn byop_scenarios_are_gated_off_until_asked_for() {
        let scenario = scenario("usage_agent_roundtrip", &[Tag::NeedsByopProvider]);
        let reason = skip_reason(&scenario, &args(&[]))
            .expect("byop scenarios do not run without --include-byop");
        assert!(reason.contains("needs-byop-provider"));
        assert!(skip_reason(&scenario, &args(&["--include-byop"])).is_none());
    }

    /// Desktop-gated scenarios follow the host, not a flag: the skip decision
    /// must agree with `has_desktop_session()` on whatever host this runs on,
    /// so the test states the relationship rather than a fixed answer.
    #[test]
    fn desktop_scenarios_are_gated_on_the_host_having_a_desktop() {
        let scenario = scenario("usage_font_size_window_resize", &[Tag::NeedsDesktop]);
        let skipped = skip_reason(&scenario, &args(&[]));
        assert_eq!(
            skipped.is_none(),
            has_desktop_session(),
            "the needs-desktop gate must track has_desktop_session()"
        );
        if let Some(reason) = skipped {
            assert!(reason.contains("needs-desktop"));
        }
    }

    /// A skip report is a report that never ran: no duration, and a reason
    /// the human table can print under the row.
    #[test]
    fn a_skip_report_has_no_duration_and_keeps_the_scenarios_tags() {
        let scenario = scenario("usage_agent_roundtrip", &[Tag::NeedsByopProvider]);
        let report = skip_report(&scenario, "because".to_string());
        assert_eq!(report.status, Status::Skip);
        assert_eq!(report.scenario, "usage_agent_roundtrip");
        assert!(report.duration_ms.is_none());
        assert!(report.failure_detail.is_none());
        assert!(report.retries.is_none());
        assert_eq!(report.reason.as_deref(), Some("because"));
        assert_eq!(report.tags, vec!["needs-byop-provider"]);
    }

    #[test]
    fn surface_defaults_to_both_and_parses_either_side() {
        assert_eq!(args(&[]).surface, SurfaceArg::Both);
        assert_eq!(args(&["--surface", "gui"]).surface, SurfaceArg::Gui);
        assert_eq!(args(&["--surface", "tui"]).surface, SurfaceArg::Tui);
    }

    /// `DISPLAY=` (exported empty) is the case `var_os(..).is_some()` gets
    /// wrong — it would report a desktop on a host that has none, and the
    /// desktop-gated scenario would then fail instead of skipping. Linux-only
    /// because macOS/Windows short-circuit to `true` without consulting the
    /// environment at all.
    ///
    /// This is the only test in the crate that touches process environment.
    /// It restores both variables before returning, and nextest runs each
    /// test in its own process regardless.
    #[test]
    #[cfg(all(unix, not(target_os = "macos")))]
    fn an_empty_display_is_not_a_desktop_session() {
        let saved_display = std::env::var_os("DISPLAY");
        let saved_wayland = std::env::var_os("WAYLAND_DISPLAY");

        // SAFETY: single-threaded test body; both variables are restored
        // below and no other test in this crate reads the environment.
        unsafe {
            std::env::remove_var("WAYLAND_DISPLAY");

            std::env::set_var("DISPLAY", "");
            assert!(
                !has_desktop_session(),
                "an exported-but-empty DISPLAY is not a desktop session"
            );

            std::env::remove_var("DISPLAY");
            assert!(!has_desktop_session(), "no display variables at all");

            std::env::set_var("DISPLAY", ":0");
            assert!(has_desktop_session(), "a configured X display counts");

            std::env::remove_var("DISPLAY");
            std::env::set_var("WAYLAND_DISPLAY", "wayland-0");
            assert!(has_desktop_session(), "a Wayland display counts too");

            std::env::remove_var("WAYLAND_DISPLAY");
            match saved_display {
                Some(value) => std::env::set_var("DISPLAY", value),
                None => std::env::remove_var("DISPLAY"),
            }
            match saved_wayland {
                Some(value) => std::env::set_var("WAYLAND_DISPLAY", value),
                None => std::env::remove_var("WAYLAND_DISPLAY"),
            }
        }
    }
}

#[cfg(test)]
mod tail_tests {
    use super::*;

    /// A GUI scenario's failure detail is the *tail* of the integration
    /// binary's stderr — the last thing it printed before dying is the useful
    /// part, unlike a TUI assertion where both ends matter (see
    /// `clamp_preserving_ends`).
    #[test]
    fn tail_keeps_the_end_and_marks_the_truncation() {
        let long = format!("{}THE-LAST-WORDS", "x".repeat(10_000));
        let cut = tail(&long, 100);
        assert!(cut.ends_with("THE-LAST-WORDS"));
        assert!(cut.starts_with("..."), "a truncated tail must say so");
        assert!(cut.len() <= 103, "got {} bytes", cut.len());
    }

    #[test]
    fn a_short_detail_is_returned_whole_and_unmarked() {
        assert_eq!(
            tail("exit code 1\nboom", FAILURE_DETAIL_MAX_BYTES),
            "exit code 1\nboom"
        );
    }

    /// Byte-based truncation must land on a char boundary; a panic here would
    /// take out the runner while it was reporting someone else's failure.
    #[test]
    fn tail_never_splits_a_multibyte_char() {
        let wide = "\u{2500}".repeat(1000);
        let cut = tail(&wide, 101);
        assert!(cut.starts_with("..."));
        assert!(cut[3..].chars().all(|c| c == '\u{2500}'));
    }
}
