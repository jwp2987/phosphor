use chrono::NaiveDateTime;
use diesel::associations::HasTable;
use diesel::{prelude::*, result::Error, SqliteConnection};
use prost::Message;
use std::collections::{HashMap, HashSet};
use warp_multi_agent_api as api;

use super::ConversationSummaryBackfill;
use super::model::{AgentConversation, AgentConversationData, AgentConversationSummary};
use super::PersistedTaskRetention;
use crate::persistence::model::{AgentConversationRecord, AgentTaskRecord};
use crate::persistence::schema::{self, agent_conversations, agent_tasks};

/// Maximum size of a single serialized `api::Task` protobuf BLOB this code will **write** to
/// `agent_tasks.task`.
///
/// This is a write-side cap only, and an earlier version of this comment said otherwise — it
/// claimed tasks over the limit were "skipped on both write and read to prevent startup OOM
/// when all task records are loaded at once". Neither half held: the read that sentence
/// describes hasn't existed since #431, when startup moved to
/// [`read_agent_conversation_metadata`], which reads the `summary` column and touches
/// `agent_tasks` only for rows written before that column existed. Both readers that DO
/// decode `agent_tasks` blobs decode unconditionally and never consult this constant.
///
/// **What happens when a task's encoded size exceeds the cap, as of the fix below:**
/// [`prune_oversized_messages`] is tried first, replacing the content of the task's largest
/// non-query messages (`ToolCallResult` above all — see that function's doc) with a small
/// placeholder, largest first, until the task fits. This is lossy for the pruned messages'
/// own content, but **not** for the conversation's restorability: a task's identity and
/// dependency structure are untouched, so [`AgentConversationSummary::from_tasks`]'s verdict
/// on it does not change, and the pruned task is written and restores normally. Only when a
/// task still doesn't fit after pruning everything prunable is it dropped from the write
/// entirely — its existing row, if any, is left alone rather than deleted (see
/// `kept_task_ids`), so the database keeps whichever earlier version of that task last fit,
/// or the task is simply absent if none ever did. **The summary written in that case is
/// honest about it**: [`upsert_agent_conversation_with_retention`] excludes the dropped task
/// from the snapshot it derives `serialized_summary` from, and forces `is_restorable: false`
/// whenever anything was dropped (the structural check alone isn't enough — see its comment)
/// instead of persisting a `summary` that promised "restorable" for a conversation that fails
/// to open with `RestoreConversationError::NoRootTask` or a subtask that's silently gone.
/// There is no UI notification beyond the `log::error!` at the drop site: this runs on the
/// SQLite writer thread with no `AppContext` to raise one from (`report_db_error` in
/// `sqlite.rs`, the writer's only other error-reporting path, is log-only for the same
/// reason) — a real fix needs a channel from that thread to the UI, which is a separate,
/// larger change than a per-task size guard.
///
/// **Read-side enforcement is still deliberately not added here**, and this part of the
/// analysis is unchanged by the fix above: a size check before `api::Task::decode` would
/// skip exactly the blobs written before pruning existed, turning "a conversation that
/// restores, slowly" into "a conversation that has silently vanished from history" —
/// `is_restorable` is what startup filters on, and eviction deletes rows. Making the OOM
/// guarantee real for those legacy rows needs a migration: walk `agent_tasks` once, and
/// either re-encode oversized rows with the same pruning this write path now does, or drop
/// them with the user told which conversations were affected. Only after that can a reader
/// refuse an oversized blob without destroying data that pruning could have saved.
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
pub(crate) enum UpsertConversationError {
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

/// [`upsert_agent_conversation_with_retention`] with the default
/// [`PersistedTaskRetention::DeleteMissing`]. Test-only: production writes go through the
/// sqlite writer, which always passes the retention the conversation asked for.
#[cfg(test)]
pub(crate) fn upsert_agent_conversation<'a>(
    conn: &mut SqliteConnection,
    conversation_id_param: &str,
    tasks: impl IntoIterator<Item = &'a api::Task>,
    conversation_data_param: AgentConversationData,
) -> Result<(), UpsertConversationError> {
    upsert_agent_conversation_with_retention(
        conn,
        conversation_id_param,
        tasks,
        conversation_data_param,
        PersistedTaskRetention::DeleteMissing,
    )
}

/// Whether a stored `summary` column describes a conversation with real content, i.e. one
/// that must not be replaced by a summary derived from a snapshot that does not hold it.
fn stored_summary_names_initial_query(stored_summary: &str) -> bool {
    serde_json::from_str::<AgentConversationSummary>(stored_summary)
        .is_ok_and(|summary| !summary.initial_query.is_empty())
}

