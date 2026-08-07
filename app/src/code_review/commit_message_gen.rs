//! Client-side AI commit-message drafting for the code-review commit dialog.
//!
//! ## Sanctioned cloud→BYOP divergence (AGENTS §5.10)
//!
//! Warp drafts the commit message through a **server-side** RPC: its
//! `git_actions::generate_commit_message` hands the diff to
//! `AIClient::generate_code_review_content`, and Warp's cloud backend — which
//! already holds the user's AI credentials — runs the model. Phosphor has no
//! cloud backend and no server-held credentials, so that RPC has no counterpart
//! here. The only place the configured provider is reachable at all is the app
//! process, which is why generation moves to the client.
//!
//! This divergence is acceptable because it changes *who holds the credential
//! and which process makes the call*, and nothing the user observes: the commit
//! dialog still opens showing "Generating commit message…", the editor is still
//! filled with an AI draft at that same moment, text the user typed still wins
//! over the draft, and a failure still degrades silently to manual entry with no
//! toast. Keeping generation on the daemon/server would have meant either
//! shipping credentials there or dropping the feature — both worse deviations
//! than relocating a call that produces the same result.
//!
//! ## Transport-agnostic entry point
//!
//! [`generate_commit_message_from_diff`] takes the diff as a plain string, so it
//! is indifferent to where the diff came from. [`generate_for_local_repo`] reads
//! it from the local working tree. The remote/SSH path will hand it the diff the
//! client already holds from diff-state-over-SSH; that wiring is deferred until
//! the remote git write-ops work (PR #125) lands, since the remote commit flow
//! it would hook into does not exist yet. No daemon RPC is involved either way —
//! the finished string goes into the existing commit request.

use std::path::Path;

use anyhow::{anyhow, bail, Result};
use warpui::AppContext;

use crate::ai::agent_providers::oneshot::{
    byop_oneshot_completion, resolve_active_ai_oneshot, OneshotConfig, OneshotOptions,
};
use crate::ai::agent_providers::prompt_renderer;
use crate::util::git::get_diff_for_commit_message;

/// Character cap on the user message. `get_diff_for_commit_message` already
/// truncates the diff to 16k chars, so this only has to leave headroom for the
/// wrapper text — it exists to stop the one-shot's 8k default from silently
/// halving a diff that was already sized for this call.
const MAX_USER_CHARS: usize = 20_000;

/// Low, but not zero: the draft must track the diff rather than free-associate,
/// while still being allowed to phrase the summary naturally.
const TEMPERATURE: f32 = 0.3;

/// Labels a model may prepend to its answer despite the prompt forbidding them.
/// Matched case-insensitively against the start of the reply.
const LABEL_PREFIXES: [&str; 3] = ["commit message:", "subject:", "message:"];

/// Resolves the BYOP model that drafts commit messages.
///
/// Reuses `resolve_active_ai_oneshot` — the shared resolver behind every other
/// app-side one-shot (prompt suggestions, NLD predict, relevant files) — which
/// reads the profile's `active_ai_model` and falls back to the base model. That
/// is what keeps this feature free of new picker UX: whatever model the user
/// already configured for lightweight assists drafts the commit message too.
///
/// `None` means BYOP is not configured, or the active model is not BYOP-coded.
/// Callers treat that as "no draft is coming" and leave the user to type.
pub fn resolve_config(app: &AppContext) -> Option<OneshotConfig> {
    // The git dialog is not owned by a terminal view, so there is no per-view
    // profile override to honour — conversation-title generation resolves with
    // `None` for the same reason.
    resolve_active_ai_oneshot(app, None)
}

/// Generates a commit message from diff content that the caller already holds.
///
/// Transport-agnostic on purpose: local repos pass the working-tree diff, and
/// the remote/SSH path will pass the synced diff-state content unchanged.
pub async fn generate_commit_message_from_diff(
    cfg: &OneshotConfig,
    diff: &str,
    branch_name: &str,
) -> Result<String> {
    // Mirrors Warp's `git_actions::generate_commit_message`: skip the model
    // round trip entirely when there is nothing to summarize.
    if diff.trim().is_empty() {
        bail!("no changes to generate a commit message from");
    }

    let system = prompt_renderer::commit_message_system_prompt();
    let user = build_user_prompt(diff, branch_name);
    let opts = OneshotOptions {
        max_chars: Some(MAX_USER_CHARS),
        temperature: Some(TEMPERATURE),
        ..Default::default()
    };
    let raw = byop_oneshot_completion(cfg, &system, &user, &opts).await?;

    // Warp errors out rather than committing an empty message; keep that, so an
    // unusable reply leaves the editor blank instead of committing whitespace.
    sanitize_commit_message(&raw).ok_or_else(|| anyhow!("AI returned an empty commit message"))
}

