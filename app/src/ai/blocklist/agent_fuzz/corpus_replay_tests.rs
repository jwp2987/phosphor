//! Replays `corpus/*.json` on every run, honouring `known_failures.toml`.
//!
//! The corpus holds the four Wave 0 scenarios (#1796, #1797 x2, #1799) and a few shape tests. A
//! scenario that is not listed in `known_failures.toml` must pass. A listed one must still fail
//! with the same invariant, and starts failing the run the day it passes (so a fixed bug leaves
//! the list). The Wave 0 coverage floor is checked here too, because under nextest each test is
//! its own process and the coverage table is per process.

use std::collections::BTreeMap;
use std::path::PathBuf;

use fake_llm::scenario::Scenario;
use serde::Deserialize;

use super::coverage::{self, WAVE0_FLOOR};
use super::runner::{run_case, stall_cases_supported};

#[derive(Debug, Deserialize)]
struct KnownFile {
    #[serde(default)]
    known: Vec<Known>,
}

#[derive(Debug, Deserialize)]
struct Known {
    scenario: String,
    invariant: String,
    issue: u32,
}

fn agent_fuzz_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/ai/blocklist/agent_fuzz")
}

fn known_failures() -> BTreeMap<String, Known> {
    let text = std::fs::read_to_string(agent_fuzz_dir().join("known_failures.toml"))
        .expect("known_failures.toml");
    let file: KnownFile = toml::from_str(&text).expect("known_failures.toml parses");
    file.known
        .into_iter()
        .map(|k| (k.scenario.clone(), k))
        .collect()
}

fn corpus() -> Vec<(String, Scenario)> {
    let mut entries: Vec<_> = std::fs::read_dir(agent_fuzz_dir().join("corpus"))
        .expect("corpus directory")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort();
    entries
        .into_iter()
        .map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).unwrap();
            let scenario =
                Scenario::from_json(&text).unwrap_or_else(|e| panic!("corpus/{name}.json: {e}"));
            (name, scenario)
        })
        .collect()
}

#[test]
fn corpus_files_parse_and_are_named_by_their_stem() {
    let entries = corpus();
    assert!(
        entries.len() >= 4,
        "the four Wave 0 scenarios must be in the corpus"
    );
    for want in [
        "wave0_1799_reject_then_transfer",
        "wave0_1797_ask_empty_args",
        "wave0_1797_ask_valid",
        "wave0_1796_silent_then_handoff",
    ] {
        assert!(
            entries.iter().any(|(name, _)| name == want),
            "missing corpus/{want}.json"
        );
    }
    for known in known_failures().keys() {
        assert!(
            entries.iter().any(|(name, _)| name == known),
            "known_failures.toml lists {known:?}, which is not in the corpus"
        );
    }
}

#[test]
fn corpus_replays_as_listed_and_reaches_the_wave0_cells() {
    let known = known_failures();
    let mut problems: Vec<String> = Vec::new();
    for (name, scenario) in corpus() {
        if !stall_cases_supported()
            && scenario
                .turns
                .iter()
                .any(|t| t.chaos.as_ref().is_some_and(|c| c.stall_after.is_some()))
        {
            continue;
        }
        let result = run_case(&scenario);
        match (result, known.get(&name)) {
            (Ok(report), None) => coverage::record(&report),
            (Ok(report), Some(entry)) => {
                coverage::record(&report);
                problems.push(format!(
                    "{name} is listed as failing {} (#{}) but passes: remove it from known_failures.toml",
                    entry.invariant, entry.issue
                ));
            }
            (Err(failure), None) => problems.push(format!("{name}: {failure}")),
            (Err(failure), Some(entry)) => {
                if failure.violation.invariant != entry.invariant {
                    problems.push(format!(
                        "{name} is listed as failing {} (#{}) but now fails {}: {}",
                        entry.invariant,
                        entry.issue,
                        failure.violation.invariant,
                        failure.violation.detail
                    ));
                }
            }
        }
    }
    for cell in WAVE0_FLOOR {
        if coverage::hits(cell) == 0 {
            problems.push(format!(
                "coverage floor: the corpus never reached the cell {cell:?}"
            ));
        }
    }
    if std::env::var("AGENT_FUZZ_COVERAGE").is_ok() {
        eprintln!("{}", coverage::table());
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}
