//! Unit tests for `should_list_live_conversation`. An "Untitled conversation" used to show up
//! in the sidebar/history the instant a split or new agent tab created one, before the user had
//! sent anything -- because the conversation being the pane's *selected* one bypassed the
//! blocklist-render check entirely, including that check's own "has this got any exchanges"
//! guard. See issue #693.

use crate::ai::agent::conversation::{AIConversation, AIConversationId};
use crate::test_util::ai_agent_tasks::{create_api_task, create_message};

use super::should_list_live_conversation;

fn conversation_with_one_exchange() -> AIConversation {
    let root_task = create_api_task("root_task", vec![create_message("message", "root_task")]);
    AIConversation::new_restored(AIConversationId::new(), vec![root_task], None)
        .expect("restored conversation should build")
}

#[test]
fn a_brand_new_empty_conversation_is_never_listed() {
    let conversation = AIConversation::new(false);
    assert_eq!(
        conversation.exchange_count(),
        0,
        "precondition: a freshly created conversation has no exchanges yet"
    );

    assert!(
        !should_list_live_conversation(&conversation, true),
        "must not be listed even when it is the pane's selected conversation"
    );
    assert!(!should_list_live_conversation(&conversation, false));
}

#[test]
fn a_conversation_with_an_exchange_is_listed_when_selected() {
    let conversation = conversation_with_one_exchange();
    assert!(
        conversation.exchange_count() > 0,
        "precondition: the conversation has sent its first message"
    );

    assert!(should_list_live_conversation(&conversation, true));
}

#[test]
fn a_conversation_with_a_visible_exchange_is_listed_even_when_not_selected() {
    let conversation = conversation_with_one_exchange();

    assert!(should_list_live_conversation(&conversation, false));
}
