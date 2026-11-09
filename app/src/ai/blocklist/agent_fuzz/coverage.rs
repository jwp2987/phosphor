//! Which cells the run exercised: (tool x op x timing) for the user's steps and (chaos x phase)
//! for the model's. A floor on the Wave 0 cells fails the corpus replay if a refactor of the
//! actor or the player silently stops reaching them, because a fuzzer that no longer drives the
//! #1797 / #1799 paths would go green while testing nothing.

use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use super::runner::CaseReport;

#[derive(Default, Debug)]
pub struct Coverage {
    pub cells: BTreeMap<String, u64>,
}

static GLOBAL: Mutex<Coverage> = Mutex::new(Coverage {
    cells: BTreeMap::new(),
});

fn global() -> MutexGuard<'static, Coverage> {
    GLOBAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Phase of a chaos turn: the first request of the run, or a later one (a retry or a follow-up).
fn phase(request_index: usize) -> &'static str {
    if request_index == 0 { "first" } else { "later" }
}

pub fn record(report: &CaseReport) {
    let mut coverage = global();
    for step in report.steps.iter().filter(|s| s.performed) {
        let cell = format!(
            "step {}/{}/{}",
            step.tool.unwrap_or("-"),
            step.op,
            step.timing
        );
        *coverage.cells.entry(cell).or_default() += 1;
    }
    for (kind, request_index) in &report.chaos {
        *coverage
            .cells
            .entry(format!("chaos {kind}/{}", phase(*request_index)))
            .or_default() += 1;
    }
}

pub fn hits(cell: &str) -> u64 {
    global().cells.get(cell).copied().unwrap_or(0)
}

/// The table, for the end of a run (`AGENT_FUZZ_COVERAGE=1` prints it).
pub fn table() -> String {
    global()
        .cells
        .iter()
        .map(|(cell, n)| format!("{n:6}  {cell}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Cells the corpus must reach on its own: the user acts on a blocked command, a blocked
/// question, and a Ctrl-C lands on a running command. (The #1796 scenario has no step of its own
/// kind: it is the same approve-on-a-command cell with a silent, prompt-shaped command.)
pub const WAVE0_FLOOR: &[&str] = &[
    "step run_shell_command/reject/on_card",
    "step run_shell_command/approve/on_card",
    "step ask_user_question/answer/on_card",
    "step -/ctrl_c/fast",
    "step run_shell_command/follow_up/on_card",
];
