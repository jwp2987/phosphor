use std::collections::HashMap;
use std::sync::Arc;

use super::{compute_find_matches, query_find_matches};
use crate::ai::agent::{AIAgentAttachment, AIAgentInput, UserQueryMode};
use crate::terminal::find::FindOptions;

fn user_query_input(query: &str, mode: UserQueryMode) -> AIAgentInput {
    AIAgentInput::UserQuery {
        query: query.to_string(),
        context: Arc::new([]),
        static_query_type: None,
        referenced_attachments: HashMap::new(),
        user_query_mode: mode,
        running_command: None,
        intended_agent: None,
    }
}

// Regression test for https://github.com/warpdotdev/warp/issues/11697.
// Find match highlights on the initial agent query must be computed against the displayed
// query (prefixed with "/agent "), not the stored query, so the ranges line up with the
// rendered text.
#[test]
fn find_matches_align_with_agent_prefixed_initial_query() {
    let query = "list files".to_string();
    let input = user_query_input(&query, UserQueryMode::Normal);

    let displayed = input
        .display_user_query(Some(&query))
        .expect("user query should produce displayed text");
    assert_eq!(displayed, "/agent list files");

    let options = FindOptions::default().with_query(Some("list".to_string()));

    // "list" starts after the "/agent " prefix (7 chars), so the match must be 7..11 — not
    // 0..4, which is what matching against the un-prefixed stored query would produce.
    assert_eq!(compute_find_matches(&displayed, &options), vec![7..11]);
}

// A "/plan" query is stored with its prefix already attached, so its highlights have always
// lined up. Lock that in alongside the agent-prefix fix.
#[test]
fn find_matches_align_with_plan_prefixed_query() {
    let input = user_query_input("write tests", UserQueryMode::Plan);

    let displayed = input
        .display_user_query(None)
        .expect("user query should produce displayed text");
    assert_eq!(displayed, "/plan write tests");

    let options = FindOptions::default().with_query(Some("tests".to_string()));
    assert_eq!(compute_find_matches(&displayed, &options), vec![12..17]);
}

fn opts(q: &str) -> FindOptions {
    FindOptions::default().with_query(Some(q.to_string()))
}

// #938: `run_find` must index into the rendered ("/agent "-prefixed) query. These exercise the
// helper `run_find` uses.
#[test]
fn query_find_matches_start_middle_end_of_agent_query() {
    let query = "long answer here".to_string();
    let inputs = [user_query_input(&query, UserQueryMode::Normal)];
    // Rendered: "/agent long answer here"
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("long")),
        vec![(0, 7..11)]
    );
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("answer")),
        vec![(0, 12..18)]
    );
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("here")),
        vec![(0, 19..23)]
    );
    // The prefix itself is rendered text and is findable.
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("/agent")),
        vec![(0, 0..6)]
    );
}

#[test]
fn query_find_matches_multiline_agent_query() {
    let query = "first line\nsecond line".to_string();
    let inputs = [user_query_input(&query, UserQueryMode::Normal)];
    // "/agent first line\nsecond line": "second" starts at 7 + 11 = 18.
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("second")),
        vec![(0, 18..24)]
    );
}

#[test]
fn query_find_matches_without_prefix_is_unchanged() {
    let query = "long answer".to_string();
    let inputs = [user_query_input(&query, UserQueryMode::Normal)];
    // Not the conversation's initial query (a follow-up): no prefix, offsets are as stored.
    let initial = "something else".to_string();
    assert_eq!(
        query_find_matches(&inputs, Some(&initial), &opts("answer")),
        vec![(0, 5..11)]
    );
    // No initial query known at all.
    assert_eq!(
        query_find_matches(&inputs, None, &opts("answer")),
        vec![(0, 5..11)]
    );
    // Plan queries are stored with their prefix; unchanged.
    let plan = [user_query_input("write tests", UserQueryMode::Plan)];
    assert_eq!(
        query_find_matches(&plan, None, &opts("tests")),
        vec![(0, 12..17)]
    );
}

#[test]
fn query_find_matches_uses_input_index_and_skips_non_query_inputs() {
    let a = "alpha".to_string();
    let inputs = [
        user_query_input(&a, UserQueryMode::Normal),
        user_query_input("alpha again", UserQueryMode::Normal),
    ];
    assert_eq!(
        query_find_matches(&inputs, Some(&a), &opts("alpha")),
        vec![(0, 7..12), (1, 0..5)]
    );
}

fn input_with_refs(query: &str, refs: &[&str]) -> AIAgentInput {
    AIAgentInput::UserQuery {
        query: query.to_string(),
        context: Arc::new([]),
        static_query_type: None,
        referenced_attachments: refs
            .iter()
            .map(|r| (r.to_string(), AIAgentAttachment::PlainText("x".into())))
            .collect(),
        user_query_mode: UserQueryMode::Normal,
        running_command: None,
        intended_agent: None,
    }
}

// #939: find must match the text the block renders, with @-references stripped.
#[test]
fn query_find_matches_after_at_reference_start_middle_end() {
    // Rendered: "look here" / "see look now" / "see look"
    let cases = [
        ("@a.txt look here", "look", 0..4),
        ("see @a.txt look now", "look", 4..8),
        ("see look @a.txt", "look", 4..8),
    ];
    for (query, needle, range) in cases {
        let inputs = [input_with_refs(query, &["@a.txt"])];
        assert_eq!(
            query_find_matches(&inputs, None, &opts(needle)),
            vec![(0, range)],
            "{query}"
        );
    }
}

#[test]
fn query_find_matches_agent_prefix_plus_reference_and_multiple_references() {
    let query = "see @a.txt then @b.rs target".to_string();
    let inputs = [input_with_refs(&query, &["@a.txt", "@b.rs"])];
    // Rendered: "/agent see then target"
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("target")),
        vec![(0, 16..22)]
    );
    assert_eq!(
        query_find_matches(&inputs, Some(&query), &opts("then")),
        vec![(0, 11..15)]
    );
}

#[test]
fn query_find_matches_reference_not_in_query_is_unchanged() {
    // An attachment whose label is absent from the text is not a context reference.
    let inputs = [input_with_refs("plain target", &["@missing"])];
    assert_eq!(
        query_find_matches(&inputs, None, &opts("target")),
        vec![(0, 6..12)]
    );
}
