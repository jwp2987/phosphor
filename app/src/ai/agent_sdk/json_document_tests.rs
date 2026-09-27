//! #637: `agent run --output-format json` prints one valid JSON document however the run
//! ends — completed, failed, or interrupted (Ctrl-C / SIGTERM).

use super::{JsonRunDocument, Outcome};
use crate::ai::agent_sdk::driver::output::json;

fn render(document: JsonRunDocument) -> String {
    let mut out = Vec::new();
    document.render(&mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// Parse the rendered output as ONE JSON value and return its array.
fn parse(rendered: &str) -> Vec<serde_json::Value> {
    assert!(rendered.ends_with('\n'), "document must end with a newline");
    let value: serde_json::Value =
        serde_json::from_str(rendered).expect("output must be a single JSON document");
    value.as_array().expect("document must be an array").clone()
}

fn two_record_batch() -> Vec<u8> {
    let mut ndjson = Vec::new();
    json::conversation_started("conv-1", &mut ndjson).unwrap();
    json::run_started("run-1", &mut ndjson).unwrap();
    ndjson
}

#[test]
fn completed_run_is_its_records_and_nothing_else() {
    let mut document = JsonRunDocument::default();
    document.push_ndjson(&two_record_batch()).unwrap();
    document.set_outcome(Outcome::Completed);

    let records = parse(&render(document));

    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["conversation_id"], "conv-1");
    assert_eq!(records[1]["run_id"], "run-1");
}

#[test]
fn interrupted_run_keeps_its_records_and_says_it_was_interrupted() {
    // No outcome recorded: the process was terminated (Ctrl-C, SIGTERM) mid-run. The
    // records so far must still be printed, as a valid document.
    let mut document = JsonRunDocument::default();
    document.push_ndjson(&two_record_batch()).unwrap();

    let records = parse(&render(document));

    assert_eq!(records.len(), 3);
    assert_eq!(records[0]["conversation_id"], "conv-1");
    assert_eq!(records[2]["type"], "system");
    assert_eq!(records[2]["event_type"], "run_interrupted");
}

#[test]
fn failed_run_ends_with_its_error() {
    let mut document = JsonRunDocument::default();
    document.push_ndjson(&two_record_batch()).unwrap();
    document.set_outcome(Outcome::Failed("provider rejected the key".to_string()));

    let records = parse(&render(document));

    assert_eq!(records.len(), 3);
    assert_eq!(records[2]["event_type"], "run_failed");
    assert_eq!(records[2]["error"], "provider rejected the key");
}

#[test]
fn setup_failure_before_any_record_is_still_a_document() {
    let mut document = JsonRunDocument::default();
    document.set_outcome(Outcome::Failed("Skill 'x' not found".to_string()));

    let records = parse(&render(document));

    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["event_type"], "run_failed");
}

#[test]
fn completed_run_with_no_records_is_an_empty_array() {
    let mut document = JsonRunDocument::default();
    document.set_outcome(Outcome::Completed);

    assert_eq!(render(document), "[]\n");
}

#[test]
fn first_outcome_wins() {
    let mut document = JsonRunDocument::default();
    document.set_outcome(Outcome::Completed);
    document.set_outcome(Outcome::Failed("late".to_string()));

    assert_eq!(render(document), "[]\n");
}

#[test]
fn invalid_record_fails_loudly_instead_of_truncating() {
    let mut document = JsonRunDocument::default();
    document.push_ndjson(&two_record_batch()).unwrap();

    let err = document
        .push_ndjson(b"{\"type\":\"agent\",\"text\":\"ok\"}\n{not json\n")
        .expect_err("a malformed record must be an error");
    assert!(err.to_string().contains("not valid JSON"), "{err}");
    document.set_outcome(Outcome::Completed);

    let records = parse(&render(document));

    // The earlier batch survives; the bad batch is replaced by one visible error record
    // rather than being cut off at the bad line.
    assert_eq!(records.len(), 3);
    assert_eq!(records[2]["event_type"], "output_error");
}

#[test]
fn records_are_stored_as_text_not_parsed_values() {
    let mut document = JsonRunDocument::default();
    document.push_ndjson(&two_record_batch()).unwrap();

    assert_eq!(document.records.len(), 2);
    assert!(
        document.records[0].starts_with('{'),
        "{}",
        document.records[0]
    );
}
