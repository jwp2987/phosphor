//! Background task for async find operations.
//!
//! This module contains the logic that runs on a background thread to scan
//! terminal blocks for matches without blocking the main thread. The task
//! pulls work items from a shared [`FindWorkQueue`] and streams results
//! back via an `async_channel`.

use std::ops::RangeInclusive;
use std::sync::Arc;
use std::time::Duration;

use crate::terminal::model::grid::Dimensions;
use futures_lite::future::yield_now;
use instant::Instant;
use parking_lot::FairMutex;
use warpui::{Entity, ModelContext};

use super::work_queue::{FindWorkItem, FindWorkQueue};
use super::{AbsoluteMatch, AsyncFindConfig, FindTaskMessage};
use crate::terminal::block_list_element::GridType;
use crate::terminal::model::TerminalModel;
use crate::terminal::model::find::{FindConfig, RegexDFAs};
use crate::terminal::model::grid::grid_handler::GridHandler;
use crate::terminal::model::index::Point;
use crate::terminal::model::terminal_model::BlockIndex;

/// Longest the terminal model lock is meant to be held for one scan slice. The UI thread takes
/// the same lock to paint and to apply results, so a slice that runs long stalls the frame behind
/// it (#1327: 1000-row slices of a many-match scan took tens of milliseconds each).
const TARGET_LOCK_HOLD: Duration = Duration::from_millis(2);

/// Rows scanned in the first slice of a grid; later slices adapt to [`TARGET_LOCK_HOLD`].
const INITIAL_ROWS_PER_SLICE: usize = 64;

/// Smallest slice the adaptation shrinks to.
const MIN_ROWS_PER_SLICE: usize = 4;

/// Number of rows to accumulate before results are sent to the main thread. Slices are small to
/// keep the lock hold short; the results of several are batched so the main thread handles a few
/// large messages rather than many tiny ones.
const ROWS_PER_CHUNK: usize = 1000;

/// Longest a batch of finished slices waits before being sent, so a many-match scan still shows
/// progress.
const MAX_BATCH_AGE: Duration = Duration::from_millis(20);

/// Picks the size of the next scan slice from how long the last one held the lock.
pub(super) fn next_slice_rows(current: usize, lock_held: Duration) -> usize {
    if lock_held > TARGET_LOCK_HOLD {
        (current / 2).max(MIN_ROWS_PER_SLICE)
    } else if lock_held < TARGET_LOCK_HOLD / 4 {
        (current * 2).min(ROWS_PER_CHUNK)
    } else {
        current
    }
}

/// Spawns a background find task that pulls work from the given queue.
///
/// Returns a handle that can be used to abort the spawned future.
pub fn spawn_find_task<E: Entity>(
    config: AsyncFindConfig,
    terminal_model: Arc<FairMutex<TerminalModel>>,
    queue: FindWorkQueue,
    result_tx: async_channel::Sender<FindTaskMessage>,
    collect_initial_scan: bool,
    ctx: &mut ModelContext<E>,
) -> warpui::r#async::SpawnedFutureHandle {
    ctx.spawn(
        async move {
            if collect_initial_scan {
                // Collect the blocks to scan here, off the UI thread (#1320), and tell the main
                // thread about them before any result for those blocks can arrive.
                let block_info = {
                    let mut model = terminal_model.lock();
                    super::collect_block_info_for_full_scan(&mut model, &config)
                };
                let _ = result_tx
                    .send(FindTaskMessage::BlocksCollected(block_info.clone()))
                    .await;
                queue.enqueue_full_scan(&block_info);
            }
            run_find_task_loop(config, terminal_model, queue, result_tx).await;
        },
        |_me, (), _ctx| {
            // Task completed — nothing to do here as results are sent via channel.
        },
    )
}