/// Whether a message's content may be replaced by [`prune_oversized_messages`].
///
/// `UserQuery` is the user's own input and must never be silently discarded.
/// `SystemQuery` is excluded too: its `AutoCodeDiff` variant is what
/// [`AgentConversationSummary::from_tasks`] checks to set
/// `is_unlisted_auto_code_diff`, so pruning it would change that classification
/// as a side effect of a size fix. Everything else -- `ToolCallResult` above
/// all, per `MAX_TASK_BLOB_BYTES`'s doc comment -- is model/tool output that
/// the size limit exists to bound in the first place.
fn message_content_is_prunable(message: &api::Message) -> bool {
    !matches!(
        message.message,
        Some(api::message::Message::UserQuery(_))
            | Some(api::message::Message::SystemQuery(_))
            // Already a placeholder from a previous prune pass -- nothing left to shrink.
            | Some(api::message::Message::DebugOutput(_))
    )
}

/// Shrinks `task` to at most `max_bytes` encoded, if possible, by replacing the
/// content of its largest prunable messages (see [`message_content_is_prunable`])
/// with a small placeholder, largest first, stopping as soon as the task fits.
///
/// Returns the ids of every message whose content was replaced, in the order
/// they were pruned; empty if `task` already fit or if it fit only because it
/// had nothing prunable at all (both callable, since the return doesn't
/// distinguish "already fine" from "nothing could be done" -- the caller
/// checks `task.encoded_len()` again either way).
///
/// Each pruned message keeps its `id`/`task_id`/`request_id`/`timestamp`, so
/// anything that pairs messages by id (a `ToolCall` with its `ToolCallResult`,
/// for instance) keeps its anchor; only the message's own content shrinks to a
/// short marker naming how many bytes were removed and why. This is a superset
/// of "prune tool-output payloads" (the case `MAX_TASK_BLOB_BYTES`'s doc
/// comment names as the one that actually reaches this size): it prunes
/// whichever non-query messages are largest, so it also covers the rarer case
/// of an oversized model response (`AgentOutput`) without needing this
/// function to know the specific shape of every tool result variant.
///
/// **Why the placeholder is `DebugOutput`, specifically, and not a truncated
/// copy of the original variant:** truncating in place would need per-variant
/// knowledge of ~30 `ToolCallResult` result kinds (shell output, file
/// contents, grep matches, ...), each with a different field to shorten, which
/// is real scope this function does not take on. Of the generically
/// constructible alternatives, `DebugOutput` was chosen over reassigning the
/// slot to `AgentOutput` (which would misattribute fabricated text to the
/// model) or `ToolCallResult`'s own `Server` result kind (which
/// `convert_conversation.rs` documents as producing no exchange at all --
/// same visibility problem as `DebugOutput`, with the added risk of being
/// mistaken for a real server-issued result). A pruned `ToolCallResult`
/// leaves its paired `ToolCall` without a result once re-sent to the model,
/// which is not a new failure mode this introduces: `chat_stream.rs` already
/// detects and drops orphaned tool calls/responses (`orphan_call_ids`,
/// `orphan_or_misordered_tool_response`) for the same shape of gap that a
/// crash or a cancelled operation produces today.
///
/// **Known limitation, stated rather than hidden:** `DebugOutput` is
/// documented (`task.proto`) as staging/local-development-only content, and
/// `render_collapsible_debug_output`'s call site gates it on
/// `ChannelState::enable_debug_features()` -- so the placeholder text is
/// invisible in the restored transcript on an ordinary build. The
/// conversation still restores and remains usable; what's lost is an
/// in-transcript indicator that something was pruned, beyond the `log::error!`
/// at the write site. A visible, generic "content removed" message kind does
/// not exist in the current protocol; adding one is out of scope here.
fn prune_oversized_messages(task: &mut api::Task, max_bytes: usize) -> Vec<String> {
    let mut prunable: Vec<usize> = task
        .messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message_content_is_prunable(message))
        .map(|(i, _)| i)
        .collect();
    // Largest first: closing the size gap with the fewest, most size-efficient
    // prunes keeps as much of the conversation's real content as possible.
    prunable.sort_by_key(|&i| std::cmp::Reverse(task.messages[i].encoded_len()));

    let mut pruned_ids = Vec::new();
    for i in prunable {
        if task.encoded_len() <= max_bytes {
            break;
        }
        let message = &mut task.messages[i];
        let original_len = message.encoded_len();
        message.message = Some(api::message::Message::DebugOutput(
            api::message::DebugOutput {
                text: format!(
                    "[Phosphor removed a {original_len}-byte message here: this conversation's \
                 stored size exceeded the {max_bytes}-byte per-task limit.]"
                ),
            },
        ));
        pruned_ids.push(message.id.clone());
    }
    pruned_ids
}

