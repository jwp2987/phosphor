//! Background monitor that scans the running harness block for known runtime
//! failure substrings (e.g. invalid API key, exhausted credits) and reports
//! the first hit so the driver can fail the task fast instead of letting the
//! harness hang.
//!
//! Ported from the pinned oracle (`02b53fcd8`, Warp `2026.07.29.09.05` stable
//! — see `ORACLE.md`) verbatim: the module is fully local (no cloud/server
//! imports), scanning terminal block output already captured by
//! `TerminalDriver` with the same `RegexDFAs` machinery the find feature
//! uses (`app/src/terminal/model/find.rs`). See #289.
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use futures::FutureExt as _;

use regex::escape;
use warpui::ModelSpawner;

use super::AgentDriver;
use super::terminal::BlockOutputMatch;
use crate::terminal::cli_agent_sessions::CLIAgentSessionStatus;
use crate::terminal::model::block::BlockId;
use crate::terminal::model::find::RegexDFAs;

/// How long the harness block must stay unchanged after a pattern hit before the hit is
/// believed (a retrying harness repaints a spinner; a dead one does not). Time itself is the
/// requirement here: "stopped producing output" cannot be an event. It is only ever started by
/// a pattern hit, never polled.
const STALL_POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Give-up for a single confirmation: if the block keeps changing this long after a hit, the
/// hit is dropped (logged) and scanning resumes on the next output event.
const STALL_CONFIRMATION_BUDGET: Duration = Duration::from_secs(60);

/// Minimum gap between two scans of the block. Output events are coalesced into one pending
/// signal, so a flood of output costs at most one scan per gap and the last output is always
/// scanned (trailing edge). Pacing for the main-thread grid scan, not a correctness delay.
const MIN_SCAN_GAP: Duration = Duration::from_millis(250);

#[derive(Debug, Clone)]
pub(crate) struct DetectedHarnessError {
    pub pattern: String,
    pub excerpt: String,
}

