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
use super::runner::{run_case, run_case_with, stall_cases_supported};

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
    // #1851: the hang-shaped question scenarios (truncated or empty arguments, a stream cut or
    // never finished after the call, a missing / empty / repeated call id, a steer on the
    // blocked card) stay in the corpus.
    for want in [
        "hang1851_ask_truncated_args",
        "hang1851_ask_empty_args",
        "hang1851_ask_no_done_after_call",
        "hang1851_ask_cut_after_call",
        "hang1851_ask_cut_then_resume_asks_again",
        "hang1851_ask_stall_after_call",
        "hang1851_ask_missing_id",
        "hang1851_ask_empty_id",
        "hang1851_ask_duplicate_ids",
        "hang1851_ask_then_truncated_ask",
        "hang1851_ask_steer_while_blocked",
        "hang1851_ask_slow_split_args",
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

/// The whole corpus again with the safe read-only allowlist ON. It only removes approvals, so
/// every scenario that passes OFF must pass ON (a listed-OFF known failure is skipped: it is
/// tracked by its issue, not by this run).
#[test]
fn corpus_replays_with_the_allowlist_on() {
    let known = known_failures();
    let mut problems: Vec<String> = Vec::new();
    for (name, scenario) in corpus() {
        if known.contains_key(&name) {
            continue;
        }
        if !stall_cases_supported()
            && scenario
                .turns
                .iter()
                .any(|t| t.chaos.as_ref().is_some_and(|c| c.stall_after.is_some()))
        {
            continue;
        }
        if let Err(failure) = run_case_with(&scenario, true) {
            problems.push(format!("{name} (allowlist on): {failure}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n\n"));
}

/// #2048 (owner decision 2026-11-10): a reply whose body closes with no `End` runs none of the
/// tools the stream runs by itself. `noend_in_stream_tools_do_not_run` is such a reply with a
/// websearch, a webfetch and a todowrite (and one id used for two calls); the rig answers every
/// dispatched search inside the process and counts it (`search_test_hook`), so "no search ran"
/// is the count standing still across the case. The invariants (I4: every call of the reply is
/// in the resume request with one result) are judged by the replay above.
///
/// The first half is the control: the same search in a reply WITH its `End` is dispatched and
/// counted, so the second half cannot pass because searches never reach the hook in this rig.
/// The count is per process, like the coverage table: run under nextest (one process per test).
#[test]
fn a_reply_that_closes_with_no_end_runs_no_search() {
    use crate::ai::agent_providers::tools::web_runtime::search_test_hook;
    use fake_llm::scenario::{Args, Call, CallId, Match, Reply, Turn};

    let mut finished = Scenario::new(2048);
    finished.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls {
            calls: vec![Call::new(CallId::Fresh, "websearch", Args::Valid)],
        },
        chaos: None,
    });
    finished.turns.push(Turn {
        when: Match::Nth { n: 1 },
        reply: Reply::Text {
            text: "done".to_owned(),
        },
        chaos: None,
    });
    let before = search_test_hook::answered();
    let report = run_case(&finished).unwrap_or_else(|failure| panic!("{failure}"));
    assert!(
        search_test_hook::answered() > before,
        "control: a finished reply's search is dispatched"
    );
    assert!(report.requests >= 2, "control: the search result goes back");

    let (_, cut_short) = corpus()
        .into_iter()
        .find(|(name, _)| name == "noend_in_stream_tools_do_not_run")
        .expect("corpus/noend_in_stream_tools_do_not_run.json");
    assert!(
        cut_short.turns[0]
            .chaos
            .as_ref()
            .is_some_and(|chaos| chaos.never_end),
        "the scenario's first reply must close with no End"
    );
    let before = search_test_hook::answered();
    run_case(&cut_short).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(
        search_test_hook::answered(),
        before,
        "a reply with no End dispatched its websearch"
    );
}

/// #2104: a plain Stop while the reply's batch of tool calls is still preprocessing. The two
/// `stop2104_*` scenarios hold every batch in preprocessing (`env.preprocess`): one for good (a
/// hung apply, #1894), one until 30 ms after the stop dropped it (a slow apply's late result),
/// followed by the user's next message.
///
/// The invariants are judged by the replay above, with the oracle as it was. What fails there
/// on the code before #2104: the stop left the batch's calls with no status and no result, the
/// turn watch (#1967) confirmed them as lost three main-loop hops later and logged
/// `hop=agent_turn_stuck fault=unsettled_calls` (an ERROR: I6). This test adds what the replay
/// cannot say: that the Stop really met a batch that was preprocessing (so a scenario that
/// stops too late cannot pass for the wrong reason), how the turn ended, and that the model
/// then got ONE result for the one call, not two (the late result is dropped).
#[test]
fn a_stop_while_the_batch_is_preprocessing_settles_its_calls_and_drops_the_late_result() {
    let entries = corpus();
    let scenario = |name: &str| {
        entries
            .iter()
            .find(|(stem, _)| stem == name)
            .map(|(_, scenario)| scenario.clone())
            .unwrap_or_else(|| panic!("missing corpus/{name}.json"))
    };

    let hung = scenario("stop2104_stop_while_preprocessing_hung_apply");
    assert_eq!(
        hung.env.preprocess,
        Some(fake_llm::scenario::PreprocessBehaviour::Hung)
    );
    let report = run_case(&hung).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(
        (report.steps[0].op, report.steps[0].stage),
        ("stop", Some("preprocessing")),
        "the Stop must meet the batch while it is preprocessing: {:?}",
        report.steps
    );
    assert_eq!(
        report.final_status,
        Some(crate::ai::agent::conversation::ConversationStatus::Cancelled)
    );
    assert_eq!(report.requests, 1, "a stop opens no request (#1942)");

    let late = scenario("stop2104_stop_while_preprocessing_late_result_then_follow_up");
    assert!(matches!(
        late.env.preprocess,
        Some(fake_llm::scenario::PreprocessBehaviour::LateAfterDrop { .. })
    ));
    let report = run_case(&late).unwrap_or_else(|failure| panic!("{failure}"));
    assert_eq!(
        (report.steps[0].op, report.steps[0].stage),
        ("stop", Some("preprocessing")),
        "the Stop must meet the batch while it is preprocessing: {:?}",
        report.steps
    );
    assert_eq!(
        report.requests, 2,
        "the reply's request and the user's next message, nothing else"
    );
    assert_eq!(
        report.tool_messages_in_last_request, 1,
        "the stopped call is answered once in the user's next request: its cancelled result \
         from the stop, and no second one from the batch's late preprocess result"
    );
}
