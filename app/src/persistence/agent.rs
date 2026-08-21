use chrono::NaiveDateTime;
use diesel::associations::HasTable;
use diesel::{prelude::*, result::Error, SqliteConnection};
use prost::Message;
use std::collections::{HashMap, HashSet};
use warp_multi_agent_api as api;

use super::ConversationSummaryBackfill;
use super::model::{AgentConversation, AgentConversationData, AgentConversationSummary};
use crate::persistence::model::{AgentConversationRecord, AgentTaskRecord};
use crate::persistence::schema::{self, agent_conversations, agent_tasks};

/// Maximum size of a single serialized `api::Task` protobuf BLOB this code will **write** to
/// `agent_tasks.task`.
///
/// This is a write-side cap only, and the previous version of this comment said otherwise —
/// it claimed tasks over the limit were "skipped on both write and read to prevent startup
/// OOM when all task records are loaded at once". Neither half held. The constant has exactly
/// one use, the `continue` in [`upsert_agent_conversation`]; both readers decode every blob
/// they load unconditionally and never consult it (`:294`, `:402`). And the read that the
/// sentence describes no longer exists: since #431, startup goes through
/// [`read_agent_conversation_metadata`], which reads the `summary` column and touches
/// `agent_tasks` only for rows written before that column existed. So the protection was
/// claimed for precisely the rows it does not cover — the oversized ones already on disk.
///
/// What actually happens when a task exceeds the cap: its blob is not written, and its row is
/// deliberately *not* deleted either (see `kept_task_ids` below), so the database keeps the
/// last version of that task that fit. Restore then hydrates a stale copy. If no earlier
/// version was ever written, the task is simply absent: `AIConversation::new_restored` drops a
/// task whose parent is missing with a `log::error!`, and a conversation missing its root
/// fails with `RestoreConversationError::NoRootTask`. The `summary` column is still derived
/// from the full in-memory snapshot including the skipped task, so such a conversation is
/// listed in history as restorable and fails when opened.
///
/// **Read-side enforcement is deliberately not added here.** A size check before
/// `api::Task::decode` would skip exactly the blobs written before this constant existed,
/// turning "a conversation that restores, slowly" into "a conversation that has silently
/// vanished from history" — `is_restorable` is what startup filters on, and eviction deletes
/// rows. Making the OOM guarantee real needs a migration first: walk `agent_tasks` once,
/// report oversized rows, and either re-encode them (pruning tool-output payloads, the only
/// thing that reaches this size) or drop them with the user told which conversations were
/// affected. Only after that can a reader refuse an oversized blob without destroying data.
const MAX_TASK_BLOB_BYTES: usize = 10 * 1024 * 1024; // 10 MB

#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = agent_conversations)]
struct NewAgentConversation {
    conversation_id: String,
    conversation_data: String,
    summary: Option<String>,
}

