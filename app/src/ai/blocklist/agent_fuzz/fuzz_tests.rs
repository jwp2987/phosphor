//! The generative layer: random scenarios (`fake_llm::strategies::arb_scenario`) through the L1
//! rig, shrunk by proptest to a minimal (script, user) pair that breaks an invariant. The
//! failure message is the scenario as schema JSON; with `AGENT_FUZZ_SAVE=<dir>` the shrunk case
//! is also written there (never into the repo) so it can be copied into `corpus/`.
//!
//! Knobs: `PROPTEST_CASES` (default 24: a case is seconds, not milliseconds),
//! `AGENT_FUZZ_MINUTES` (a time-boxed loop of fresh runs: the unseeded find-it-in-10-minutes
//! mode), `AGENT_FUZZ_SAVE`, `AGENT_FUZZ_COVERAGE=1`.

use std::time::{Duration, Instant};

use fake_llm::scenario::Scenario;
use fake_llm::strategies::arb_scenario;
use proptest::prelude::*;
use proptest::test_runner::{Config, FailurePersistence, TestError, TestRunner};

use super::coverage;
use super::runner::{run_case, stall_cases_supported};

fn config() -> Config {
    let mut config = Config::default();
    if std::env::var("PROPTEST_CASES").is_err() {
        config.cases = 24;
    }
    // A shrink step is a whole app run; bound the search.
    config.max_shrink_iters = 256;
    config.max_shrink_time = 120_000;
    // Never write `proptest-regressions` into the tree; the corpus is the memory.
    config.failure_persistence = None::<Box<dyn FailurePersistence>>;
    config
}

fn has_stall(scenario: &Scenario) -> bool {
    scenario
        .turns
        .iter()
        .any(|t| t.chaos.as_ref().is_some_and(|c| c.stall_after.is_some()))
}

fn scenarios() -> impl Strategy<Value = Scenario> {
    arb_scenario().prop_filter(
        "stall chaos needs PHOSPHOR_BYOP_STREAM_*_TIMEOUT_SECS <= 3 (script/agent_fuzz sets them)",
        |scenario| stall_cases_supported() || !has_stall(scenario),
    )
}

fn check(scenario: Scenario) -> Result<(), TestCaseError> {
    match run_case(&scenario) {
        Ok(report) => {
            coverage::record(&report);
            Ok(())
        }
        Err(failure) => {
            save(&scenario, &failure.violation.invariant);
            Err(TestCaseError::fail(failure.to_string()))
        }
    }
}

/// `AGENT_FUZZ_SAVE=<dir>`: writes the (possibly not yet shrunk) failing scenario there. The last
/// file written for an invariant is the smallest proptest found.
fn save(scenario: &Scenario, invariant: &str) {
    let Ok(dir) = std::env::var("AGENT_FUZZ_SAVE") else {
        return;
    };
    let dir = std::path::PathBuf::from(dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let path = dir.join(format!("{invariant}-seed{}.json", scenario.seed));
    let _ = std::fs::write(path, scenario.to_json());
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn random_agent_conversations_hold_the_invariants(scenario in scenarios()) {
        check(scenario)?;
    }
}

/// A time-boxed run of fresh random cases (`AGENT_FUZZ_MINUTES=10`). Does nothing when unset,
/// so a plain test run stays short.
#[test]
fn random_agent_conversations_for_the_time_box() {
    let Some(minutes) = std::env::var("AGENT_FUZZ_MINUTES")
        .ok()
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|m| *m > 0.0)
    else {
        return;
    };
    let deadline = Instant::now() + Duration::from_secs_f64(minutes * 60.0);
    let mut rounds = 0u32;
    while Instant::now() < deadline {
        let mut runner = TestRunner::new(config());
        if let Err(error) = runner.run(&scenarios(), check) {
            match error {
                TestError::Fail(reason, scenario) => panic!(
                    "round {rounds}: {reason}\n\nshrunk scenario:\n{}",
                    scenario.to_json()
                ),
                TestError::Abort(reason) => panic!("round {rounds}: aborted: {reason}"),
            }
        }
        rounds += 1;
    }
    if std::env::var("AGENT_FUZZ_COVERAGE").is_ok() {
        eprintln!("{rounds} rounds\n{}", coverage::table());
    }
}