/// Local-repo entry point: read the working-tree diff for exactly the scope that
/// will be committed, then generate from it.
pub async fn generate_for_local_repo(
    cfg: &OneshotConfig,
    repo_path: &Path,
    include_unstaged: bool,
    branch_name: &str,
) -> Result<String> {
    let diff = get_diff_for_commit_message(repo_path, include_unstaged).await?;
    generate_commit_message_from_diff(cfg, &diff, branch_name).await
}

/// Wraps the diff for the model. The branch name is tagged separately so the
/// prompt's "context only" rule has something to point at, and the diff is
/// fenced in a tag so a diff containing prose can't read as an instruction.
fn build_user_prompt(diff: &str, branch_name: &str) -> String {
    format!(
        "Write the commit message for this change.\n<branch>{branch_name}</branch>\n<diff>\n{diff}\n</diff>"
    )
}

/// Cleans a raw model reply into a commit message, or `None` when nothing usable
/// is left.
///
/// Order:
/// 1. Strip `<think>` / `<reasoning>` blocks (reasoning models emit them first).
/// 2. Strip a wrapping markdown code fence.
/// 3. Strip a leading `Commit message:` / `Subject:` label.
/// 4. Strip matching quotes or backticks around the whole message.
/// 5. Trim; an empty result becomes `None`.
///
/// Deliberately does **not** collapse to the first line, unlike the conversation
/// title sanitizer: a commit message is legitimately multi-line, and dropping the
/// body would throw away most of what was generated.
fn sanitize_commit_message(raw: &str) -> Option<String> {
    let without_reasoning = strip_reasoning_blocks(raw);
    let unfenced = strip_code_fence(&without_reasoning);
    let unlabeled = strip_label_prefix(unfenced);
    let unquoted = strip_wrapping_quotes(unlabeled).trim();
    if unquoted.is_empty() {
        None
    } else {
        Some(unquoted.to_owned())
    }
}

/// Removes `<think>…</think>`-style blocks, including several in a row.
fn strip_reasoning_blocks(raw: &str) -> String {
    let mut out = raw.to_owned();
    for tag in ["think", "reasoning", "thought", "scratchpad"] {
        let open = format!("<{tag}>");
        let close = format!("</{tag}>");
        while let (Some(start), Some(end)) = (
            out.find(&open),
            out.find(&close).map(|idx| idx + close.len()),
        ) {
            if end <= start {
                break;
            }
            out.replace_range(start..end, "");
        }
    }
    out
}

/// Unwraps a reply the model wrapped in a ``` fence. A fence that is opened but
/// never closed still gets unwrapped, since the opening line is never message
/// text.
fn strip_code_fence(raw: &str) -> &str {
    let trimmed = raw.trim();
    if !trimmed.starts_with("```") {
        return trimmed;
    }
    let Some(first_newline) = trimmed.find('\n') else {
        // A lone fence line with no content after it.
        return "";
    };
    let body = &trimmed[first_newline + 1..];
    match body.rfind("```") {
        Some(close) => body[..close].trim(),
        None => body.trim(),
    }
}

/// Strips a leading `Commit message:`-style label. Uses `get` + ASCII-insensitive
/// compare rather than lowercasing first, so a non-ASCII leading character can't
/// produce an out-of-bounds slice.
fn strip_label_prefix(raw: &str) -> &str {
    for prefix in LABEL_PREFIXES {
        if let Some(head) = raw.get(..prefix.len()) {
            if head.eq_ignore_ascii_case(prefix) {
                return raw[prefix.len()..].trim_start();
            }
        }
    }
    raw
}

/// Strips one pair of matching quotes/backticks wrapping the whole message.
fn strip_wrapping_quotes(raw: &str) -> &str {
    let pairs = [
        ('"', '"'),
        ('\'', '\''),
        ('`', '`'),
        ('\u{201c}', '\u{201d}'),
        ('\u{300c}', '\u{300d}'),
    ];
    let mut chars = raw.chars();
    let (Some(first), Some(last)) = (chars.next(), chars.next_back()) else {
        // Fewer than two characters — there is no pair to strip.
        return raw;
    };
    for (open, close) in pairs {
        if first == open && last == close {
            return raw[open.len_utf8()..raw.len() - close.len_utf8()].trim();
        }
    }
    raw
}

#[cfg(test)]
#[path = "commit_message_gen_tests.rs"]
mod tests;
