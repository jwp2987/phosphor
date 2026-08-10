use chrono::{DateTime, Duration, Utc};

use super::*;

/// Ported from the pin's `agent_conversations_model_tests.rs` (`02b53fcd8`,
/// `conversation_query_*`). The pin builds its fixtures by inserting `AmbientAgentTask`s into
/// the model and calling `get_entries` to normalize them; the fork's `AgentConversationsModel`
/// never populates `tasks` locally (see `agent_conversations_model.rs`'s `get_entries` doc
/// comment) so that path doesn't exist here. `query_conversation_entries` itself was ported
/// verbatim (see its doc comment) and is a pure function over `Vec<AgentConversationEntry>`, so
/// these tests build `AgentConversationEntry` fixtures directly instead of going through a task
/// model -- the ranking/truncation/tie-break assertions are unchanged from the pin.
fn make_entry(title: &str, last_updated: DateTime<Utc>) -> AgentConversationEntry {
    let id = AIConversationId::new();
    AgentConversationEntry {
        id: AgentConversationEntryId::Conversation(id),
        identity: AgentConversationIdentity {
            local_conversation_id: Some(id),
            server_conversation_token: None,
        },
        display: AgentConversationDisplayData {
            title: title.to_string(),
            last_updated,
            status: AgentRunDisplayStatus::ConversationSucceeded,
            harness: None,
        },
    }
}

#[test]
fn conversation_query_caps_recent_entries_and_places_newest_last() {
    let now = Utc::now();
    let entries: Vec<AgentConversationEntry> = (0..55)
        .map(|index| {
            make_entry(
                &format!("Conversation {index}"),
                now - Duration::seconds(index as i64),
            )
        })
        .collect();

    let results = query_conversation_entries(entries, "");

    assert_eq!(results.len(), DEFAULT_RESULT_COUNT);
    assert_eq!(
        results
            .first()
            .map(|result| result.entry.display.title.as_str()),
        Some("Conversation 49")
    );
    assert_eq!(
        results
            .last()
            .map(|result| result.entry.display.title.as_str()),
        Some("Conversation 0")
    );
    assert!(
        !results
            .iter()
            .any(|result| result.entry.display.title == "Conversation 50")
    );
}

#[test]
fn conversation_query_filters_titles_and_caps_best_fuzzy_results() {
    let now = Utc::now();
    let entries: Vec<AgentConversationEntry> = (0..=MAX_SEARCH_RESULTS + 2)
        .map(|index| {
            let title = if index == 1 {
                "Fix unit tests".to_owned()
            } else {
                format!("Deploy service {index}")
            };
            make_entry(&title, now - Duration::seconds(index as i64))
        })
        .collect();

    let results = query_conversation_entries(entries, "deploy");

    assert_eq!(results.len(), MAX_SEARCH_RESULTS);
    assert!(
        results
            .iter()
            .all(|result| result.entry.display.title.contains("Deploy"))
    );
    assert!(results.windows(2).all(|window| {
        window[0].title_match.as_ref().unwrap().score
            <= window[1].title_match.as_ref().unwrap().score
    }));
}

#[test]
fn conversation_query_orders_equal_fuzzy_scores_by_recency() {
    let now = Utc::now();
    let entries: Vec<AgentConversationEntry> = [0, 2, 1]
        .into_iter()
        .map(|index| make_entry("Deploy service", now - Duration::seconds(index as i64)))
        .collect();

    let results = query_conversation_entries(entries, "deploy");

    assert!(results.windows(2).all(|window| {
        window[0].entry.display.last_updated <= window[1].entry.display.last_updated
    }));
}