#[derive(Debug, Insertable, AsChangeset)]
#[diesel(table_name = agent_tasks)]
struct NewAgentTask {
    conversation_id: String,
    task_id: String,
    task: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum UpsertConversationError {
    #[error("Failed to serialize conversation data: {0:?}")]
    Serialization(#[from] serde_json::Error),
    #[error("Failed to upsert conversation to sqlite: {0:?}")]
    DB(#[from] diesel::result::Error),
}

/// Maximum number of `agent_conversations` rows we retain on disk before
/// `select_conversations_to_evict` starts dropping trees. 200 gives roughly
/// 10–40 orchestration sessions of headroom; trees are kept atomically, so an
/// active orchestration session is never split even if it pushes past the cap.
pub(super) const MAX_PERSISTED_CONVERSATION_COUNT: usize = 200;

pub(super) fn upsert_agent_conversation<'a>(
    conn: &mut SqliteConnection,
    conversation_id_param: &str,
    tasks: impl IntoIterator<Item = &'a api::Task>,
    conversation_data_param: AgentConversationData,
) -> Result<(), UpsertConversationError> {
    
    use diesel::QueryDsl;
    use schema::agent_conversations::dsl::*;
    use schema::agent_tasks::dsl as tasks_dsl;

    let serialized_conversation_data = serde_json::to_string(&conversation_data_param)?;

    // `tasks` is always a full snapshot of the conversation's current task set,
    // so persistence is replace/delete-missing: any `agent_tasks` row for this
    // conversation that is not in the snapshot is deleted below. This keeps
    // pruned subtasks (e.g. those dropped by a conversation rewind) from
    // lingering as orphan rows and being resurrected on restore — reads load
    // every row for the conversation, unfiltered.
    let tasks: Vec<&api::Task> = tasks.into_iter().collect();
    // Every id in the snapshot is kept, including one whose blob is skipped as
    // oversized below: we could not write its new version, so deleting the row
    // we already have would throw away the last copy of that task.
    let kept_task_ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();

    // Derive the task-based summary here (on the writer thread) so every
    // write path keeps the `summary` column in sync with the task snapshot,
    // letting startup list conversations without loading `agent_tasks`.
    let serialized_summary =
        serde_json::to_string(&AgentConversationSummary::from_tasks(tasks.iter().copied())).ok();

    conn.transaction::<_, Error, _>(|conn| {
        // Upsert the conversation level metadata
        let new_conversation = NewAgentConversation {
            conversation_id: conversation_id_param.to_owned(),
            conversation_data: serialized_conversation_data,
            summary: serialized_summary,
        };

        diesel::insert_into(agent_conversations::table())
            .values(&new_conversation)
            .on_conflict(conversation_id)
            .do_update()
            .set(&new_conversation)
            .execute(conn)?;

        // Upsert each task
        for task in &tasks {
            // Check encoded size before allocating the full BLOB to avoid a
            // large heap allocation that is immediately discarded.
            let encoded_len = task.encoded_len();
            if encoded_len > MAX_TASK_BLOB_BYTES {
                // `error`, not `warn`: this drops a turn's worth of the user's conversation
                // on the floor, and it is the only notice anyone gets. The row is left
                // alone rather than deleted, so what survives on disk is whichever earlier
                // version of this task last fit — or nothing at all, if none ever did, in
                // which case restoring this conversation will fail with `NoRootTask` (or
                // quietly lose the subtask) even though its `summary` still advertises it as
                // restorable. See `MAX_TASK_BLOB_BYTES`.
                log::error!(
                    "Task {} encoded size is {} bytes (write limit {}); not persisting it. \
                     Any existing row for this task keeps its previous, older contents, and \
                     this conversation may not restore.",
                    task.id,
                    encoded_len,
                    MAX_TASK_BLOB_BYTES,
                );
                continue;
            }
            let task_binary = task.encode_to_vec();
            let new_task = NewAgentTask {
                conversation_id: conversation_id_param.to_owned(),
                task_id: task.id.clone(),
                task: task_binary,
            };

            if let Err(e) = diesel::insert_into(agent_tasks::table)
                .values(&new_task)
                .on_conflict(tasks_dsl::task_id)
                .do_update()
                .set(&new_task)
                .execute(conn)
            {
                log::warn!("Failed to upsert task {e:?}");
                return Err(e);
            }
        }

        // Delete any tasks for this conversation that are no longer part of the
        // snapshot (replace semantics). `ne_all` with an empty set matches every
        // row, so a fully-rewound conversation (no persisted tasks) has all of
        // its task rows cleared.
        diesel::delete(
            agent_tasks::table
                .filter(tasks_dsl::conversation_id.eq(conversation_id_param))
                .filter(tasks_dsl::task_id.ne_all(kept_task_ids)),
        )
        .execute(conn)?;

        // Prune old conversations if we exceed MAX_PERSISTED_CONVERSATION_COUNT.
        //
        // Eviction is tree-aware: parents and children are an atomic unit, so
        // we never delete a parent whose child still lives in the DB (or vice
        // versa). See `select_conversations_to_evict`.
        let conversation_count: i64 = agent_conversations::table().count().get_result(conn)?;
        if conversation_count > MAX_PERSISTED_CONVERSATION_COUNT as i64 {
            let all_rows: Vec<AgentConversationRecord> = agent_conversations::table()
                .select(AgentConversationRecord::as_select())
                .load(conn)?;
            let conversations_to_remove =
                select_conversations_to_evict(&all_rows, MAX_PERSISTED_CONVERSATION_COUNT);
            if !conversations_to_remove.is_empty() {
                delete_agent_conversations(conn, conversations_to_remove)?;
            }
        }

        Ok(())
    })?;

    Ok(())
}

/// Evicts whole orchestration trees so the remaining set fits within `limit`.
/// Trees are sorted freshest-first by `max(member.last_modified_at)` (ties
/// broken by `root_id` ASC); the freshest tree is always retained, every
/// older tree is kept only if cumulative kept rows + tree size ≤ `limit`,
/// and once any tree exceeds the budget every older tree is evicted as well.
/// Parse failures and orphan parent references are treated as their own
/// root rather than linked into another tree. Returns a stable
/// `conversation_id`-sorted vector.
pub(super) fn select_conversations_to_evict(
    rows: &[AgentConversationRecord],
    limit: usize,
) -> Vec<String> {
    if rows.len() <= limit {
        return Vec::new();
    }

    // Map each row to its declared parent, but only when that parent is
    // itself in `rows`; orphan references collapse to a root.
    let row_set: HashSet<&str> = rows.iter().map(|r| r.conversation_id.as_str()).collect();
    let parent_by_id: HashMap<&str, Option<String>> = rows
        .iter()
        .map(|r| {
            let parent = serde_json::from_str::<AgentConversationData>(&r.conversation_data)
                .ok()
                .and_then(|d| d.parent_conversation_id)
                .filter(|p| row_set.contains(p.as_str()));
            (r.conversation_id.as_str(), parent)
        })
        .collect();

    fn find_root<'a>(start: &'a str, parent_by_id: &'a HashMap<&str, Option<String>>) -> &'a str {
        let mut current = start;
        let mut seen: HashSet<&str> = HashSet::new();
        loop {
            // Defensive: cycle entries become their own root.
            if !seen.insert(current) {
                return current;
            }
            match parent_by_id.get(current) {
                Some(Some(p)) => current = p.as_str(),
                _ => return current,
            }
        }
    }

    let mut trees: HashMap<String, Vec<&AgentConversationRecord>> = HashMap::new();
    for row in rows {
        let root = find_root(row.conversation_id.as_str(), &parent_by_id).to_owned();
        trees.entry(root).or_default().push(row);
    }

    let mut tree_list: Vec<(NaiveDateTime, String, Vec<&AgentConversationRecord>)> = trees
        .into_iter()
        .map(|(root, members)| {
            let effective = members
                .iter()
                .map(|r| r.last_modified_at)
                .max()
                .expect("tree always has at least one member by construction");
            (effective, root, members)
        })
        .collect();
    tree_list.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    let mut kept_count: usize = 0;
    let mut evicted: Vec<String> = Vec::new();
    let mut tree_iter = tree_list.into_iter();

    // Freshest tree is always retained, even when it alone exceeds `limit`.
    if let Some((_effective, _root, members)) = tree_iter.next() {
        kept_count += members.len();
    }

    let mut stopped = false;
    for (_effective, _root, members) in tree_iter {
        let tree_size = members.len();
        let keep_this = !stopped && kept_count + tree_size <= limit;
        if keep_this {
            kept_count += tree_size;
        } else {
            stopped = true;
            for m in &members {
                evicted.push(m.conversation_id.clone());
            }
        }
    }

    evicted.sort();
    evicted
}

/// Reads conversation metadata from `agent_conversations` only, without
/// loading or decoding the (potentially very large) `agent_tasks` blobs.
///
/// The returned [`AgentConversation`]s have empty `tasks`; consumers use the
/// summary on each record plus lazy per-conversation loading
/// ([`read_agent_conversation_by_id`]) for full task data.
///
/// Rows written before the `summary` column existed get their summary derived
/// here from their own task snapshot (the one-time slow path); those
/// derivations are returned as backfills so the caller can persist them (via
/// [`backfill_conversation_summaries`]) and keep subsequent startups
/// metadata-only.
///
/// Ported from the pin (`app/src/persistence/agent.rs:246-303`, `02b53fcd8`) for #431. Replaces
/// the previous `read_agent_conversations`, which unconditionally loaded and decoded every
/// `agent_tasks` blob for every conversation on every startup just to compute a possibly-already-
/// valid summary.
pub(super) fn read_agent_conversation_metadata(
    conn: &mut SqliteConnection,
) -> Result<(Vec<AgentConversation>, Vec<ConversationSummaryBackfill>), diesel::result::Error> {
    use schema::agent_conversations::dsl::*;

    let records: Vec<AgentConversationRecord> = agent_conversations
        .select(AgentConversationRecord::as_select())
        .load(conn)?;

    let mut conversations = Vec::with_capacity(records.len());
    let mut backfills = Vec::new();
    for mut record in records {
        let has_valid_summary = record
            .summary
            .as_deref()
            .is_some_and(|json| serde_json::from_str::<AgentConversationSummary>(json).is_ok());
        if !has_valid_summary {
            let task_records: Vec<AgentTaskRecord> = agent_tasks::table
                .filter(schema::agent_tasks::dsl::conversation_id.eq(&record.conversation_id))
                .select(AgentTaskRecord::as_select())
                .load(conn)?;

            let mut decoded_tasks = Vec::with_capacity(task_records.len());
            let mut decode_failed = false;
            for task_record in task_records {
                match api::Task::decode(&task_record.task[..]) {
                    Ok(task) => decoded_tasks.push(task),
                    Err(e) => {
                        log::error!("Failed to decode task protobuf: {e}");
                        decode_failed = true;
                        break;
                    }
                }
            }
            // Matches the historical behavior of dropping conversations with
            // undecodable tasks.
            if decode_failed {
                continue;
            }

            let derived = AgentConversationSummary::from_tasks(decoded_tasks.iter());
            let Ok(summary_json) = serde_json::to_string(&derived) else {
                continue;
            };
            let previous_summary = record.summary.replace(summary_json.clone());
            backfills.push(ConversationSummaryBackfill {
                conversation_id: record.conversation_id.clone(),
                summary_json,
                previous_summary,
                last_modified_at: record.last_modified_at,
            });
        }

        conversations.push(AgentConversation {
            conversation: record,
            tasks: vec![],
        });
    }

    Ok((conversations, backfills))
}

/// Persists read-time-derived summaries into the `summary` column so the
/// derivation in [`read_agent_conversation_metadata`] only happens once per
/// row.
///
/// Ported from the pin (`app/src/persistence/agent.rs:313-351`, `02b53fcd8`) verbatim, for #431.
pub(super) fn backfill_conversation_summaries(
    conn: &mut SqliteConnection,
    backfills: Vec<ConversationSummaryBackfill>,
) -> Result<(), diesel::result::Error> {
    use schema::agent_conversations::dsl::*;

    conn.transaction::<_, Error, _>(|conn| {
        for backfill in backfills {
            // Compare-and-set against the value observed at read time (NULL
            // or invalid JSON), so invalid summaries heal while a newer
            // write's summary is never overwritten.
            let update_target =
                agent_conversations.filter(conversation_id.eq(&backfill.conversation_id));
            let updated = match &backfill.previous_summary {
                Some(previous_summary) => {
                    diesel::update(update_target.filter(summary.eq(previous_summary)))
                        .set(summary.eq(&backfill.summary_json))
                        .execute(conn)?
                }
                None => diesel::update(update_target.filter(summary.is_null()))
                    .set(summary.eq(&backfill.summary_json))
                    .execute(conn)?,
            };

            // The `update_last_modified_at_for_agent_conversations` trigger
            // bumps `last_modified_at` on any update that leaves it
            // unchanged; restore the original value so backfilling doesn't
            // reorder the history list. Setting an explicit (different)
            // value keeps the trigger from firing on this second update.
            if updated > 0 {
                diesel::update(
                    agent_conversations.filter(conversation_id.eq(&backfill.conversation_id)),
                )
                .set(last_modified_at.eq(backfill.last_modified_at))
                .execute(conn)?;
            }
        }
        Ok(())
    })
}

/// Read a single agent conversation by its ID, including decoded tasks.
pub(crate) fn read_agent_conversation_by_id(
    conn: &mut SqliteConnection,
    conversation_id_str: &str,
) -> Result<Option<AgentConversation>, diesel::result::Error> {
    use schema::agent_conversations::dsl as convo_dsl;
    use schema::agent_tasks::dsl as tasks_dsl;

    let maybe_record: Option<AgentConversationRecord> = convo_dsl::agent_conversations
        .filter(convo_dsl::conversation_id.eq(conversation_id_str.to_owned()))
        .select(AgentConversationRecord::as_select())
        .first(conn)
        .optional()?;

    let Some(conversation_record) = maybe_record else {
        return Ok(None);
    };

    let task_records: Vec<AgentTaskRecord> = schema::agent_tasks::table
        .filter(tasks_dsl::conversation_id.eq(conversation_id_str))
        .select(AgentTaskRecord::as_select())
        .load(conn)?;

    let mut decoded_tasks = Vec::new();
    for task_record in task_records.into_iter() {
        match api::Task::decode(&task_record.task[..]) {
            Ok(task) => decoded_tasks.push(task),
            Err(e) => {
                log::error!("Failed to decode task protobuf: {e}");
            }
        }
    }

    Ok(Some(AgentConversation {
        conversation: conversation_record,
        tasks: decoded_tasks,
    }))
}

pub(super) fn delete_agent_conversations(
    conn: &mut SqliteConnection,
    conversation_ids: Vec<String>,
) -> Result<(), diesel::result::Error> {
    use diesel::ExpressionMethods;
    use diesel::QueryDsl;
    use schema::agent_conversations::dsl::*;
    use schema::agent_tasks::dsl as tasks_dsl;

    conn.transaction::<_, Error, _>(|conn| {
        // Delete tasks for these conversations first (due to foreign key constraint)
        diesel::delete(
            agent_tasks::table.filter(tasks_dsl::conversation_id.eq_any(&conversation_ids)),
        )
        .execute(conn)?;

        // Delete the conversations themselves
        diesel::delete(
            agent_conversations::table().filter(conversation_id.eq_any(&conversation_ids)),
        )
        .execute(conn)?;

        Ok(())
    })?;

    Ok(())
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
