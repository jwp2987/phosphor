//! What an integration run keeps so a failure can be judged afterwards (#2122).
//!
//! Three things, all plain text on the scenario's own stderr (the per-scenario log the test
//! shim in `crates/integration/tests/common` keeps as a CI artifact when the scenario fails):
//!
//! * `integ_event seq=N ...`: every event the driver injects, numbered in injection order, with
//!   whether the app handled it. Typed text is given by its LENGTH, never its characters (#2040);
//!   a named key is given by name.
//! * `integ_failure_state ...`: what the app showed when a step failed (see
//!   [`super::Builder::with_failure_dump`]).
//! * `INTEG-SKIP <scenario> NEEDS: <reason>`: a scenario whose `set_should_run_test` predicate
//!   said no. The process still exits 0 (nextest and the usage suite have no "skipped at run
//!   time" result), so this line and the marker file next to the logs are what stop a scenario
//!   that never ran from being counted as one that passed: `script/ci/integ_evidence.py` lists
//!   and counts them at the end of the job.

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::event::Event;

/// Directory the integration run keeps its evidence in. Set by the CI job; unset, nothing is
/// written to disk and the lines only go to stderr.
pub const EVIDENCE_DIR_ENV_VAR: &str = "WARP_INTEGRATION_EVIDENCE_DIR";

/// First word of the line a skipped scenario prints. The CI summary counts lines that start
/// with it.
pub const SKIP_MARKER: &str = "INTEG-SKIP";

/// The reason given when a predicate said no without naming one.
pub const UNNAMED_SKIP_REASON: &str =
    "its set_should_run_test predicate said no (platform, shell or feature gate; no reason given)";

static SKIP_REASON: Mutex<Option<String>> = Mutex::new(None);
static EVENT_SEQ: AtomicU64 = AtomicU64::new(0);

/// Records why the scenario will not run and returns `false`, so a `set_should_run_test`
/// predicate can end with `return skip_because("sshd is not installed")`. The first reason of a
/// process wins: it is the one that decided.
pub fn skip_because(reason: impl Into<String>) -> bool {
    let mut slot = SKIP_REASON.lock().unwrap_or_else(|err| err.into_inner());
    if slot.is_none() {
        *slot = Some(reason.into());
    }
    false
}

/// `condition`, recording `reason` when it is false. For a predicate made of several gates:
/// `needs(!flag_on, "the tmux ssh wrapper off") && needs(has_sshd, "sshd")`.
pub fn needs(condition: bool, reason: &str) -> bool {
    condition || skip_because(reason)
}

fn take_skip_reason() -> Option<String> {
    SKIP_REASON
        .lock()
        .unwrap_or_else(|err| err.into_inner())
        .take()
}

/// One line, no line breaks, whatever the reason holds.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The line a skipped scenario prints: `INTEG-SKIP <scenario> NEEDS: <reason>`.
pub fn skip_line(test_name: &str, reason: Option<&str>) -> String {
    format!(
        "{SKIP_MARKER} {test_name} NEEDS: {}",
        one_line(reason.unwrap_or(UNNAMED_SKIP_REASON))
    )
}

/// Where a scenario's skip marker goes, when the run keeps evidence.
fn skip_marker_path(test_name: &str) -> Option<PathBuf> {
    let dir = std::env::var_os(EVIDENCE_DIR_ENV_VAR).filter(|dir| !dir.is_empty())?;
    Some(
        PathBuf::from(dir)
            .join("skips")
            .join(format!("{test_name}.skip")),
    )
}

/// Says that `test_name` did not run: the line on stderr, and the marker file when the run
/// keeps evidence. Called by the driver when the scenario's predicate said no.
pub(super) fn report_skip(test_name: &str) {
    let line = skip_line(test_name, take_skip_reason().as_deref());
    eprintln!("{line}");
    let Some(path) = skip_marker_path(test_name) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(err) = std::fs::write(&path, format!("{line}\n")) {
        log::warn!("could not write the skip marker {}: {err}", path.display());
    }
}