/// Build a combined case-insensitive DFA from the harness's static patterns.
pub(crate) fn build_dfas(patterns: &[&'static str]) -> Option<RegexDFAs> {
    if patterns.is_empty() {
        return None;
    }
    let escaped: Vec<String> = patterns.iter().map(|p| escape(p)).collect();
    let refs: Vec<&str> = escaped.iter().map(String::as_str).collect();
    match RegexDFAs::new_many(
        &refs, false, // enable_unicode_word_boundary
        false, // case_sensitive
    ) {
        Ok(dfas) => Some(dfas),
        Err(err) => {
            log::warn!("Failed to build harness output DFAs: {err}");
            None
        }
    }
}

/// Map a `matched_text` produced by the combined DFA back to the originating
/// needle. The DFA matched `(p1|p2|…)`, so the matched substring is exactly
/// one of `patterns` up to case — we lowercase-compare to identify it.
pub(crate) fn pattern_for_match(
    matched_text: &str,
    patterns: &[&'static str],
) -> Option<&'static str> {
    let matched_lower = matched_text.to_lowercase();
    patterns
        .iter()
        .copied()
        .find(|p| p.to_lowercase() == matched_lower)
}

fn outputs_stalled(before: Option<&str>, after: Option<&str>) -> bool {
    matches!((before, after), (Some(a), Some(b)) if a == b)
}

async fn find_match_once(
    block_id: &BlockId,
    dfas: &Arc<RegexDFAs>,
    foreground: &ModelSpawner<AgentDriver>,
) -> Option<BlockOutputMatch> {
    let block_id_for_tick = block_id.clone();
    let dfas_for_tick = Arc::clone(dfas);
    foreground
        .spawn(move |me, ctx| {
            me.terminal_driver
                .as_ref(ctx)
                .find_first_match_in_block_output(&block_id_for_tick, &dfas_for_tick, ctx)
        })
        .await
        .ok()
        .flatten()
}

async fn fetch_plaintext(
    block_id: &BlockId,
    foreground: &ModelSpawner<AgentDriver>,
) -> Option<String> {
    let block_id_for_tick = block_id.clone();
    foreground
        .spawn(move |me, ctx| {
            me.terminal_driver
                .as_ref(ctx)
                .block_output_plaintext(&block_id_for_tick, ctx)
        })
        .await
        .ok()
        .flatten()
}

/// Subscribe to the terminal's output events (one coalesced signal per burst of output).
async fn subscribe_output(
    foreground: &ModelSpawner<AgentDriver>,
) -> Option<async_channel::Receiver<()>> {
    foreground
        .spawn(|me, ctx| {
            let view = me.terminal_driver.as_ref(ctx).terminal_view().clone();
            view.update(ctx, |terminal, _| terminal.subscribe_output())
        })
        .await
        .ok()
}

#[derive(Debug, PartialEq, Eq)]
enum Quiet {
    /// Unchanged for the whole quiet window.
    Stalled,
    /// Still changing when the budget ran out.
    Moving,
    /// The output signal closed (terminal gone).
    Closed,
}

/// After a pattern hit, wait for the block to go quiet. Returns [`Quiet::Stalled`] when the block
/// stayed unchanged for the quiet window (the harness really is stuck on the error),
/// [`Quiet::Moving`] when it kept changing for the whole budget (a retry in progress). The window restarts only when an output event shows the block actually
/// changed; events that leave the block identical do not extend it.
async fn confirm_quiet<S, SF>(
    output_rx: &async_channel::Receiver<()>,
    mut snapshot: S,
    quiet: Duration,
    budget: Duration,
) -> Quiet
where
    S: FnMut() -> SF,
    SF: Future<Output = Option<String>>,
{
    let mut previous = snapshot().await;
    let mut quiet_timer = warpui::r#async::Timer::after(quiet).fuse();
    let mut budget_timer = warpui::r#async::Timer::after(budget).fuse();
    loop {
        futures::select! {
            _ = quiet_timer => {
                // Last look: a change that raced the timer must still count.
                let current = snapshot().await;
                if outputs_stalled(previous.as_deref(), current.as_deref()) {
                    return Quiet::Stalled;
                }
                previous = current;
                quiet_timer = warpui::r#async::Timer::after(quiet).fuse();
            }
            _ = budget_timer => {
                log::info!(
                    "Harness output kept changing for {budget:?} after an error pattern hit; \
                     treating it as a retry, not a failure"
                );
                return Quiet::Moving;
            }
            event = output_rx.recv().fuse() => {
                if event.is_err() {
                    return Quiet::Closed;
                }
                let current = snapshot().await;
                if !outputs_stalled(previous.as_deref(), current.as_deref()) {
                    // The block changed: it is still moving. Restart the quiet window.
                    previous = current;
                    quiet_timer = warpui::r#async::Timer::after(quiet).fuse();
                }
            }
        }
    }
}

/// The scan loop, parameterised over how the block is probed so it can be tested without a
/// running app. Scans once up front (output may already be there), then once per output event
/// (paced by `min_gap`), until a hit is confirmed quiet or the signal closes.
async fn watch_with<F, FF, S, SF>(
    output_rx: async_channel::Receiver<()>,
    mut find: F,
    mut snapshot: S,
    quiet: Duration,
    budget: Duration,
    min_gap: Duration,
) -> Option<BlockOutputMatch>
where
    F: FnMut() -> FF,
    FF: Future<Output = Option<BlockOutputMatch>>,
    S: FnMut() -> SF,
    SF: Future<Output = Option<String>>,
{
    let mut first = true;
    loop {
        if !first {
            output_rx.recv().await.ok()?;
        }
        first = false;
        if find().await.is_some() {
            match confirm_quiet(&output_rx, &mut snapshot, quiet, budget).await {
                Quiet::Stalled => {
                    // Re-run the DFA so we report the post-dwell match in case the row
                    // positions shifted while we waited.
                    if let Some(hit) = find().await {
                        return Some(hit);
                    }
                }
                // The confirmation consumed the output signal while the block kept changing;
                // judge the block as it is now rather than waiting for output that may never
                // come.
                Quiet::Moving => first = true,
                Quiet::Closed => return None,
            }
        }
        if !min_gap.is_zero() {
            warpui::r#async::Timer::after(min_gap).await;
        }
    }
}

/// Watch the given block for harness output errors, driven by the terminal's output events
/// (#1381, APP-13): an error is seen when it is printed, not at the next 5-15 s sample, and
/// the watch no longer gives up after a fixed observation window.
///
/// On every pattern hit, wait for the block to go quiet (see [`confirm_quiet`]) and only
/// resolve with `Some(DetectedHarnessError)` when the harness output stabilizes with the
/// pattern still present. If the harness keeps producing output (e.g. spinner frames during
/// an automatic retry), the hit is dropped and the watch resumes on the next output event.
///
/// Returns `None` when `patterns` is empty, DFA construction fails, or the terminal goes
/// away. Cancellation-safe: dropping the future stops the watch.
pub(crate) async fn watch_block_for_errors(
    block_id: BlockId,
    patterns: &'static [&'static str],
    foreground: &ModelSpawner<AgentDriver>,
) -> Option<DetectedHarnessError> {
    if patterns.is_empty() {
        return None;
    }
    let dfas = Arc::new(build_dfas(patterns)?);
    let output_rx = subscribe_output(foreground).await?;

    let hit = watch_with(
        output_rx,
        || find_match_once(&block_id, &dfas, foreground),
        || fetch_plaintext(&block_id, foreground),
        STALL_POLL_INTERVAL,
        STALL_CONFIRMATION_BUDGET,
        MIN_SCAN_GAP,
    )
    .await?;

    // The DFA matched one of our needles, so `pattern_for_match`
    // should always resolve. Fall back to the matched substring
    // verbatim if it somehow doesn't.
    let pattern = pattern_for_match(&hit.matched_text, patterns)
        .map(str::to_owned)
        .unwrap_or_else(|| hit.matched_text.clone());
    Some(DetectedHarnessError {
        pattern,
        excerpt: cap_excerpt(&hit.excerpt),
    })
}

pub(crate) fn should_suppress_runtime_failure(status: Option<&CLIAgentSessionStatus>) -> bool {
    // On CLIAgentSessionStatus::Failed the session model has already reported the
    // failure, so runtime pattern matching would only duplicate it.
    //
    // NOTE: the previous comment here claimed `Failed` was cloud-only and absent
    // from this fork. Both halves were wrong -- it arrives over OSC 777, which is
    // local, and the variant was simply unported (#582). Restored to the pin's set.
    matches!(
        status,
        Some(CLIAgentSessionStatus::Success) | Some(CLIAgentSessionStatus::Failed { .. })
    )
}

/// Cap excerpt length so we don't blow up status messages or logs with
/// terminal-width rows.
const EXCERPT_MAX_LEN: usize = 240;
fn cap_excerpt(excerpt: &str) -> String {
    if excerpt.chars().count() <= EXCERPT_MAX_LEN {
        return excerpt.to_owned();
    }
    let mut out: String = excerpt.chars().take(EXCERPT_MAX_LEN).collect();
    out.push('…');
    out
}

#[cfg(test)]
#[path = "harness_output_monitor_tests.rs"]
mod tests;
