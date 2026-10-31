//! Unit tests for the `ai_queries` persistence layer in [`super`].
//!
//! Covers the FIFO eviction cap added to [`super::upsert_ai_query`] and the empty-input filter
//! that drives the persistence skip in `handle_ai_history_event`.

use std::sync::Arc;

use diesel::sqlite::SqliteConnection;
use diesel::{Connection, ExpressionMethods, QueryDsl, RunQueryDsl};
use diesel_migrations::MigrationHarness;

use super::{
    process_ai_queries_for_nld_history_match, process_ai_queries_for_uparrow_prompt,
    read_recent_ai_queries,
};
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{AIAgentExchangeId, AIAgentInput, UserQueryMode};
use crate::ai::blocklist::{AIQueryHistoryOutputStatus, PersistedAIInput, PersistedAIInputType};
use crate::ai::llms::LLMId;

/// Builds an in-memory SQLite database with all migrations applied.
fn test_connection() -> SqliteConnection {
    let mut conn =
        SqliteConnection::establish(":memory:").expect("in-memory sqlite connection should open");
    conn.run_pending_migrations(::persistence::MIGRATIONS)
        .expect("migrations should run");
    conn
}

/// Builds a query-bearing [`PersistedAIInput`] with a fresh, unique `exchange_id`.
fn make_query(text: &str) -> Arc<PersistedAIInput> {
    Arc::new(PersistedAIInput {
        exchange_id: AIAgentExchangeId::new(),
        conversation_id: AIConversationId::new(),
        start_ts: chrono::Local::now(),
        inputs: vec![PersistedAIInputType::Query {
            text: text.to_string(),
            context: Default::default(),
            referenced_attachments: Default::default(),
        }],
        output_status: AIQueryHistoryOutputStatus::Completed,
        working_directory: None,
        model_id: LLMId::from("test-model"),
        coding_model_id: LLMId::from("test-coding-model"),
    })
}

fn ai_query_count(conn: &mut SqliteConnection) -> i64 {
    use crate::persistence::schema::ai_queries::dsl::ai_queries;
    ai_queries
        .count()
        .first(conn)
        .expect("count query should succeed")
}

/// Returns the persisted `exchange_id`s ordered by `id` ascending (i.e. insertion / FIFO order).
fn remaining_exchange_ids(conn: &mut SqliteConnection) -> Vec<String> {
    use crate::persistence::schema::ai_queries::dsl::{ai_queries, exchange_id, id};
    ai_queries
        .select(exchange_id)
        .order(id.asc())
        .load::<String>(conn)
        .expect("load query should succeed")
}

fn input_json_for_exchange(conn: &mut SqliteConnection, exchange: &str) -> String {
    use crate::persistence::schema::ai_queries::dsl::{ai_queries, exchange_id, input};
    ai_queries
        .filter(exchange_id.eq(exchange))
        .select(input)
        .first::<String>(conn)
        .expect("row for exchange should exist")
}

/// Returns the text of the first query input on a [`PersistedAIInput`].
fn first_query_text(query: &PersistedAIInput) -> &str {
    match query.inputs.first().expect("query should have an input") {
        PersistedAIInputType::Query { text, .. } => text,
    }
}

/// Builds a [`PersistedAIInput`] whose inputs serialize to `[]`, mirroring legacy rows
/// written before empty inputs were skipped at write time.
fn make_empty_input_query() -> Arc<PersistedAIInput> {
    Arc::new(PersistedAIInput {
        inputs: vec![],
        ..(*make_query("unused")).clone()
    })
}

/// Re-stamps a query's `start_ts`, so a test can order rows explicitly rather than relying on
/// `Local::now()` resolving differently between consecutive calls.
fn with_start_ts(
    query: Arc<PersistedAIInput>,
    start_ts: chrono::DateTime<chrono::Local>,
) -> Arc<PersistedAIInput> {
    Arc::new(PersistedAIInput {
        start_ts,
        ..(*query).clone()
    })
}

#[test]
fn upsert_ai_query_caps_table_and_evicts_oldest_first() {
    let mut conn = test_connection();
    let limit = 3;

    // Insert five distinct exchanges into a table capped at three.
    let queries: Vec<Arc<PersistedAIInput>> =
        (0..5).map(|i| make_query(&format!("q{i}"))).collect();
    let exchange_ids: Vec<String> = queries.iter().map(|q| q.exchange_id.to_string()).collect();

    for query in &queries {
        super::upsert_ai_query_with_limit(&mut conn, query.clone(), limit)
            .expect("upsert should succeed");
    }

    // The table never exceeds the limit.
    assert_eq!(ai_query_count(&mut conn), limit);

    // The two oldest (q0, q1) are evicted; the three newest remain in insertion order.
    assert_eq!(
        remaining_exchange_ids(&mut conn),
        exchange_ids[2..].to_vec()
    );
}

