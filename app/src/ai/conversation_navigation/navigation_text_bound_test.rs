//! Navigation data must not retain the full first input of a conversation (#947).

use super::{
    ConversationNavigationData, MAX_NAVIGATION_TEXT_CHARS, bounded_navigation_text,
    bounded_navigation_text_ref,
};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::history_model::AIConversationMetadata;

fn metadata(title: String, initial_query: String) -> AIConversationMetadata {
    AIConversationMetadata {
        id: AIConversationId::new(),
        title,
        initial_query,
        last_modified_at: chrono::Utc::now().naive_utc(),
        initial_working_directory: None,
        credits_spent: None,
        server_conversation_token: None,
        has_local_data: false,
        artifacts: Vec::new(),
        ambient_agent_task_id: None,
        parent_conversation_id: None,
        parent_agent_id: None,
    }
}

#[test]
fn three_mb_input_is_capped_for_historical_metadata() {
    let big = "word ".repeat(3 * 1024 * 1024 / 5);
    let meta = metadata(big.clone(), big);
    let nav = ConversationNavigationData::from_historical_conversation_metadata(&meta);
    assert!(nav.title.chars().count() <= MAX_NAVIGATION_TEXT_CHARS);
    let q = nav.initial_query.expect("initial query present");
    assert!(q.chars().count() <= MAX_NAVIGATION_TEXT_CHARS);
    assert!(q.starts_with("word word"));
    // The persisted metadata is untouched.
    assert!(meta.initial_query.len() > 3_000_000);
}

#[test]
fn multibyte_and_emoji_cut_on_a_char_boundary() {
    for unit in ["é", "日本", "😀", "👨\u{200d}👩\u{200d}👧"] {
        let s = unit.repeat(MAX_NAVIGATION_TEXT_CHARS);
        let out = bounded_navigation_text(s.clone());
        assert!(out.chars().count() <= MAX_NAVIGATION_TEXT_CHARS);
        assert!(s.starts_with(&out));
        assert!(!out.ends_with('\u{200d}'));
        assert_eq!(out, bounded_navigation_text_ref(&s));
    }
}

#[test]
fn short_and_empty_text_is_unchanged() {
    assert_eq!(bounded_navigation_text(String::new()), "");
    assert_eq!(bounded_navigation_text_ref(""), "");
    let exact = "a".repeat(MAX_NAVIGATION_TEXT_CHARS);
    assert_eq!(bounded_navigation_text(exact.clone()), exact);
    let nav = ConversationNavigationData::from_historical_conversation_metadata(&metadata(
        "t".into(),
        String::new(),
    ));
    assert_eq!(nav.initial_query.as_deref(), Some(""));
}

#[test]
fn fuzzy_search_still_finds_a_word_in_the_first_100_chars() {
    let big = format!(
        "please refactor the zebrafish module {}",
        "x ".repeat(2_000_000)
    );
    let nav = ConversationNavigationData::from_historical_conversation_metadata(&metadata(
        "Title".into(),
        big,
    ));
    let q = nav.initial_query.as_deref().expect("initial query present");
    assert!(fuzzy_match::match_indices_case_insensitive(q, "zebrafish").is_some());
}