/// What the evidence line says about one event. No typed text: its length (#2040).
pub fn describe_event(event: &Event) -> String {
    match event {
        Event::KeyDown {
            keystroke,
            chars,
            is_composing,
            ..
        } => format!(
            "kind=KeyDown key={} ctrl={} alt={} shift={} cmd={} chars_len={} composing={}",
            // A key that types one character is that character: say only that it is one.
            if keystroke.key.chars().count() == 1 {
                "<char>"
            } else {
                keystroke.key.as_str()
            },
            keystroke.ctrl,
            keystroke.alt,
            keystroke.shift,
            keystroke.cmd,
            chars.chars().count(),
            is_composing
        ),
        Event::TypedCharacters { chars } => format!(
            "kind=TypedCharacters chars_len={} lines={}",
            chars.chars().count(),
            chars.matches(['\n', '\r']).count()
        ),
        Event::MouseMoved {
            position,
            is_synthetic,
            ..
        } => format!(
            "kind=MouseMoved x={:.1} y={:.1} synthetic={is_synthetic}",
            position.x(),
            position.y()
        ),
        Event::LeftMouseDown {
            position,
            click_count,
            ..
        } => format!(
            "kind=LeftMouseDown x={:.1} y={:.1} clicks={click_count}",
            position.x(),
            position.y()
        ),
        Event::LeftMouseUp { position, .. } => format!(
            "kind=LeftMouseUp x={:.1} y={:.1}",
            position.x(),
            position.y()
        ),
        Event::LeftMouseDragged { position, .. } => format!(
            "kind=LeftMouseDragged x={:.1} y={:.1}",
            position.x(),
            position.y()
        ),
        Event::RightMouseDown { position, .. } => format!(
            "kind=RightMouseDown x={:.1} y={:.1}",
            position.x(),
            position.y()
        ),
        Event::ScrollWheel {
            position, delta, ..
        } => format!(
            "kind=ScrollWheel x={:.1} y={:.1} dx={:.1} dy={:.1}",
            position.x(),
            position.y(),
            delta.x(),
            delta.y()
        ),
        Event::SetMarkedText { marked_text, .. } => format!(
            "kind=SetMarkedText chars_len={}",
            marked_text.chars().count()
        ),
        Event::DragAndDropFiles { paths, .. } => {
            format!("kind=DragAndDropFiles paths={}", paths.len())
        }
        other => format!("kind={:?}", crate::event::EventDiscriminants::from(other)),
    }
}

/// The numbered evidence line of one injected event. `position_id` is the saved position a
/// click or hover was aimed at; `handled` is what the app's event callback answered.
pub(super) fn injected_event_line(
    seq: u64,
    step: &str,
    event: &Event,
    position_id: Option<&str>,
    handled: bool,
) -> String {
    let position = position_id
        .map(|id| format!(" position_id={}", one_line(id)))
        .unwrap_or_default();
    format!(
        "integ_event seq={seq} step={:?} {}{position} handled={handled}",
        one_line(step),
        describe_event(event)
    )
}

/// Logs one injected event with the next sequence number of this process.
pub(super) fn log_injected_event(
    step: &str,
    event: &Event,
    position_id: Option<&str>,
    handled: bool,
) {
    let seq = EVENT_SEQ.fetch_add(1, Ordering::Relaxed) + 1;
    log::info!(
        "{}",
        injected_event_line(seq, step, event, position_id, handled)
    );
}

/// The failure-state line: one line, bounded, so a scenario that fails with a huge block on
/// screen does not flood its log.
pub(super) fn failure_state_line(step: &str, state: &str) -> String {
    const MAX_CHARS: usize = 6000;
    let escaped = state.escape_debug().to_string();
    let total = escaped.chars().count();
    let shown: String = if total > MAX_CHARS {
        // The end is what the failing assertion looked at.
        let tail: String = escaped.chars().skip(total - MAX_CHARS).collect();
        format!("...[{} chars cut]{tail}", total - MAX_CHARS)
    } else {
        escaped
    };
    format!("integ_failure_state step={:?} {shown}", one_line(step))
}

#[cfg(test)]
#[path = "evidence_tests.rs"]
mod tests;
