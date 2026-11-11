//! The agent context over the shell backend (Wave H parity, P8, family `h`): the host's skills
//! and global rule files read with ordinary commands and published as the same
//! `RemoteAgentContextSnapshot` the helper pushes, so the app's reconciliation is one code path.
//!
//! The commands, the caps and the assembly are [`crate::shellcmd::agent_context`]. There is no
//! watcher in shell mode: a read happens at session start, on an explicit refresh, and nowhere
//! else (no timer). A read that finds the same context as the last one queues nothing; a changed
//! one bumps the revision (kept per host, so a new session on the host never goes backwards) and
//! is queued as a [`ShellPush::AgentContext`] for the caller to drain.
//!
//! The old server published home skills and global rules only, so there is nothing to refresh
//! when the cwd moves into another project; project skills travel with the file tree
//! (`standing_results`).

use std::sync::Mutex;

use super::{ShellBackend, ShellPush};
use crate::backend::{BackendError, Family};
use crate::proto::RemoteAgentContextSnapshot;
use crate::shellcmd::agent_context as spec;
use crate::shellcmd::{Script, StderrMode};

/// The last context a host's shell backend published and the revision it carried.
#[derive(Default)]
pub struct AgentContextCache {
    last: Mutex<Option<RemoteAgentContextSnapshot>>,
    revision: Mutex<u64>,
}

impl AgentContextCache {
    /// Forgets the context, e.g. when the host disconnects.
    pub fn clear(&self) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

impl ShellBackend {
    /// Reads the host's agent context and queues it as a push when it differs from the last one
    /// read. Returns whether it did. `Unsupported` on a PowerShell host.
    pub async fn refresh_agent_context(&self) -> Result<bool, BackendError> {
        let kind = self.shell_kind();
        let (Some(home), Some(skills), Some(rules)) = (
            spec::home_command(kind),
            spec::skills_command(kind),
            spec::rules_command(kind),
        ) else {
            return Err(self.not_implemented(Family::AgentContext, "agent_context"));
        };
        let script = Script::new(Self::new_nonce())?
            .section("home", home, StderrMode::Discard)?
            .section("skills", skills, StderrMode::Discard)?
            .section("rules", rules, StderrMode::Discard)?;
        let frame = self.run_script(&script).await?;
        let home_dir = frame
            .section("home")
            .filter(|s| s.success())
            .map(|s| s.text())
            .filter(|h| !h.is_empty())
            .ok_or_else(|| {
                BackendError::Unavailable("the host reports no home directory".into())
            })?;
        let read = |name: &str| {
            frame
                .section(name)
                .filter(|s| {
                    if !s.success() {
                        // Losing a whole section silently hid the host's skills (#2152). Stderr
                        // is discarded to keep the records unframed-noise free, so the exit
                        // status and any stdout are what there is to report.
                        log::warn!(
                            "agent context: the `{name}` section exited {} on the host and was dropped (output: {:?})",
                            s.exit,
                            s.text().chars().take(200).collect::<String>()
                        );
                    }
                    s.success()
                })
                .map(spec::parse_records)
                .unwrap_or_default()
        };
        let mut snapshot = spec::assemble(&home_dir, read("skills"), read("rules"), 0);

        let mut last = self
            .agent_cache
            .last
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if last
            .as_ref()
            .is_some_and(|previous| spec::same_context(previous, &snapshot))
        {
            return Ok(false);
        }
        let mut revision = self
            .agent_cache
            .revision
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *revision += 1;
        snapshot.revision = *revision;
        *last = Some(snapshot.clone());
        drop(last);
        drop(revision);
        self.push(ShellPush::AgentContext(snapshot));
        Ok(true)
    }
}