/// Runs the main find task loop, pulling work items from the queue.
async fn run_find_task_loop(
    config: AsyncFindConfig,
    terminal_model: Arc<FairMutex<TerminalModel>>,
    queue: FindWorkQueue,
    result_tx: async_channel::Sender<FindTaskMessage>,
) {
    // Build RegexDFAs from config.
    let Ok(dfas) = RegexDFAs::new_with_config(
        config.query.as_str(),
        FindConfig {
            is_regex_enabled: config.is_regex_enabled,
            is_case_sensitive: config.is_case_sensitive,
        },
    ) else {
        // Invalid regex — signal completion with no matches.
        let _ = result_tx.send(FindTaskMessage::Done).await;
        return;
    };

    while let Ok((item, queue_drained)) = queue.pop().await {
        match item {
            FindWorkItem::FullBlock { block_index } => {
                scan_terminal_block_chunked(
                    block_index,
                    &terminal_model,
                    &dfas,
                    &result_tx,
                    config.block_sort_direction,
                )
                .await;
            }
            FindWorkItem::DirtyRange {
                block_index,
                grid_type,
                row_range,
                num_lines_truncated,
            } => {
                scan_grid_chunked(
                    block_index,
                    grid_type,
                    *row_range.start(),
                    Some(*row_range.end() + 1),
                    ScanResultMode::DirtyRange {
                        num_lines_truncated,
                    },
                    &terminal_model,
                    &dfas,
                    &result_tx,
                )
                .await;
            }
            FindWorkItem::AIBlock {
                view_id,
                total_index,
            } => {
                // Forward to main thread for execution.
                let _ = result_tx
                    .send(FindTaskMessage::ScanAIBlock {
                        view_id,
                        total_index,
                    })
                    .await;
            }
        }

        // The emptiness flag is checked atomically with the pop inside
        // the queue lock, avoiding the TOCTOU race of a separate
        // `is_empty()` call.
        if queue_drained {
            let _ = result_tx.send(FindTaskMessage::Done).await;
        }
    }
}

/// Scans a terminal block in chunks, streaming results back to the main thread.
async fn scan_terminal_block_chunked(
    block_index: BlockIndex,
    terminal_model: &Arc<FairMutex<TerminalModel>>,
    dfas: &RegexDFAs,
    result_tx: &async_channel::Sender<FindTaskMessage>,
    block_sort_direction: crate::terminal::model::terminal_model::BlockSortDirection,
) {
    // Determine grid order based on sort direction.
    let grid_order = match block_sort_direction {
        crate::terminal::model::terminal_model::BlockSortDirection::MostRecentFirst => {
            &[GridType::PromptAndCommand, GridType::Output]
        }
        crate::terminal::model::terminal_model::BlockSortDirection::MostRecentLast => {
            &[GridType::Output, GridType::PromptAndCommand]
        }
    };

    for &grid_type in grid_order {
        scan_grid_chunked(
            block_index,
            grid_type,
            0,
            None,
            ScanResultMode::FullBlock,
            terminal_model,
            dfas,
            result_tx,
        )
        .await;
    }
}

/// Controls how each chunk's matches are sent to the main thread.
enum ScanResultMode {
    /// Send [`FindTaskMessage::BlockGridMatches`] per chunk. Empty chunks are
    /// skipped (no message sent).
    FullBlock,
    /// Send [`FindTaskMessage::DirtyRangeMatches`] per chunk, converting the
    /// scanned row range to absolute coordinates using the provided truncation
    /// offset. Messages are always sent, even for empty chunks, so that old
    /// matches in the sub-range are cleared.
    DirtyRange { num_lines_truncated: u64 },
}

