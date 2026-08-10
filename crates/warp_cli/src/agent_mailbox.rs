//! Local on-disk mailbox for agent-to-agent messaging.
//!
//! This is the local BYOP replacement for the pin's cloud-backed `oz run
//! message send`/`list` mailbox. That surface was removed entirely --
//! `crates/warp_cli/src/lib_tests.rs`'s `run_command_is_removed` asserts
//! `CliCommand::Run` no longer parses -- because the pin's `task.rs` (`oz run
//! message *`) is a client for Warp's server-side "hosted CLI task" registry:
//! `MessageSendArgs`/`MessageListArgs` address runs by a server-assigned ID
//! and the pin's executor (`server::server_api::ServerApiProvider`,
//! `SendAgentMessageRequest`/`Response`) posts to Warp's GraphQL backend.
//! There is no local equivalent of that registry, so `oz run` could not be
//! ported as-is.
//!
//! What *is* local is the run/parent-run identity children already carry:
//! `OZ_RUN_ID`/`OZ_PARENT_RUN_ID` (`app/src/pane_group/pane/local_harness_launch.rs`),
//! set from a locally generated [`AmbientAgentTaskId`](../../app/src/ai/ambient_agents.rs)
//! rather than a server-assigned run. This module gives that local run ID a
//! mailbox: a plain directory of JSON message files, keyed by the recipient's
//! run ID, that any local process for the same OS user can read or write
//! without a running app instance, a network listener, or a settings gate.
//!
//! This is deliberately simpler than `crates/local_control` (`warpctrl`):
//! that surface authenticates arbitrary external automation clients against
//! a live app instance over loopback HTTP, gated behind Settings > Scripting.
//! A spawned child here is not an external client -- it is a trusted local
//! process Zap itself created, identified by an unforgeable env var the
//! parent process set at spawn time -- so a filesystem mailbox keyed by run
//! ID is sufficient and works whether or not any Zap GUI/TUI instance is
//! currently running.
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Overrides the mailbox root directory. Used by tests so they don't touch
/// the real per-user state directory; see `crates/warp_core/src/paths.rs`'s
/// doc comment on why its path functions should not be called directly from
/// tests.
pub const AGENT_MAILBOX_ROOT_ENV: &str = "OZ_AGENT_MAILBOX_ROOT";

const AGENT_MAILBOX_DIR_NAME: &str = "agent-mailbox";

/// One message delivered to a run's mailbox.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxMessage {
    /// Client-generated message ID (a UUID), unique per send.
    pub message_id: String,
    /// The sender's run ID.
    pub from: String,
    /// The recipient's run ID (the mailbox this message was filed under).
    pub to: String,
    pub subject: String,
    pub body: String,
    pub sent_at: DateTime<Utc>,
}

/// Returns the root directory under which every run's mailbox subdirectory
/// lives. Overridable via [`AGENT_MAILBOX_ROOT_ENV`] for tests and for
/// callers that want an isolated mailbox (e.g. integration harnesses).
pub fn mailbox_root() -> PathBuf {
    if let Ok(dir) = std::env::var(AGENT_MAILBOX_ROOT_ENV) {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    warp_core::paths::state_dir()
        .join("oz")
        .join(AGENT_MAILBOX_DIR_NAME)
}

/// Run IDs are locally generated UUIDs (see `AmbientAgentTaskId::new_local`),
/// but the mailbox directory name is derived from caller-supplied input, so
/// defensively collapse anything that isn't a plain path segment rather than
/// trusting it not to contain `/` or `..`.
fn sanitize_run_id_for_path(run_id: &str) -> String {
    run_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn inbox_dir(root: &Path, run_id: &str) -> PathBuf {
    root.join(sanitize_run_id_for_path(run_id))
}

fn message_file_name(message: &MailboxMessage) -> String {
    // Zero-padded nanosecond timestamp prefix keeps `read_dir` + sort in
    // send order even though message IDs are random UUIDs.
    let nanos = message.sent_at.timestamp_nanos_opt().unwrap_or_default();
    format!("{nanos:020}-{}.json", message.message_id)
}

/// Writes `bytes` to `path` atomically (write to a sibling temp file, then
/// rename), so a concurrent reader never observes a partially written
/// message file.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{} has no parent directory", path.display()))?;
    fs::create_dir_all(parent)
        .with_context(|| format!("Failed to create mailbox directory {}", parent.display()))?;
    let tmp_path = parent.join(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("message"),
        Uuid::new_v4()
    ));
    fs::write(&tmp_path, bytes)
        .with_context(|| format!("Failed to write {}", tmp_path.display()))?;
    fs::rename(&tmp_path, path)
        .with_context(|| format!("Failed to finalize {}", path.display()))?;
    Ok(())
}

/// Delivers one message from `from` to `to`'s local mailbox under `root`.
///
/// Returns the persisted [`MailboxMessage`] (with its generated
/// `message_id` and `sent_at`) so the caller can report or echo it back.
pub fn send_message(
    root: &Path,
    from: &str,
    to: &str,
    subject: &str,
    body: &str,
) -> Result<MailboxMessage> {
    let message = MailboxMessage {
        message_id: Uuid::new_v4().to_string(),
        from: from.to_string(),
        to: to.to_string(),
        subject: subject.to_string(),
        body: body.to_string(),
        sent_at: Utc::now(),
    };
    let path = inbox_dir(root, &message.to).join(message_file_name(&message));
    let bytes = serde_json::to_vec_pretty(&message).context("Failed to serialize message")?;
    write_atomically(&path, &bytes)?;
    Ok(message)
}

/// Lists up to `limit` messages delivered to `run_id`'s mailbox under
/// `root`, oldest first, most recent `limit` kept.
///
/// Returns an empty list -- not an error -- for a run that has no mailbox
/// yet, matching a fresh inbox with no messages.
pub fn list_messages(root: &Path, run_id: &str, limit: usize) -> Result<Vec<MailboxMessage>> {
    let dir = inbox_dir(root, run_id);
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .with_context(|| format!("Failed to read mailbox directory {}", dir.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();

    let mut messages = Vec::with_capacity(paths.len().min(limit));
    for path in paths {
        let bytes = fs::read(&path)
            .with_context(|| format!("Failed to read mailbox message {}", path.display()))?;
        match serde_json::from_slice::<MailboxMessage>(&bytes) {
            Ok(message) => messages.push(message),
            Err(err) => {
                log::warn!(
                    "Skipping malformed mailbox message {}: {err:#}",
                    path.display()
                );
            }
        }
    }

    if messages.len() > limit {
        let drop_count = messages.len() - limit;
        messages.drain(0..drop_count);
    }
    Ok(messages)
}

#[cfg(test)]
#[path = "agent_mailbox_tests.rs"]
mod tests;