pub(crate) fn upsert_agent_conversation_with_retention<'a>(
    conn: &mut SqliteConnection,
    conversation_id_param: &str,
    tasks: impl IntoIterator<Item = &'a api::Task>,
    conversation_data_param: AgentConversationData,
    retention: PersistedTaskRetention,
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
    //
    // Except for an empty snapshot under the default retention (see
    // `PersistedTaskRetention`), which deletes nothing: `ne_all` with an empty set matches
    // every row, so without this an in-memory conversation that merely failed to load its
    // tasks erased all of them on its next save. Only a rewind past the first exchange
    // asks for an empty snapshot to be honoured.
    let tasks: Vec<&api::Task> = tasks.into_iter().collect();
    let empty_snapshot_is_authoritative = match retention {
        PersistedTaskRetention::DeleteMissing => false,
        PersistedTaskRetention::DeleteMissingEvenIfEmpty => true,
    };
    let delete_missing_rows = !tasks.is_empty() || empty_snapshot_is_authoritative;
    // Same rule for the summary: an empty snapshot that is not authoritative must not
    // replace a summary that names a real initial query, or the history list (which reads
    // only this column at startup) drops the conversation.
    let may_keep_stored_summary = tasks.is_empty() && !empty_snapshot_is_authoritative;
    // Every id in the snapshot is kept, including one whose blob is skipped as
    // oversized below: we could not write its new version, so deleting the row
    // we already have would throw away the last copy of that task.
    let kept_task_ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();

    // Fit each task under `MAX_TASK_BLOB_BYTES` before it reaches a BLOB write.
    // An oversized task is pruned first (see `prune_oversized_messages`) rather
    // than dropped outright, so the conversation stays restorable in the
    // common case; only a task that still doesn't fit after pruning is
    // dropped from this write, same as before pruning existed (its existing
    // row, if any, is left alone -- see `kept_task_ids` above).
    let mut tasks_to_write: Vec<api::Task> = Vec::with_capacity(tasks.len());
    let mut dropped_task_ids: HashSet<&str> = HashSet::new();
    for task in &tasks {
        let encoded_len = task.encoded_len();
        if encoded_len <= MAX_TASK_BLOB_BYTES {
            tasks_to_write.push((*task).clone());
            continue;
        }
        let mut pruned = (*task).clone();
        let pruned_message_ids = prune_oversized_messages(&mut pruned, MAX_TASK_BLOB_BYTES);
        let pruned_len = pruned.encoded_len();
        if !pruned_message_ids.is_empty() && pruned_len <= MAX_TASK_BLOB_BYTES {
            log::error!(
                "Task {} in conversation {conversation_id_param} was {encoded_len} bytes \
                 (limit {MAX_TASK_BLOB_BYTES}); pruned {} oversized message(s) down to \
                 {pruned_len} bytes and persisted the pruned version so the conversation \
                 stays restorable. Pruned message ids: {pruned_message_ids:?}.",
                task.id,
                pruned_message_ids.len(),
            );
            tasks_to_write.push(pruned);
        } else {
            // `error`, not `warn`: this drops a turn's worth of the user's conversation on
            // the floor, and short of this log line there is no other notice today -- this
            // runs on the SQLite writer thread with no `AppContext` to raise a UI
            // notification from (the writer's own error path, `report_db_error` in
            // `sqlite.rs`, is log-only for the identical reason). The row is left alone
            // rather than deleted, so what survives on disk is whichever earlier version of
            // this task last fit, or nothing at all if none ever did. Excluding this task's
            // id from `dropped_task_ids`'s complement below keeps the summary this write
            // persists honest about it.
            log::error!(
                "Task {} in conversation {conversation_id_param} is {encoded_len} bytes \
                 (limit {MAX_TASK_BLOB_BYTES}) and could not be pruned under the limit; not \
                 persisting it. Any existing row for this task keeps its previous, older \
                 contents. This conversation's summary is now marked not-restorable, so it is \
                 excluded from history instead of being listed there and failing to open.",
                task.id,
            );
            dropped_task_ids.insert(task.id.as_str());
        }
    }

    // Derive the task-based summary here (on the writer thread) so every write path keeps
    // the `summary` column in sync with what actually lands on disk, letting startup list
    // conversations without loading `agent_tasks`. Built from `tasks` (the original,
    // unpruned snapshot) minus anything in `dropped_task_ids`, not from `tasks_to_write`:
    // pruning only ever replaces a message's content, never a task's identity or dependency
    // structure that `AgentConversationSummary::from_tasks` inspects, so the two agree except
    // on dropped tasks -- and computing it from the original snapshot means a dropped task
    // still contributes its real `UserQuery`/`AutoCodeDiff` content to `initial_query`/
    // `is_unlisted_auto_code_diff` instead of being silently treated as absent everywhere.
    let mut conversation_summary = AgentConversationSummary::from_tasks(
        tasks
            .iter()
            .copied()
            .filter(|task| !dropped_task_ids.contains(task.id.as_str())),
    );
    // `is_restorable` from `from_tasks` is a structural check
    // (`tasks_are_restorable`: a single well-formed root, or the one documented multi-root
    // exception) over the tasks it was GIVEN -- it has no way to know a task existed and was
    // dropped, and its `tasks.len() <= 1` base case means dropping every task down to zero
    // or one survivor reads as trivially restorable. So this is forced rather than left to
    // that check whenever anything was dropped: without it, dropping a conversation's sole
    // task persisted `is_restorable: true` for a conversation with no tasks at all, and
    // dropping a non-root leaf could coincidentally leave a single-root shape that also read
    // as restorable while failing to open with `RestoreConversationError::NoRootTask` or
    // quietly missing a subtask the user actually had.
    if !dropped_task_ids.is_empty() {
        conversation_summary.is_restorable = false;
    }
    let serialized_summary = serde_json::to_string(&conversation_summary).ok();

    conn.transaction::<_, Error, _>(|conn| {
        let summary_to_write = if may_keep_stored_summary {
            let stored_summary: Option<String> = agent_conversations
                .filter(conversation_id.eq(conversation_id_param))
                .select(summary)
                .first::<Option<String>>(conn)
                .optional()?
                .flatten();
            match stored_summary {
                Some(stored) if stored_summary_names_initial_query(&stored) => Some(stored),
                _ => serialized_summary,
            }
        } else {
            serialized_summary
        };

        // Upsert the conversation level metadata
        let new_conversation = NewAgentConversation {
            conversation_id: conversation_id_param.to_owned(),
            conversation_data: serialized_conversation_data,
            summary: summary_to_write,
        };

        diesel::insert_into(agent_conversations::table())
            .values(&new_conversation)
            .on_conflict(conversation_id)
            .do_update()
            .set(&new_conversation)
            .execute(conn)?;

        // Upsert each task that fit under `MAX_TASK_BLOB_BYTES`, as-is or pruned -- sizing,
        // pruning and the dropped-task logging all already happened above, before the
        // transaction opened.
        for task in &tasks_to_write {
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
        // snapshot (replace semantics), when the retention allows it. `ne_all`
        // with an empty set matches every row, so an empty snapshot clears every
        // row only under `DeleteMissingEvenIfEmpty` (a rewind past the first
        // exchange); see `delete_missing_rows` above.
        if delete_missing_rows {
            diesel::delete(
                agent_tasks::table
                    .filter(tasks_dsl::conversation_id.eq(conversation_id_param))
                    .filter(tasks_dsl::task_id.ne_all(kept_task_ids)),
            )
            .execute(conn)?;
        }

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
pub(crate) fn read_agent_conversation_metadata(
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
///
/// Returns `Err(DeserializationError)` when any of the conversation's task rows fails to
/// decode, rather than a conversation that silently lacks that task.
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

    // Any undecodable row fails the whole read. The pin (`4111d08f9`) logs and skips the row
    // instead, handing back a conversation that is missing that task — or, when no row
    // decodes, one with no tasks at all, for which the local-DB restore synthesizes an empty
    // optimistic root. Either way the caller gets an editable conversation that does not
    // hold what is on disk, and its first save used to prune the rows it could not read.
    // Refusing keeps the rows intact for a build that can decode them, and matches what
    // `read_agent_conversation_metadata` already does for a legacy row with an undecodable
    // task (drops it from the history list). See `DECLINED.md` → `IMPROVED`.
    let mut decoded_tasks = Vec::with_capacity(task_records.len());
    for task_record in task_records.into_iter() {
        match api::Task::decode(&task_record.task[..]) {
            Ok(task) => decoded_tasks.push(task),
            Err(e) => {
                log::error!(
                    "Failed to decode task {} of conversation {conversation_id_str}: {e}; \
                     not restoring the conversation, so its persisted tasks are left intact",
                    task_record.task_id,
                );
                return Err(diesel::result::Error::DeserializationError(Box::new(e)));
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
