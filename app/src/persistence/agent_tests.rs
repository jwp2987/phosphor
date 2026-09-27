use chrono::NaiveDate;
use diesel_migrations::MigrationHarness;

use super::*;

fn data_with_parent(parent: Option<&str>) -> String {
    match parent {
        Some(p) => {
            format!(r#"{{"server_conversation_token":null,"parent_conversation_id":"{p}"}}"#)
        }
        None => r#"{"server_conversation_token":null}"#.to_string(),
    }
}

fn ts(secs_from_epoch: i64) -> NaiveDateTime {
    // 2026-01-01 baseline keeps failure messages readable.
    NaiveDate::from_ymd_opt(2026, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        + chrono::Duration::seconds(secs_from_epoch)
}

fn make_row(
    id: i32,
    conversation_id: &str,
    parent: Option<&str>,
    secs: i64,
) -> AgentConversationRecord {
    AgentConversationRecord {
        id,
        conversation_id: conversation_id.to_string(),
        conversation_data: data_with_parent(parent),
        last_modified_at: ts(secs),
        summary: None,
    }
}

/// Row count ≤ limit ⇒ no eviction, regardless of tree shape.
#[test]
fn prune_is_no_op_when_under_limit() {
    let rows = vec![
        make_row(1, "a", None, 100),
        make_row(2, "b", None, 200),
        make_row(3, "c", None, 300),
    ];
    assert!(select_conversations_to_evict(&rows, 3).is_empty());
    assert!(select_conversations_to_evict(&rows, 100).is_empty());
}

/// A tree's effective timestamp is the max of its members; older standalone
/// rows get evicted instead of splitting a fresh tree.
#[test]
fn keeps_fresh_tree_atomically_and_evicts_older_singletons() {
    let mut rows = vec![
        make_row(1, "root", None, 100), // parent, older
        make_row(2, "c1", Some("root"), 500),
        make_row(3, "c2", Some("root"), 1000),
        make_row(4, "c3", Some("root"), 1500),
        make_row(5, "c4", Some("root"), 2000),
    ];
    for i in 0_i32..9 {
        let id = format!("s{i}");
        rows.push(make_row(100 + i, &id, None, 10 + i64::from(i)));
    }
    let evicted = select_conversations_to_evict(&rows, 13);
    assert_eq!(evicted.len(), 1, "evicted={evicted:?}");
    assert_eq!(evicted[0], "s0", "must evict the oldest singleton");
    for tree_member in ["root", "c1", "c2", "c3", "c4"] {
        assert!(
            !evicted.contains(&tree_member.to_string()),
            "tree member {tree_member} was evicted; evicted={evicted:?}"
        );
    }
}

/// A stale parent is kept when its child is fresh: tree ts = max(members).
#[test]
fn child_kept_drags_parent_along() {
    let mut rows = vec![
        make_row(1, "parent", None, 1),              // very old parent
        make_row(2, "child", Some("parent"), 9_999), // very fresh child
    ];
    for i in 0_i32..8 {
        let id = format!("s{i}");
        rows.push(make_row(100 + i, &id, None, 100 + i64::from(i)));
    }
    let evicted = select_conversations_to_evict(&rows, 9);
    assert_eq!(evicted.len(), 1, "evicted={evicted:?}");
    assert!(!evicted.contains(&"parent".to_string()));
    assert!(!evicted.contains(&"child".to_string()));
    assert_eq!(evicted[0], "s0");
}

/// Reverse of the previous case: a stale child is kept when its parent is
/// fresh.
#[test]
fn parent_kept_drags_child_along() {
    let mut rows = vec![
        make_row(1, "parent", None, 9_999),      // very fresh parent
        make_row(2, "child", Some("parent"), 1), // very old child
    ];
    for i in 0_i32..8 {
        let id = format!("s{i}");
        rows.push(make_row(100 + i, &id, None, 100 + i64::from(i)));
    }
    let evicted = select_conversations_to_evict(&rows, 9);
    assert_eq!(evicted.len(), 1, "evicted={evicted:?}");
    assert!(!evicted.contains(&"parent".to_string()));
    assert!(
        !evicted.contains(&"child".to_string()),
        "stale child must not be evicted while its parent is kept; evicted={evicted:?}"
    );
    assert_eq!(evicted[0], "s0");
}

/// Orphans (declared parent missing from row set) are their own root.
#[test]
fn orphan_with_missing_parent_is_its_own_tree() {
    let rows = vec![
        make_row(1, "orphan", Some("missing_parent_id"), 9_999), // fresh
        make_row(2, "a", None, 100),
        make_row(3, "b", None, 200),
        make_row(4, "c", None, 300),
    ];
    let evicted = select_conversations_to_evict(&rows, 3);
    assert_eq!(evicted.len(), 1, "evicted={evicted:?}");
    assert_eq!(evicted[0], "a");
    assert!(!evicted.contains(&"orphan".to_string()));
}

/// The freshest tree is retained even when it alone exceeds the cap, so we
/// never split an active orchestration session.
#[test]
fn single_tree_larger_than_limit_is_kept_in_full() {
    let mut rows = vec![make_row(1, "big_root", None, 10_000)];
    for i in 0_i32..199 {
        let cid = format!("big_child_{i}");
        rows.push(make_row(2 + i, &cid, Some("big_root"), 100 + i64::from(i)));
    }
    rows.push(make_row(9_999, "older_singleton", None, 50));
    let evicted = select_conversations_to_evict(&rows, 50);
    assert_eq!(evicted, vec!["older_singleton".to_string()]);
}

/// A parse-failure row is still a valid parent reference: it just becomes
/// its own root rather than getting quarantined out of the parent index.
#[test]
fn parse_failure_row_is_treated_as_root_and_can_be_referenced_by_others() {
    let mut rows = vec![
        AgentConversationRecord {
            id: 1,
            conversation_id: "garbage".to_string(),
            conversation_data: "{not valid json".to_string(),
            last_modified_at: ts(50),
            summary: None,
        },
        make_row(2, "a", None, 100),
        make_row(3, "b", None, 200),
        make_row(4, "c", None, 300),
    ];
    rows.push(make_row(5, "child_of_garbage", Some("garbage"), 9_999));
    let evicted = select_conversations_to_evict(&rows, 4);
    assert_eq!(evicted, vec!["a".to_string()]);
}

/// Same input twice produces the same output. Tie-broken by root_id ASC.
#[test]
fn eviction_is_deterministic() {
    let rows = vec![
        make_row(1, "a", None, 100),
        make_row(2, "b", None, 100),
        make_row(3, "c", None, 100),
        make_row(4, "d", None, 100),
    ];
    let e1 = select_conversations_to_evict(&rows, 2);
    let e2 = select_conversations_to_evict(&rows, 2);
    assert_eq!(e1, e2);
    assert_eq!(e1, vec!["c".to_string(), "d".to_string()]);
}

// ---------------------------------------------------------------------------
// #431: read_agent_conversation_metadata / backfill_conversation_summaries.
// Ported from the pin (`app/src/persistence/agent_tests.rs:1-227`, `02b53fcd8`).
// ---------------------------------------------------------------------------

/// Builds an in-memory SQLite database with all migrations applied.
fn test_connection() -> SqliteConnection {
    let mut conn =
        SqliteConnection::establish(":memory:").expect("in-memory sqlite connection should open");
    conn.run_pending_migrations(::persistence::MIGRATIONS)
        .expect("migrations should run");
    conn
}

fn task_with_user_query(task_id: &str, query: &str, description: &str) -> api::Task {
    api::Task {
        id: task_id.to_string(),
        description: description.to_string(),
        dependencies: None,
        messages: vec![api::Message {
            id: format!("{task_id}-user-query"),
            task_id: task_id.to_string(),
            message: Some(api::message::Message::UserQuery(api::message::UserQuery {
                query: query.to_string(),
                ..Default::default()
            })),
            ..Default::default()
        }],
        summary: String::new(),
        server_data: String::new(),
    }
}

fn empty_conversation_data() -> AgentConversationData {
    serde_json::from_str(r#"{"server_conversation_token":null}"#)
        .expect("minimal conversation data should deserialize")
}

fn summary_column(conn: &mut SqliteConnection, conversation: &str) -> Option<String> {
    use schema::agent_conversations::dsl::*;
    agent_conversations
        .filter(conversation_id.eq(conversation))
        .select(summary)
        .first::<Option<String>>(conn)
        .expect("conversation row should exist")
}

fn last_modified_column(conn: &mut SqliteConnection, conversation: &str) -> NaiveDateTime {
    use schema::agent_conversations::dsl::*;
    agent_conversations
        .filter(conversation_id.eq(conversation))
        .select(last_modified_at)
        .first::<NaiveDateTime>(conn)
        .expect("conversation row should exist")
}

#[test]
fn upsert_writes_summary_and_metadata_read_skips_tasks() {
    let mut conn = test_connection();
    let task = task_with_user_query("task-1", "Initial query", "Root title");
    upsert_agent_conversation(&mut conn, "conv-1", [&task], empty_conversation_data())
        .expect("upsert should succeed");

    let (conversations, backfills) =
        read_agent_conversation_metadata(&mut conn).expect("metadata read should succeed");

    assert!(
        backfills.is_empty(),
        "rows written with a summary must not be backfilled"
    );
    assert_eq!(conversations.len(), 1);
    assert!(
        conversations[0].tasks.is_empty(),
        "metadata read must not hydrate task payloads"
    );
    let summary: AgentConversationSummary = serde_json::from_str(
        conversations[0]
            .conversation
            .summary
            .as_deref()
            .expect("summary column should be written at upsert time"),
    )
    .expect("summary column should hold valid summary JSON");
    assert_eq!(summary.initial_query, "Initial query");
    assert_eq!(summary.title, "Root title");
    assert!(summary.is_restorable);
}

#[test]
fn metadata_read_derives_and_backfill_persists_summary_for_legacy_rows() {
    use schema::agent_conversations::dsl::*;

    let mut conn = test_connection();
    let task = task_with_user_query("task-1", "Initial query", "Root title");
    upsert_agent_conversation(&mut conn, "conv-1", [&task], empty_conversation_data())
        .expect("upsert should succeed");

    // Simulate a row written before the summary column existed. Setting
    // `last_modified_at` explicitly keeps the update trigger from bumping it.
    let legacy_ts = ts(1_000);
    diesel::update(agent_conversations.filter(conversation_id.eq("conv-1")))
        .set((summary.eq(None::<String>), last_modified_at.eq(legacy_ts)))
        .execute(&mut conn)
        .expect("legacy row setup should succeed");

    let (conversations, backfills) =
        read_agent_conversation_metadata(&mut conn).expect("metadata read should succeed");

    // The read derives the summary from the row's own task snapshot.
    assert_eq!(conversations.len(), 1);
    let derived: AgentConversationSummary = serde_json::from_str(
        conversations[0]
            .conversation
            .summary
            .as_deref()
            .expect("legacy rows should get a read-time-derived summary"),
    )
    .expect("derived summary should be valid JSON");
    assert_eq!(derived.initial_query, "Initial query");

    // ... and queues a backfill preserving the original timestamp.
    assert_eq!(backfills.len(), 1);
    assert_eq!(backfills[0].conversation_id, "conv-1");
    assert_eq!(backfills[0].last_modified_at, legacy_ts);

    backfill_conversation_summaries(&mut conn, backfills).expect("backfill should succeed");

    assert!(
        summary_column(&mut conn, "conv-1").is_some(),
        "backfill must persist the derived summary"
    );
    assert_eq!(
        last_modified_column(&mut conn, "conv-1"),
        legacy_ts,
        "backfill must not reorder history by bumping last_modified_at"
    );

    // Subsequent startups stay metadata-only.
    let (_, backfills) =
        read_agent_conversation_metadata(&mut conn).expect("metadata read should succeed");
    assert!(backfills.is_empty());
}

#[test]
fn backfill_never_overwrites_a_newer_summary() {
    let mut conn = test_connection();
    let task = task_with_user_query("task-1", "Initial query", "Root title");
    upsert_agent_conversation(&mut conn, "conv-1", [&task], empty_conversation_data())
        .expect("upsert should succeed");
    let written_summary = summary_column(&mut conn, "conv-1");
    let written_ts = last_modified_column(&mut conn, "conv-1");

    // Stale backfills (computed before a newer write landed) must not
    // clobber the row's summary or timestamp, regardless of whether the
    // reader observed a NULL or a since-replaced invalid value.
    let stale_from_null = ConversationSummaryBackfill {
        conversation_id: "conv-1".to_string(),
        summary_json: "{\"stale\":true}".to_string(),
        previous_summary: None,
        last_modified_at: ts(1),
    };
    let stale_from_invalid = ConversationSummaryBackfill {
        conversation_id: "conv-1".to_string(),
        summary_json: "{\"stale\":true}".to_string(),
        previous_summary: Some("{not valid json".to_string()),
        last_modified_at: ts(1),
    };
    backfill_conversation_summaries(&mut conn, vec![stale_from_null, stale_from_invalid])
        .expect("backfill should succeed");

    assert_eq!(summary_column(&mut conn, "conv-1"), written_summary);
    assert_eq!(last_modified_column(&mut conn, "conv-1"), written_ts);
}

#[test]
fn metadata_read_heals_invalid_non_null_summaries() {
    use schema::agent_conversations::dsl::*;

    let mut conn = test_connection();
    let task = task_with_user_query("task-1", "Initial query", "Root title");
    upsert_agent_conversation(&mut conn, "conv-1", [&task], empty_conversation_data())
        .expect("upsert should succeed");

    // Corrupt the summary with unparseable JSON.
    let legacy_ts = ts(1_000);
    diesel::update(agent_conversations.filter(conversation_id.eq("conv-1")))
        .set((
            summary.eq(Some("{not valid json")),
            last_modified_at.eq(legacy_ts),
        ))
        .execute(&mut conn)
        .expect("corrupt summary setup should succeed");

    let (_, backfills) =
        read_agent_conversation_metadata(&mut conn).expect("metadata read should succeed");
    assert_eq!(backfills.len(), 1);
    assert_eq!(
        backfills[0].previous_summary.as_deref(),
        Some("{not valid json"),
        "the backfill must carry the observed invalid value for its compare-and-set"
    );

    backfill_conversation_summaries(&mut conn, backfills).expect("backfill should succeed");

    // The invalid summary healed in place and history order is preserved.
    let healed: AgentConversationSummary = serde_json::from_str(
        summary_column(&mut conn, "conv-1")
            .as_deref()
            .expect("summary should be present after healing"),
    )
    .expect("healed summary should be valid JSON");
    assert_eq!(healed.initial_query, "Initial query");
    assert_eq!(last_modified_column(&mut conn, "conv-1"), legacy_ts);

    // Subsequent startups stay metadata-only.
    let (_, backfills) =
        read_agent_conversation_metadata(&mut conn).expect("metadata read should succeed");
    assert!(backfills.is_empty());
}

// ---------------------------------------------------------------------------
// Task-row retention: a save must never shrink persisted history by accident.

fn task_ids_column(conn: &mut SqliteConnection, conversation: &str) -> Vec<String> {
    use schema::agent_tasks::dsl::*;
    let mut ids: Vec<String> = agent_tasks
        .filter(conversation_id.eq(conversation))
        .select(task_id)
        .load::<String>(conn)
        .expect("agent_tasks should be readable");
    ids.sort();
    ids
}

fn stored_initial_query(conn: &mut SqliteConnection, conversation: &str) -> String {
    let summary_json =
        summary_column(conn, conversation).expect("summary column should be written");
    serde_json::from_str::<AgentConversationSummary>(&summary_json)
        .expect("summary column should hold valid summary JSON")
        .initial_query
}

/// An empty task snapshot, typed for `upsert_agent_conversation*`.
fn no_tasks() -> Vec<&'static api::Task> {
    Vec::new()
}

fn subtask_of(parent_id: &str, task_id: &str) -> api::Task {
    let mut task = task_with_user_query(task_id, "sub query", "Subtask");
    task.dependencies = Some(api::task::Dependencies {
        parent_task_id: parent_id.to_string(),
    });
    task
}

/// The reported defect: a conversation whose in-memory tasks carry no source (a hollow,
/// synthesized root) saves an EMPTY snapshot, and `ne_all(&[])` deleted every task row and
/// rewrote the summary with an empty `initial_query`, dropping the conversation from the
/// history list. An ordinary empty save must now leave both alone.
#[test]
fn empty_snapshot_keeps_task_rows_and_summary() {
    let mut conn = test_connection();
    let root = task_with_user_query("root", "Initial query", "Root title");
    upsert_agent_conversation(&mut conn, "conv-1", [&root], empty_conversation_data())
        .expect("first upsert should succeed");

    upsert_agent_conversation(&mut conn, "conv-1", no_tasks(), empty_conversation_data())
        .expect("empty upsert should succeed");

    assert_eq!(
        task_ids_column(&mut conn, "conv-1"),
        vec!["root".to_string()],
        "an empty snapshot must not delete the conversation's task rows",
    );
    assert_eq!(
        stored_initial_query(&mut conn, "conv-1"),
        "Initial query",
        "an empty snapshot must not replace a summary that names a real initial query",
    );
}

/// Replace semantics are still what keeps a removed subtask (e.g. one pruned by a rewind)
/// from being resurrected on restore: a non-empty snapshot that drops a task deletes its row.
#[test]
fn non_empty_snapshot_still_deletes_a_removed_task() {
    let mut conn = test_connection();
    let root = task_with_user_query("root", "Initial query", "Root title");
    let sub = subtask_of("root", "sub");
    upsert_agent_conversation(
        &mut conn,
        "conv-1",
        [&root, &sub],
        empty_conversation_data(),
    )
    .expect("first upsert should succeed");
    assert_eq!(
        task_ids_column(&mut conn, "conv-1"),
        vec!["root".to_string(), "sub".to_string()],
    );

    upsert_agent_conversation(&mut conn, "conv-1", [&root], empty_conversation_data())
        .expect("second upsert should succeed");

    assert_eq!(
        task_ids_column(&mut conn, "conv-1"),
        vec!["root".to_string()],
        "a task removed from a non-empty snapshot must be deleted",
    );
    assert_eq!(stored_initial_query(&mut conn, "conv-1"), "Initial query");
}

/// A rewind past the first exchange is the one deliberate empty snapshot: it asks for
/// `DeleteMissingEvenIfEmpty`, and every row goes, or the rewound exchanges come back.
#[test]
fn explicit_empty_snapshot_clears_task_rows() {
    let mut conn = test_connection();
    let root = task_with_user_query("root", "Initial query", "Root title");
    let sub = subtask_of("root", "sub");
    upsert_agent_conversation(
        &mut conn,
        "conv-1",
        [&root, &sub],
        empty_conversation_data(),
    )
    .expect("first upsert should succeed");

    upsert_agent_conversation_with_retention(
        &mut conn,
        "conv-1",
        no_tasks(),
        empty_conversation_data(),
        PersistedTaskRetention::DeleteMissingEvenIfEmpty,
    )
    .expect("explicit empty upsert should succeed");

    assert!(task_ids_column(&mut conn, "conv-1").is_empty());
    assert_eq!(
        stored_initial_query(&mut conn, "conv-1"),
        "",
        "a deliberately emptied conversation's summary follows its (empty) task set",
    );
}

/// A conversation restored with a synthesized root saves with `KeepMissing`: after a
/// follow-up its snapshot holds only a NEW root, which must be added next to the persisted
/// rows rather than replace them, and the stored summary must keep the original query.
#[test]
fn keep_missing_adds_rows_without_deleting_or_relabelling() {
    let mut conn = test_connection();
    let root = task_with_user_query("root", "Initial query", "Root title");
    let sub = subtask_of("root", "sub");
    upsert_agent_conversation(
        &mut conn,
        "conv-1",
        [&root, &sub],
        empty_conversation_data(),
    )
    .expect("first upsert should succeed");

    let fresh_root = task_with_user_query("fresh-root", "Follow-up", "Follow-up title");
    upsert_agent_conversation_with_retention(
        &mut conn,
        "conv-1",
        [&fresh_root],
        empty_conversation_data(),
        PersistedTaskRetention::KeepMissing,
    )
    .expect("keep-missing upsert should succeed");

    assert_eq!(
        task_ids_column(&mut conn, "conv-1"),
        vec![
            "fresh-root".to_string(),
            "root".to_string(),
            "sub".to_string()
        ],
    );
    assert_eq!(stored_initial_query(&mut conn, "conv-1"), "Initial query");
}

/// `KeepMissing` only protects a summary that describes something: a conversation whose
/// stored summary has no initial query (e.g. a child persisted before its first response)
/// still gets its summary written once real content arrives, or it would never be listed.
#[test]
fn keep_missing_writes_summary_when_none_names_a_query() {
    let mut conn = test_connection();
    upsert_agent_conversation(&mut conn, "conv-1", no_tasks(), empty_conversation_data())
        .expect("empty upsert should succeed");
    assert_eq!(stored_initial_query(&mut conn, "conv-1"), "");

    let root = task_with_user_query("root", "First real query", "Root title");
    upsert_agent_conversation_with_retention(
        &mut conn,
        "conv-1",
        [&root],
        empty_conversation_data(),
        PersistedTaskRetention::KeepMissing,
    )
    .expect("keep-missing upsert should succeed");

    assert_eq!(
        stored_initial_query(&mut conn, "conv-1"),
        "First real query"
    );
}

/// Explicit deletion is a separate path and must still remove everything, whatever
/// retention earlier saves used.
#[test]
fn explicit_conversation_deletion_still_removes_task_rows() {
    let mut conn = test_connection();
    let root = task_with_user_query("root", "Initial query", "Root title");
    upsert_agent_conversation(&mut conn, "conv-1", [&root], empty_conversation_data())
        .expect("first upsert should succeed");
    upsert_agent_conversation(&mut conn, "conv-1", no_tasks(), empty_conversation_data())
        .expect("empty upsert should succeed");

    delete_agent_conversations(&mut conn, vec!["conv-1".to_string()])
        .expect("delete should succeed");

    assert!(task_ids_column(&mut conn, "conv-1").is_empty());
    use schema::agent_conversations::dsl::*;
    let remaining: i64 = agent_conversations
        .filter(conversation_id.eq("conv-1"))
        .count()
        .get_result(&mut conn)
        .expect("count should succeed");
    assert_eq!(remaining, 0);
}

/// A conversation with an undecodable task row is not handed out at all, rather than as a
/// conversation silently missing that task (whose next save would prune the row).
#[test]
fn read_by_id_refuses_a_conversation_with_an_undecodable_task() {
    let mut conn = test_connection();
    let root = task_with_user_query("root", "Initial query", "Root title");
    let sub = subtask_of("root", "sub");
    upsert_agent_conversation(
        &mut conn,
        "conv-1",
        [&root, &sub],
        empty_conversation_data(),
    )
    .expect("upsert should succeed");
    {
        use schema::agent_tasks::dsl::*;
        diesel::update(agent_tasks.filter(task_id.eq("sub")))
            .set(task.eq(vec![0xffu8, 0xff, 0xff]))
            .execute(&mut conn)
            .expect("corrupting the row should succeed");
    }

    assert!(
        read_agent_conversation_by_id(&mut conn, "conv-1").is_err(),
        "a partially undecodable conversation must not be returned as if complete",
    );
    assert_eq!(
        task_ids_column(&mut conn, "conv-1"),
        vec!["root".to_string(), "sub".to_string()],
        "refusing the read must leave every row in place",
    );
}
