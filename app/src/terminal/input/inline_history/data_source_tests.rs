use chrono::{Local, TimeZone as _};

use crate::input_suggestions::HistoryOrder;

use super::{interleave_conversations, MenuEntry, MenuItem};

#[test]
fn interleave_conversations_only_inserts_into_current_session_segment() {
    let t10 = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 10).unwrap();
    let t20 = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 20).unwrap();
    let t50 = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, 50).unwrap();
    let t100 = Local.with_ymd_and_hms(2024, 1, 1, 0, 1, 40).unwrap();
    let t150 = Local.with_ymd_and_hms(2024, 1, 1, 0, 2, 30).unwrap();
    let t200 = Local.with_ymd_and_hms(2024, 1, 1, 0, 3, 20).unwrap();

    let base = vec![
        MenuEntry {
            order: HistoryOrder::DifferentSession,
            sort_timestamp: t10,
            item: MenuItem::Command {
                command: "d10".to_string(),
                display_timestamp: t10,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::DifferentSession,
            sort_timestamp: t20,
            item: MenuItem::Command {
                command: "d20".to_string(),
                display_timestamp: t20,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t100,
            item: MenuItem::Command {
                command: "c100".to_string(),
                display_timestamp: t100,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t200,
            item: MenuItem::Command {
                command: "c200".to_string(),
                display_timestamp: t200,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
    ];

    let conversations = vec![
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t150,
            item: MenuItem::Command {
                command: "conv150".to_string(),
                display_timestamp: t150,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
        MenuEntry {
            order: HistoryOrder::CurrentSession,
            sort_timestamp: t50,
            item: MenuItem::Command {
                command: "conv50".to_string(),
                display_timestamp: t50,
                linked_workflow_data: None,
                prefix_match_len: 0,
            },
        },
    ];

    let merged = interleave_conversations(base, conversations);

    let commands = merged
        .iter()
        .map(|e| match &e.item {
            MenuItem::Command { command, .. } => command.as_str(),
            MenuItem::Conversation { title, .. } => title.as_str(),
        })
        .collect::<Vec<_>>();

    // DifferentSession entries stay at the top; conversations are inserted into the CurrentSession segment.
    assert_eq!(
        commands,
        vec!["d10", "d20", "conv50", "c100", "conv150", "c200"]
    );
}

fn command_entry(order: HistoryOrder, name: &str, secs: u32) -> MenuEntry {
    let ts = Local.with_ymd_and_hms(2024, 1, 1, 0, 0, secs).unwrap();
    MenuEntry {
        order,
        sort_timestamp: ts,
        item: MenuItem::Command {
            command: name.to_string(),
            display_timestamp: ts,
            linked_workflow_data: None,
            prefix_match_len: 0,
        },
    }
}

fn names(entries: &[MenuEntry]) -> Vec<&str> {
    entries
        .iter()
        .map(|e| match &e.item {
            MenuItem::Command { command, .. } => command.as_str(),
            MenuItem::Conversation { title, .. } => title.as_str(),
        })
        .collect()
}

#[test]
fn interleave_conversations_edge_orderings() {
    use HistoryOrder::{CurrentSession as Cur, DifferentSession as Diff};

    // Conversations older than everything land first in the current segment, newer ones last,
    // and a conversation with an equal timestamp goes before the base entry (strict `<`).
    let base = vec![
        command_entry(Diff, "d1", 1),
        command_entry(Cur, "c10", 10),
        command_entry(Cur, "c20", 20),
    ];
    let conversations = vec![
        command_entry(Cur, "new", 30),
        command_entry(Cur, "tie", 20),
        command_entry(Cur, "old", 5),
    ];
    let merged = interleave_conversations(base, conversations);
    assert_eq!(
        names(&merged),
        vec!["d1", "old", "c10", "tie", "c20", "new"]
    );

    // No current-session entries: every conversation is appended after the base.
    let base = vec![command_entry(Diff, "d1", 1), command_entry(Diff, "d2", 2)];
    let merged = interleave_conversations(base, vec![command_entry(Cur, "conv", 0)]);
    assert_eq!(names(&merged), vec!["d1", "d2", "conv"]);

    // No conversations: base is unchanged.
    let base = vec![command_entry(Diff, "d1", 1), command_entry(Cur, "c2", 2)];
    let merged = interleave_conversations(base, vec![]);
    assert_eq!(names(&merged), vec!["d1", "c2"]);
}