/// Scans a range of rows within a single grid in chunks, releasing the
/// terminal model lock between chunks to avoid blocking the main thread.
///
/// Both full-block scanning and dirty-range scanning delegate to this
/// function; the [`ScanResultMode`] determines the message type sent per
/// chunk.
///
/// # Arguments
/// * `start_row` — First row to scan (inclusive).
/// * `end_row` — Upper bound on rows to scan (exclusive). `None` scans to
///   the end of the grid.
/// * `mode` — Determines the message type sent per chunk.
#[allow(clippy::too_many_arguments)]
async fn scan_grid_chunked(
    block_index: BlockIndex,
    grid_type: GridType,
    start_row: usize,
    end_row: Option<usize>,
    mode: ScanResultMode,
    terminal_model: &Arc<FairMutex<TerminalModel>>,
    dfas: &RegexDFAs,
    result_tx: &async_channel::Sender<FindTaskMessage>,
) {
    let mut current_row = start_row;
    let mut slice_rows = INITIAL_ROWS_PER_SLICE;
    // Matches of finished slices not yet sent, covering rows `batch_start..current_row`.
    let mut batch: Vec<AbsoluteMatch> = Vec::new();
    let mut batch_start = start_row;
    let mut batch_began = Instant::now();

    loop {
        let slice_result = {
            let lock_start = Instant::now();
            let model = terminal_model.lock();

            let scanned = (|| {
                let block = model.block_list().block_at(block_index)?;
                let grid = match grid_type {
                    GridType::Output => block.output_grid(),
                    GridType::PromptAndCommand => block.prompt_and_command_grid(),
                    _ => return None,
                };

                let grid_handler = grid.grid_handler();
                let total_rows = grid_handler.total_rows();
                let effective_end = end_row.unwrap_or(total_rows).min(total_rows);

                if current_row >= effective_end {
                    return None;
                }

                let chunk_end = (current_row + slice_rows).min(effective_end);
                // Do not cut a soft-wrapped logical line in two: a match spanning the cut would
                // be found by neither slice.
                let mut chunk_end = chunk_end;
                while chunk_end < effective_end
                    && chunk_end - current_row < slice_rows + ROWS_PER_CHUNK
                    && grid_handler.row_soft_wraps(chunk_end - 1)
                {
                    chunk_end += 1;
                }

                let point_matches = scan_grid_range(grid_handler, dfas, current_row, chunk_end);
                let matches: Vec<AbsoluteMatch> = point_matches
                    .iter()
                    .map(|range| AbsoluteMatch::from_range(range, grid_handler))
                    .collect();
                Some((matches, chunk_end, effective_end))
            })();

            scanned.map(|(matches, chunk_end, effective_end)| {
                (matches, chunk_end, effective_end, lock_start.elapsed())
            })
        };

        let Some((mut matches, chunk_end, effective_end, elapsed)) = slice_result else {
            // The block or the rest of the range is gone: report what was scanned so far.
            if current_row > batch_start {
                send_batch(
                    &mode,
                    block_index,
                    grid_type,
                    batch_start,
                    current_row,
                    std::mem::take(&mut batch),
                    result_tx,
                )
                .await;
            }
            return;
        };

        // find_in_range returns matches in descending order; reverse to ascending.
        matches.reverse();
        batch.extend(matches);

        let done = chunk_end >= effective_end;
        if done
            || chunk_end - batch_start >= ROWS_PER_CHUNK
            || batch_began.elapsed() >= MAX_BATCH_AGE
        {
            send_batch(
                &mode,
                block_index,
                grid_type,
                batch_start,
                chunk_end,
                std::mem::take(&mut batch),
                result_tx,
            )
            .await;
            batch_start = chunk_end;
            batch_began = Instant::now();
        }

        if done {
            break;
        }

        current_row = chunk_end;
        slice_rows = next_slice_rows(slice_rows, elapsed);

        // Always give the main thread (and an abort) a turn between slices.
        yield_now().await;
    }
}

/// Sends the matches found in rows `first_row..end_row` based on the scan mode.
async fn send_batch(
    mode: &ScanResultMode,
    block_index: BlockIndex,
    grid_type: GridType,
    first_row: usize,
    end_row: usize,
    matches: Vec<AbsoluteMatch>,
    result_tx: &async_channel::Sender<FindTaskMessage>,
) {
    match mode {
        ScanResultMode::FullBlock => {
            if !matches.is_empty() {
                let _ = result_tx
                    .send(FindTaskMessage::BlockGridMatches {
                        block_index,
                        grid_type,
                        matches,
                    })
                    .await;
            }
        }
        ScanResultMode::DirtyRange {
            num_lines_truncated,
        } => {
            let absolute_start = first_row as u64 + num_lines_truncated;
            let absolute_end = (end_row - 1) as u64 + num_lines_truncated;
            let _ = result_tx
                .send(FindTaskMessage::DirtyRangeMatches {
                    block_index,
                    grid_type,
                    dirty_range: absolute_start..=absolute_end,
                    matches,
                })
                .await;
        }
    }
}

/// Scans a range of rows in a grid for matches.
fn scan_grid_range(
    grid: &GridHandler,
    dfas: &RegexDFAs,
    start_row: usize,
    end_row: usize,
) -> Vec<RangeInclusive<Point>> {
    let columns = Dimensions::columns(grid);
    let start_point = Point::new(start_row, 0);
    let end_point = Point::new(end_row.saturating_sub(1), columns.saturating_sub(1));

    grid.find_in_range(dfas, start_point, end_point).collect()
}
