//! All-or-nothing file edits for a local session (#1277).
//!
//! Matching is already all-or-nothing: `apply_edits` returns `Err` for the whole batch if any
//! diff fails, before anything is written. The write phase is per file (each `CodeDiffView`
//! file saves independently), so a failure part-way through used to leave the earlier files
//! changed. The executor keeps each file's pre-edit contents in memory ([`PreEdit`]); when some
//! files landed and others did not, [`plan_rollback`] / [`run_rollback`] put the landed files
//! back, each through `write_atomic`.

use std::path::PathBuf;

use ai::diff_validation::{AIRequestedCodeDiff, DiffType};

/// What a file held before the batch, kept in memory for rollback.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PreEdit {
    /// The resolved path the edit was registered under.
    pub path: String,
    /// The path the file moves to, for a rename.
    pub rename_to: Option<String>,
    /// Whether the edit creates the file (it did not exist).
    pub created: bool,
    /// The contents before the edit (empty for a creation).
    pub original: String,
}

impl PreEdit {
    pub(super) fn from_applied(diff: &AIRequestedCodeDiff, resolved_path: String) -> Self {
        let (created, rename_to) = match &diff.diff_type {
            DiffType::Create { .. } => (true, None),
            DiffType::Update { rename, .. } => (
                false,
                rename.as_ref().map(|p| p.to_string_lossy().into_owned()),
            ),
            DiffType::Delete { .. } => (false, None),
        };
        Self {
            path: resolved_path,
            rename_to,
            created,
            original: diff.original_content.clone(),
        }
    }
}

/// One step that undoes a landed write.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum RollbackOp {
    /// Write `content` back at `path` (an edited, deleted or renamed-away file).
    Restore { path: String, content: String },
    /// Remove a file the batch created or renamed to.
    Remove { path: String },
}

/// The undo steps for every file in `pre_edits` that reached disk: `updated` are the paths the
/// save reported written, `deleted` the paths it reported removed.
pub(super) fn plan_rollback(
    pre_edits: &[PreEdit],
    updated: &[String],
    deleted: &[String],
) -> Vec<RollbackOp> {
    let mut ops = Vec::new();
    for pre in pre_edits {
        let dest_written = pre
            .rename_to
            .as_ref()
            .is_some_and(|to| updated.contains(to));
        let landed = updated.contains(&pre.path) || deleted.contains(&pre.path) || dest_written;
        if !landed {
            continue;
        }
        if pre.created {
            ops.push(RollbackOp::Remove {
                path: pre.path.clone(),
            });
            continue;
        }
        ops.push(RollbackOp::Restore {
            path: pre.path.clone(),
            content: pre.original.clone(),
        });
        if let Some(to) = &pre.rename_to
            && dest_written
        {
            ops.push(RollbackOp::Remove { path: to.clone() });
        }
    }
    ops
}

/// What running a rollback achieved.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct RollbackReport {
    /// Paths put back (or removed).
    pub restored: Vec<String>,
    /// Paths that could not be rolled back, with the reason.
    pub failed: Vec<(String, String)>,
}

pub(super) async fn run_rollback(ops: Vec<RollbackOp>) -> RollbackReport {
    let mut report = RollbackReport::default();
    for op in ops {
        let (path, result) = match op {
            RollbackOp::Restore { path, content } => {
                let result = warp_files::atomic_write::write_atomic(
                    &PathBuf::from(&path),
                    content.as_bytes(),
                )
                .await;
                (path, result)
            }
            RollbackOp::Remove { path } => {
                let result = match async_fs::remove_file(&path).await {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    other => other,
                };
                (path, result)
            }
        };
        match result {
            Ok(()) => report.restored.push(path),
            Err(err) => report.failed.push((path, err.to_string())),
        }
    }
    report
}

/// The message reported to the model after a rollback was attempted. `reasons` is the rendered
/// save failure(s).
pub(super) fn rollback_message(
    reasons: &str,
    applied: &[String],
    report: &RollbackReport,
) -> String {
    if report.failed.is_empty() {
        format!(
            "No changes were applied: the batch is all-or-nothing and it failed.\n{reasons}\n\
             Files written before the failure were restored to their previous contents: {}.",
            report.restored.join(", ")
        )
    } else {
        let failed = report
            .failed
            .iter()
            .map(|(path, why)| format!("{path} ({why})"))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "This edit was applied partially and could not be fully rolled back.\n{reasons}\n\
             Restored: {}.\nStill holding the edit, do NOT apply it again: {failed}.\n\
             Applied before the failure: {}.",
            report.restored.join(", "),
            applied.join(", ")
        )
    }
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