#[test]
fn upsert_ai_query_stays_below_limit_without_evicting() {
    let mut conn = test_connection();
    let limit = 3;

    // Filling exactly up to the limit should not evict anything.
    let queries: Vec<Arc<PersistedAIInput>> =
        (0..3).map(|i| make_query(&format!("q{i}"))).collect();
    let exchange_ids: Vec<String> = queries.iter().map(|q| q.exchange_id.to_string()).collect();

    for query in &queries {
        super::upsert_ai_query_with_limit(&mut conn, query.clone(), limit)
            .expect("upsert should succeed");
    }

    assert_eq!(ai_query_count(&mut conn), limit);
    assert_eq!(remaining_exchange_ids(&mut conn), exchange_ids);
}

#[test]
fn upsert_ai_query_updates_existing_exchange_without_evicting() {
    let mut conn = test_connection();
    let limit = 2;

    // Fill the table to its limit with two distinct exchanges.
    let first = make_query("first");
    let second = make_query("second");
    super::upsert_ai_query_with_limit(&mut conn, first.clone(), limit)
        .expect("upsert should succeed");
    super::upsert_ai_query_with_limit(&mut conn, second.clone(), limit)
        .expect("upsert should succeed");
    assert_eq!(ai_query_count(&mut conn), limit);

    // Re-upsert the oldest exchange (same `exchange_id`) repeatedly. Because this is an update of
    // an existing exchange rather than a new one, it must update in place and never evict.
    let updated_first = Arc::new(PersistedAIInput {
        inputs: vec![PersistedAIInputType::Query {
            text: "first-updated".to_string(),
            context: Default::default(),
            referenced_attachments: Default::default(),
        }],
        ..(*first).clone()
    });
    for _ in 0..5 {
        super::upsert_ai_query_with_limit(&mut conn, updated_first.clone(), limit)
            .expect("upsert should succeed");
    }

    // Still exactly two rows, and both original exchanges survive (the oldest was not evicted).
    assert_eq!(ai_query_count(&mut conn), limit);
    assert_eq!(
        remaining_exchange_ids(&mut conn),
        vec![
            first.exchange_id.to_string(),
            second.exchange_id.to_string()
        ]
    );

    // The in-place update took effect.
    let input_json = input_json_for_exchange(&mut conn, &first.exchange_id.to_string());
    assert!(
        input_json.contains("first-updated"),
        "existing row should have been updated in place, got: {input_json}"
    );
}

#[test]
fn process_ai_queries_for_uparrow_prompt_keeps_newest_capped_oldest_first() {
    // Build 150 oldest-first queries; only the newest 100 should survive, order preserved.
    let queries: Vec<PersistedAIInput> = (0..150)
        .map(|i| (*make_query(&format!("q{i}"))).clone())
        .collect();

    let kept = process_ai_queries_for_uparrow_prompt(queries);

    assert_eq!(kept.len(), 100);
    // The newest 100 (q50..=q149) survive, still oldest-first.
    assert_eq!(first_query_text(&kept[0]), "q50");
    assert_eq!(first_query_text(&kept[99]), "q149");
}

#[test]
fn process_ai_queries_for_uparrow_prompt_keeps_all_when_under_cap() {
    // Fewer than the cap: everything is kept, order preserved.
    let queries: Vec<PersistedAIInput> = (0..3)
        .map(|i| (*make_query(&format!("q{i}"))).clone())
        .collect();

    let kept = process_ai_queries_for_uparrow_prompt(queries);

    let texts: Vec<&str> = kept.iter().map(first_query_text).collect();
    assert_eq!(texts, vec!["q0", "q1", "q2"]);
}

#[test]
fn empty_input_skip_filters_out_non_query_inputs() {
    // Mirrors the filter in `handle_ai_history_event`: only query-bearing inputs are persisted.
    // An exchange whose inputs are all non-query types collapses to an empty `inputs` vec, which
    // is the exact condition that skips persistence.
    let user_query = AIAgentInput::UserQuery {
        query: "hello".to_string(),
        context: Default::default(),
        static_query_type: None,
        referenced_attachments: Default::default(),
        user_query_mode: UserQueryMode::default(),
        running_command: None,
        intended_agent: None,
    };
    let non_query = AIAgentInput::ResumeConversation {
        context: Default::default(),
    };

    // A query input is persistable; a non-query input is not.
    assert!(PersistedAIInputType::try_from(&user_query).is_ok());
    assert!(PersistedAIInputType::try_from(&non_query).is_err());

    // An exchange carrying only non-query inputs collapses to empty -> skipped.
    let only_non_query = [non_query];
    let persisted: Vec<_> = only_non_query
        .iter()
        .filter_map(|input| PersistedAIInputType::try_from(input).ok())
        .collect();
    assert!(persisted.is_empty());

    // An exchange carrying a query input is persisted.
    let with_query = [user_query];
    let persisted: Vec<_> = with_query
        .iter()
        .filter_map(|input| PersistedAIInputType::try_from(input).ok())
        .collect();
    assert_eq!(persisted.len(), 1);
}

#[test]
fn read_recent_ai_queries_returns_oldest_first() {
    let mut conn = test_connection();

    let t0 = chrono::Local::now();
    let texts = ["q0", "q1", "q2"];
    for (i, text) in texts.iter().enumerate() {
        let query = Arc::new(PersistedAIInput {
            start_ts: t0 + chrono::Duration::seconds(i as i64),
            ..(*make_query(text)).clone()
        });
        super::upsert_ai_query_with_limit(&mut conn, query, 10).expect("upsert should succeed");
    }

    let recent = read_recent_ai_queries(&mut conn).expect("read should succeed");
    let texts: Vec<&str> = recent.iter().map(first_query_text).collect();
    assert_eq!(texts, vec!["q0", "q1", "q2"]);
}

/// Ported from the pin (`42effe840:app/src/persistence/block_list_tests.rs:190`), assertions
/// unchanged. It goes through real SQLite rather than a hand-built vector on purpose: the
/// `[]`-inputs case only arises from a row that was already written, so a test that skipped the
/// round trip would not exercise the thing that produces it.
#[test]
fn process_ai_queries_for_nld_history_match_filters_empty_and_whitespace_inputs_oldest_first() {
    let mut conn = test_connection();

    // Explicit, strictly increasing timestamps keep the `start_ts`-ordered read deterministic.
    let t0 = chrono::Local::now();
    for query in [
        with_start_ts(make_query("older prompt"), t0),
        with_start_ts(make_query("   "), t0 + chrono::Duration::seconds(1)),
        with_start_ts(make_empty_input_query(), t0 + chrono::Duration::seconds(2)),
        with_start_ts(make_query("newer prompt"), t0 + chrono::Duration::seconds(3)),
    ] {
        super::upsert_ai_query_with_limit(&mut conn, query, 10).expect("upsert should succeed");
    }

    let recent_ai_queries = read_recent_ai_queries(&mut conn).expect("read should succeed");
    let prompts = process_ai_queries_for_nld_history_match(&recent_ai_queries);
    let texts: Vec<&str> = prompts.iter().map(|(text, _)| text.as_str()).collect();
    // `[]` and whitespace-only rows are dropped; the rest come back oldest-first.
    assert_eq!(texts, vec!["older prompt", "newer prompt"]);
}

/// Inserts a block row for `pane` that started at `start_ts` (or never, for `None`).
fn insert_block(conn: &mut SqliteConnection, pane: &[u8], start_ts: Option<chrono::NaiveDateTime>) {
    use crate::persistence::schema::blocks;
    diesel::insert_into(blocks::table)
        .values((
            blocks::pane_leaf_uuid.eq(pane.to_vec()),
            blocks::stylized_command.eq(b"ls".to_vec()),
            blocks::stylized_output.eq(b"out".to_vec()),
            blocks::exit_code.eq(0),
            blocks::did_execute.eq(true),
            blocks::honor_ps1.eq(false),
            blocks::is_background.eq(false),
            blocks::block_id.eq(uuid::Uuid::new_v4().to_string()),
            blocks::start_ts.eq(start_ts),
        ))
        .execute(conn)
        .expect("block should insert");
}

fn block_panes(conn: &mut SqliteConnection) -> Vec<(Vec<u8>, Option<chrono::NaiveDateTime>)> {
    use crate::persistence::schema::blocks;
    blocks::table
        .select((blocks::pane_leaf_uuid, blocks::start_ts))
        .order(blocks::id.asc())
        .load(conn)
        .expect("blocks should load")
}

/// #1155: only blocks that are unreferenced by `terminal_panes` AND older than the
/// cutoff are deleted. Foreign keys are switched off below (the migrations leave them on), so
/// a pane row can be inserted without its parent pane node and an orphan block can exist.
#[test]
fn delete_orphaned_blocks_removes_only_old_unreferenced_blocks() {
    use crate::persistence::schema::terminal_panes;
    let mut conn = test_connection();
    diesel::sql_query("PRAGMA foreign_keys = OFF")
        .execute(&mut conn)
        .expect("pragma should run");
    let now = chrono::Utc::now().naive_utc();
    let old = now - chrono::Duration::days(30);
    let cutoff = now - chrono::Duration::days(7);
    let live_pane = vec![1u8; 16];
    let dead_pane = vec![2u8; 16];
    diesel::insert_into(terminal_panes::table)
        .values((
            terminal_panes::id.eq(1),
            terminal_panes::uuid.eq(live_pane.clone()),
        ))
        .execute(&mut conn)
        .expect("pane should insert");

    insert_block(&mut conn, &live_pane, Some(old)); // referenced, old: kept
    insert_block(&mut conn, &dead_pane, Some(old)); // orphan, old: deleted
    insert_block(&mut conn, &dead_pane, Some(old)); // orphan, old: deleted
    insert_block(&mut conn, &dead_pane, Some(now)); // orphan, recent: kept
    insert_block(&mut conn, &dead_pane, None); // orphan, no timestamp: kept

    let deleted = super::delete_orphaned_blocks(&mut conn, cutoff).expect("gc should run");

    assert_eq!(deleted, 2);
    let remaining = block_panes(&mut conn);
    assert_eq!(remaining.len(), 3);
    assert_eq!(remaining[0], (live_pane, Some(old)));
    assert_eq!(remaining[1], (dead_pane.clone(), Some(now)));
    assert_eq!(remaining[2], (dead_pane, None));
}
