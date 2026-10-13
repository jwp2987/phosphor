mod alt_screen;
pub mod async_find;
mod block_list;
#[allow(dead_code)]
mod rich_content;
#[cfg(any(test, feature = "integration_tests"))]
mod testing;

use std::collections::HashMap;
use std::ops::RangeInclusive;
use std::sync::Arc;

use alt_screen::{AltScreenFindRun, run_find_on_alt_screen};
pub use async_find::{AsyncFindController, AsyncFindStatus};
use block_list::run_find_on_block_list;
pub use block_list::{BlockGridMatch, BlockListFindRun, BlockListMatch};
use parking_lot::FairMutex;
use rich_content::FindableRichContentHandle;
pub use rich_content::{FindableRichContentView, RichContentMatchId};
use settings::Setting as _;
use warpui::{AppContext, Entity, EntityId, ModelContext, SingletonEntity, ViewHandle};

use crate::settings::InputModeSettings;
use crate::terminal::block_list_element::GridType;
use crate::terminal::block_list_viewport::InputMode;
use crate::terminal::model::blockgrid::BlockGrid;
use crate::terminal::model::grid::grid_handler::{ContentGeneration, GridHandler};
use crate::terminal::model::index::Point;
use crate::terminal::model::terminal_model::BlockIndex;
use crate::terminal::model::{BlockId, TerminalModel};
use crate::terminal::settings::TerminalSettings;
use crate::view_components::find::{FindDirection, FindEvent, FindModel};

/// Pre-computed find data for rendering a single block.
///
/// This struct provides a unified interface for both sync (`BlockListFindRun`) and async
/// (`AsyncFindController`) find paths, allowing the rendering code to work with either.
///
/// Stores references to the underlying data source and provides methods to create iterators
/// on demand (since iterators can only be consumed once).
pub enum BlockFindRenderData<'a> {
    /// Data from the synchronous find path.
    Sync {
        run: &'a BlockListFindRun,
        block_index: BlockIndex,
    },
    /// Data from the asynchronous find path.
    ///
    /// For the async path, we pre-compute and store converted matches since they use
    /// absolute coordinates internally and need conversion to relative Points.
    Async {
        /// Pre-converted command grid matches (filtered for truncation).
        command_matches: Vec<RangeInclusive<Point>>,
        /// Pre-converted output grid matches (filtered for truncation).
        output_matches: Vec<RangeInclusive<Point>>,
        /// Focused range in command grid, if any.
        focused_command_range: Option<RangeInclusive<Point>>,
        /// Focused range in output grid, if any.
        focused_output_range: Option<RangeInclusive<Point>>,
    },
}

impl<'a> BlockFindRenderData<'a> {
    /// Creates render data from the sync `BlockListFindRun`.
    pub fn from_sync(run: &'a BlockListFindRun, block_index: BlockIndex) -> Self {
        Self::Sync { run, block_index }
    }

    /// Creates render data from the async `AsyncFindController`.
    ///
    /// This pre-converts matches from absolute to relative coordinates, filtering out
    /// any matches that have been truncated from scrollback.
    pub fn from_async(
        controller: &'a AsyncFindController,
        block_index: BlockIndex,
        command_grid: Option<&GridHandler>,
        output_grid: Option<&GridHandler>,
    ) -> Self {
        // Convert command grid matches. Highlights are rendered in *original*
        // grid coordinates (the renderer maps them to displayed positions
        // itself), matching the sync path.
        let command_matches = command_grid
            .and_then(|grid| {
                controller
                    .matches_for_block_grid(block_index, GridType::PromptAndCommand)
                    .map(|matches| {
                        matches
                            .iter()
                            .filter_map(|m| m.to_original_range(grid))
                            .collect::<Vec<_>>()
                    })
            })
            .unwrap_or_default();

        // Convert output grid matches, skipping any hidden by an active block
        // filter so highlights match the (filtered) visible content. Uses
        // original coordinates (see command_matches above).
        let output_matches = output_grid
            .and_then(|grid| {
                controller
                    .matches_for_block_grid(block_index, GridType::Output)
                    .map(|matches| {
                        matches
                            .iter()
                            .filter(|m| !m.is_filtered)
                            .filter_map(|m| m.to_original_range(grid))
                            .collect::<Vec<_>>()
                    })
            })
            .unwrap_or_default();

        // Get focused match ranges (also in original coordinates).
        let focused_match = controller.focused_terminal_match();
        let focused_command_range = focused_match
            .as_ref()
            .filter(|m| m.block_index == block_index && m.grid_type == GridType::PromptAndCommand)
            .and_then(|m| command_grid.and_then(|grid| m.range.to_original_range(grid)));
        let focused_output_range = focused_match
            .as_ref()
            .filter(|m| m.block_index == block_index && m.grid_type == GridType::Output)
            .and_then(|m| output_grid.and_then(|grid| m.range.to_original_range(grid)));

        Self::Async {
            command_matches,
            output_matches,
            focused_command_range,
            focused_output_range,
        }
    }

    /// Returns an iterator over match ranges for the command grid.
    pub fn command_grid_matches(
        &self,
    ) -> Option<Box<dyn Iterator<Item = &RangeInclusive<Point>> + '_>> {
        match self {
            Self::Sync { run, block_index } => {
                Some(run.matches_for_block_grid(*block_index, GridType::PromptAndCommand))
            }
            Self::Async {
                command_matches, ..
            } => Some(Box::new(command_matches.iter())),
        }
    }

    /// Returns an iterator over match ranges for the output grid.
    pub fn output_grid_matches(
        &self,
    ) -> Option<Box<dyn Iterator<Item = &RangeInclusive<Point>> + '_>> {
        match self {
            Self::Sync { run, block_index } => {
                Some(run.matches_for_block_grid(*block_index, GridType::Output))
            }
            Self::Async { output_matches, .. } => Some(Box::new(output_matches.iter())),
        }
    }

    /// Returns the focused match range if it's in the specified grid.
    pub fn focused_range_for_grid(&self, grid_type: GridType) -> Option<RangeInclusive<Point>> {
        match self {
            Self::Sync { run, block_index } => run.focused_match().and_then(|m| match m {
                BlockListMatch::CommandBlock(grid_match)
                    if grid_match.block_index == *block_index
                        && grid_match.grid_type == grid_type =>
                {
                    Some(grid_match.range.clone())
                }
                _ => None,
            }),
            Self::Async {
                focused_command_range,
                focused_output_range,
                ..
            } => match grid_type {
                GridType::PromptAndCommand => focused_command_range.clone(),
                GridType::Output => focused_output_range.clone(),
                _ => None,
            },
        }
    }
}

/// The active block as the async path last enqueued a scan of it (#817).
///
/// [`TerminalFindModel::rerun_find_on_active_grid`] compares the active block
/// against it to tell a wakeup that changed nothing there -- the common case
/// while a background job prints into a background block -- apart from a
/// change the dirty-row ranges could not localise.
struct ActiveBlockScanState {
    block_index: BlockIndex,
    block_id: BlockId,
    /// The output grid's generation, if the enqueued work covers the whole
    /// grid at it; `None` if unknown (the next wakeup without dirty rows then
    /// rescans).
    output: Option<ContentGeneration>,
    /// Same, for the prompt-and-command grid.
    prompt_and_command: Option<ContentGeneration>,
}

/// Consumes `grid`'s find dirty-row range and reads its content generation.
///
/// Returns the dirty range as the `(rows, grid type, lines truncated)` tuple
/// [`TerminalFindModel::invalidate_async_find_block`] takes, and the
/// generation the grid is at once that range has been taken.
fn take_find_dirty_info(
    grid: Option<&mut BlockGrid>,
    grid_type: GridType,
) -> (
    Option<(RangeInclusive<usize>, GridType, u64)>,
    Option<ContentGeneration>,
) {
    let Some(grid) = grid else {
        return (None, None);
    };
    let grid_handler = grid.grid_handler_mut();
    let dirty_info = grid_handler
        .take_find_dirty_rows_range()
        .map(|dirty| (dirty, grid_type, grid_handler.num_lines_truncated()));
    (dirty_info, Some(grid_handler.content_generation()))
}

/// `TerminalView`-scoped model for the find bar.
pub struct TerminalFindModel {
    terminal_model: Arc<FairMutex<TerminalModel>>,

    rich_content_views: HashMap<EntityId, Box<dyn FindableRichContentHandle>>,

    /// The most recent find "run" on the alt screen, if any.
    alt_screen_find_run: Option<AltScreenFindRun>,

    /// The most recent find "run" on the block list, if any (sync path).
    block_list_find_run: Option<BlockListFindRun>,

    /// `true` if the find bar is open.
    is_find_bar_open: bool,

    /// Controller for async find operations.
    pub(crate) async_find_controller: Option<AsyncFindController>,

    /// Bumped every time the find results a view can observe change: the match
    /// set (positions or filtered flags, on any grid or rich-content view), the
    /// focused match, the scanning state, or the active run itself.
    ///
    /// Views that repaint on [`FindEvent::RanFind`] compare this against the
    /// value they last repainted for and skip the frame when it is equal. With
    /// the find bar open, every pty chunk re-runs find on the active block's
    /// dirty rows; for a command whose output never touches the query that
    /// rerun reproduces the same results, and before this existed it still cost
    /// a second full-window frame per chunk (#815).
    ///
    /// Only the per-chunk paths -- [`Self::rerun_find_on_active_grid`] and the
    /// async controller's result stream -- do real change detection. Every
    /// user-driven path (a new query, clearing, focusing the next match, a block
    /// filter) bumps unconditionally, so they behave exactly as before.
    results_revision: u64,

    /// Async path only: the active block as of the last scan enqueued for it,
    /// or `None` when nothing is known (no wakeup since the run started, or a
    /// run whose work queue was not live), which makes the next wakeup rescan
    /// the active block whole.
    active_block_scan_state: Option<ActiveBlockScanState>,
}

impl FindModel for TerminalFindModel {
    fn focused_match_index(&self) -> Option<usize> {
        if self.terminal_model.lock().is_alt_screen_active() {
            self.alt_screen_find_run
                .as_ref()
                .and_then(|run| run.focused_match_index())
        } else if let Some(controller) = &self.async_find_controller {
            controller.focused_match_index()
        } else {
            self.block_list_find_run
                .as_ref()
                .and_then(|run| run.focused_match_index())
        }
    }

    fn match_count(&self) -> usize {
        if self.terminal_model.lock().is_alt_screen_active() {
            self.alt_screen_find_run
                .as_ref()
                .map(|run| run.matches().len())
                .unwrap_or(0)
        } else if let Some(controller) = &self.async_find_controller {
            controller.match_count()
        } else {
            self.block_list_find_run
                .as_ref()
                .map(|run| run.matches().count())
                .unwrap_or(0)
        }
    }

    fn default_find_direction(&self, app: &AppContext) -> FindDirection {
        let input_mode = *InputModeSettings::as_ref(app).input_mode.value();
        match input_mode {
            InputMode::PinnedToBottom | InputMode::Waterfall => FindDirection::Up,
            InputMode::PinnedToTop => FindDirection::Down,
        }
    }

    fn is_scanning(&self) -> bool {
        self.is_async_find_scanning()
    }

    fn results_revision(&self) -> Option<u64> {
        Some(self.results_revision)
    }
}

impl TerminalFindModel {
    pub fn new(terminal_model: Arc<FairMutex<TerminalModel>>, ctx: &AppContext) -> Self {
        let async_find_controller = if TerminalSettings::as_ref(ctx).is_async_find_enabled() {
            Some(AsyncFindController::new(terminal_model.clone()))
        } else {
            None
        };

        Self {
            terminal_model,
            rich_content_views: HashMap::new(),
            alt_screen_find_run: None,
            block_list_find_run: None,
            is_find_bar_open: false,
            async_find_controller,
            results_revision: 0,
            active_block_scan_state: None,
        }
    }

    /// Records that the observable find results changed. See
    /// [`Self::results_revision`] for what counts.
    pub(crate) fn mark_results_changed(&mut self) {
        self.results_revision = self.results_revision.wrapping_add(1);
    }
    pub fn register_findable_rich_content_view<T: FindableRichContentView>(
        &mut self,
        view_handle: ViewHandle<T>,
    ) {
        let view_id = view_handle.id();
        let boxed_handle: Box<dyn FindableRichContentHandle> = Box::new(view_handle.clone());

        // Register with async find controller if enabled.
        if let Some(controller) = &mut self.async_find_controller {
            controller.register_rich_content_view(view_id, Box::new(view_handle));
        }

        self.rich_content_views.insert(view_id, boxed_handle);
    }

    /// Returns `true` if the find bar is currently open.
    pub(crate) fn is_find_bar_open(&self) -> bool {
        self.is_find_bar_open
    }

    /// Updates find bar visibility.
    pub(crate) fn set_is_find_bar_open(&mut self, is_open: bool) {
        self.is_find_bar_open = is_open;
    }

    /// Returns the last find run for the alt screen.
    pub(crate) fn alt_screen_find_run(&self) -> Option<&AltScreenFindRun> {
        self.alt_screen_find_run.as_ref()
    }

    /// Returns the last find run for the blocklist.
    pub(crate) fn block_list_find_run(&self) -> Option<&BlockListFindRun> {
        self.block_list_find_run.as_ref()
    }

    /// Returns the currently focused match as a `BlockListMatch`.
    ///
    /// This works for both sync and async find paths.
    pub(crate) fn focused_block_list_match(&self) -> Option<BlockListMatch> {
        let model = self.terminal_model.lock();
        if model.is_alt_screen_active() {
            // Alt screen doesn't use BlockListMatch.
            return None;
        }

        if let Some(controller) = &self.async_find_controller {
            // Async path: the focused match is either a terminal match or an
            // AI match (or neither). Try each in turn and synthesize the
            // corresponding `BlockListMatch` variant so consumers don't need
            // to know which path produced the focus.
            if let Some(async_match) = controller.focused_terminal_match() {
                let block = model.block_list().block_at(async_match.block_index)?;
                let grid = match async_match.grid_type {
                    GridType::PromptAndCommand => block.prompt_and_command_grid().grid_handler(),
                    GridType::Output => block.output_grid().grid_handler(),
                    _ => return None,
                };
                let range = async_match.range.to_range(grid)?;
                return Some(BlockListMatch::CommandBlock(BlockGridMatch {
                    block_index: async_match.block_index,
                    grid_type: async_match.grid_type,
                    range,
                    is_filtered: false,
                }));
            }
            if let Some(ai_match) = controller.focused_ai_match() {
                return Some(BlockListMatch::RichContent {
                    match_id: ai_match.match_id,
                    view_id: ai_match.view_id,
                    index: ai_match.total_index,
                });
            }
            None
        } else {
            // Sync path: get from block_list_find_run.
            self.block_list_find_run
                .as_ref()
                .and_then(|run| run.focused_match())
                .cloned()
        }
    }

    /// Returns the focused rich content (AI) match id, if any.
    ///
    /// This works for both sync and async find paths and is used by AI block
    /// rendering to apply the focused-match highlight color.
    pub(crate) fn focused_rich_content_match_id(&self) -> Option<RichContentMatchId> {
        if self.terminal_model.lock().is_alt_screen_active() {
            return None;
        }

        if let Some(controller) = &self.async_find_controller {
            controller.focused_ai_match().map(|m| m.match_id)
        } else {
            self.block_list_find_run
                .as_ref()
                .and_then(|run| run.focused_match())
                .and_then(|m| match m {
                    BlockListMatch::RichContent { match_id, .. } => Some(*match_id),
                    _ => None,
                })
        }
    }

    /// Returns find render data for a specific block, if find is active.
    ///
    /// This works for both sync and async find paths.
    ///
    /// Note: This method does NOT check if alt screen is active. It is intended
    /// for use during blocklist rendering where the caller has already determined
    /// that we are not in alt screen mode. Callers who need alt screen checking
    /// should do so before calling this method.
    ///
    /// For async find, the grid handlers are needed to convert from absolute to
    /// relative coordinates and filter truncated matches.
    pub(crate) fn find_render_data_for_block(
        &self,
        block_index: BlockIndex,
        command_grid: Option<&GridHandler>,
        output_grid: Option<&GridHandler>,
    ) -> Option<BlockFindRenderData<'_>> {
        if let Some(controller) = &self.async_find_controller {
            if !controller.has_active_find() {
                return None;
            }
            Some(BlockFindRenderData::from_async(
                controller,
                block_index,
                command_grid,
                output_grid,
            ))
        } else {
            self.block_list_find_run
                .as_ref()
                .map(|run| BlockFindRenderData::from_sync(run, block_index))
        }
    }

    /// Returns `FindOptions` applied to the active find run, if any.
    pub fn active_find_options(&self) -> Option<&FindOptions> {
        if self.terminal_model.lock().is_alt_screen_active() {
            self.alt_screen_find_run.as_ref().map(|run| run.options())
        } else if let Some(controller) = &self.async_find_controller {
            controller.find_options()
        } else {
            self.block_list_find_run.as_ref().map(|run| run.options())
        }
    }

    /// Runs find with the given `options` on the alt screen or blocklist (depending on which is
    /// active).
    pub fn run_find(&mut self, options: FindOptions, ctx: &mut ModelContext<Self>) {
        if self.terminal_model.lock().is_alt_screen_active() {
            self.alt_screen_find_run = Some(run_find_on_alt_screen(
                options,
                self.terminal_model.lock().alt_screen(),
            ));
            self.mark_results_changed();
            ctx.emit(FindEvent::RanFind);
            return;
        }

        let block_sort_direction = InputModeSettings::as_ref(ctx)
            .input_mode
            .value()
            .block_sort_direction();

        // Use async find if the feature flag is enabled.
        if let Some(controller) = &mut self.async_find_controller {
            log::trace!(
                "[async_find] Starting async find with query: {:?}",
                options.query
            );
            controller.start_find(&options, block_sort_direction, ctx);
            // Claim nothing about the active block: the run's initial queue
            // comes from `collect_block_info`, which skips zero-height blocks,
            // and the active block at the prompt is one of them (and keeps its
            // id once its command starts). The first wakeup rescans it whole.
            self.active_block_scan_state = None;
            self.mark_results_changed();
            ctx.emit(FindEvent::RanFind);
            return;
        }

        // Synchronous path.
        let _ = self.block_list_find_run.take();
        self.block_list_find_run = Some(run_find_on_block_list(
            options,
            self.terminal_model.lock().block_list(),
            &self.rich_content_views,
            block_sort_direction,
            ctx,
        ));
        self.mark_results_changed();
        ctx.emit(FindEvent::RanFind);
    }

    /// Reruns find with the same options applied to the current run, to be called if terminal
    /// contents have changed since the last find run.
    ///
    /// This runs on every terminal wakeup while the find bar is open, so it
    /// emits [`FindEvent::RanFind`] -- and bumps the results revision -- only
    /// when the rerun actually changed the results (#815). On the alt screen
    /// and the sync block-list path the rerun compares its new matches and
    /// focus against the old; on the async path nothing is known until the
    /// background scan reports back, and the result stream does the comparing.
    pub fn rerun_find_on_active_grid(&mut self, ctx: &mut ModelContext<Self>) {
        if self.terminal_model.lock().is_alt_screen_active() {
            if let Some(old_find_state) = self.alt_screen_find_run.take() {
                let (new_find_state, changed) =
                    old_find_state.rerun(self.terminal_model.lock().alt_screen());
                self.alt_screen_find_run = Some(new_find_state);
                if changed {
                    self.mark_results_changed();
                    ctx.emit(FindEvent::RanFind);
                }
            }
            return;
        }

        // Handle async find path.
        if let Some(controller) = &self.async_find_controller {
            if !controller.has_active_find() {
                return;
            }

            let is_accepting_work = controller.is_accepting_work();

            // Get the active block index and dirty range info.
            // We use active_block_index() (not last_non_hidden_block_by_index) because
            // the active block is where output is being written, even if it's still
            // "empty" and would be filtered out by the default BlockFilter.
            let mut model = self.terminal_model.lock();
            let active_block_index = model.block_list().active_block_index();

            // Consume dirty ranges from both grids. We need mutable access
            // because take_find_dirty_rows_range is destructive.
            let active_block = model.block_list_mut().active_block_mut();
            let block_id = active_block.id().clone();
            let (output_dirty_info, output_generation) = take_find_dirty_info(
                active_block.grid_of_type_mut(GridType::Output),
                GridType::Output,
            );
            let (command_dirty_info, command_generation) = take_find_dirty_info(
                active_block.grid_of_type_mut(GridType::PromptAndCommand),
                GridType::PromptAndCommand,
            );

            // Drop the model lock before emitting events.
            drop(model);

            // A grid with no dirty rows either did not change at all or
            // changed in a way the cursor-driven dirty tracking cannot
            // localise (resize/reflow, a scrollback load, ...). Only the
            // latter needs the active block rescanned whole; telling them
            // apart takes the grid's content generation (#817). Without this,
            // every wakeup whose output lands in another block -- a background
            // job printing while the shell sits at its prompt -- dropped and
            // rescanned the active block, which shows as scanning and so cost
            // a RanFind and a find-bar frame each time.
            let needs_full_rescan = match self.active_block_scan_state.take() {
                Some(previous)
                    if previous.block_index == active_block_index
                        && previous.block_id == block_id =>
                {
                    (output_dirty_info.is_none() && previous.output != output_generation)
                        || (command_dirty_info.is_none()
                            && previous.prompt_and_command != command_generation)
                }
                // A different active block (a new command, or indices shifted
                // under it), or nothing recorded: scan it whole, as before.
                _ => true,
            };

            // Remember a grid's generation only if the work enqueued below
            // covers the whole grid at it: a full rescan, or no change to
            // cover. A dirty-range rescan does not -- the dirty range is only
            // the rows the cursor passed through (#822), so the same chunk may
            // have changed others (an erase above the cursor, deleted lines).
            // Recording `None` for such a grid makes the first wakeup without
            // dirty rows rescan it whole, which is what healed such changes
            // before #817 too.
            let covered = |generation: Option<ContentGeneration>, has_dirty_rows: bool| {
                generation.filter(|_| needs_full_rescan || !has_dirty_rows)
            };
            let scan_state = ActiveBlockScanState {
                block_index: active_block_index,
                block_id,
                output: covered(output_generation, output_dirty_info.is_some()),
                prompt_and_command: covered(command_generation, command_dirty_info.is_some()),
            };
            // Likewise only remember anything if work is enqueued at all;
            // with no live work queue the next wakeup must not treat the
            // block as scanned.
            self.active_block_scan_state = is_accepting_work.then_some(scan_state);

            if needs_full_rescan {
                self.invalidate_async_find_block(active_block_index, None, ctx);
            } else if let Some(info) = output_dirty_info {
                self.invalidate_async_find_block(active_block_index, Some(info), ctx);
            }
            if let Some(info) = command_dirty_info {
                self.invalidate_async_find_block(active_block_index, Some(info), ctx);
            }
            return;
        }

        // Sync find path.
        // Find the last block index. This is the only block whose state may change.
        let last_block_index = self
            .terminal_model
            .lock()
            .block_list()
            .last_non_hidden_block_by_index()
            .unwrap_or_default();

        // Call find on the the last block's command and output grids.
        // If the block is a new finished block, the matches are inserted at a new key, the block's index in the blocklist.
        // If the block is an active, running block, its matches are overwritten in the terminal's block_matches.
        // The lock guard must be gone before `mark_results_changed` (which needs
        // `&mut self`), so the rerun happens inside this block and only its
        // verdict escapes.
        let changed = {
            let terminal_model = self.terminal_model.lock();
            let Some(block) = terminal_model.block_list().block_at(last_block_index) else {
                return;
            };
            let block_sort_direction = InputModeSettings::handle(ctx)
                .as_ref(ctx)
                .input_mode
                .value()
                .block_sort_direction();

            match self.block_list_find_run.take() {
                Some(old_find_run) => {
                    let (new_find_run, changed) =
                        old_find_run.rerun_on_block(block, last_block_index, block_sort_direction);
                    self.block_list_find_run = Some(new_find_run);
                    changed
                }
                None => false,
            }
        };
        if changed {
            self.mark_results_changed();
            ctx.emit(FindEvent::RanFind);
        }
    }

    /// Focus the "next" match, depending on the given [`FindDirection`], in the active find run's
    /// list of matches.
    ///
    /// If there is no focused match, focuses the first match in the list.
    pub fn focus_next_find_match(
        &mut self,
        find_direction: FindDirection,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.terminal_model.lock().is_alt_screen_active() {
            if let Some(alt_screen_find_run) = self.alt_screen_find_run.as_mut() {
                alt_screen_find_run.focus_next_match(find_direction);
            }
        } else if let Some(controller) = &mut self.async_find_controller {
            controller.focus_next_match(find_direction);
        } else if let Some(block_list_find_run) = self.block_list_find_run.as_mut() {
            let block_sort_direction = InputModeSettings::as_ref(ctx)
                .input_mode
                .value()
                .block_sort_direction();

            block_list_find_run.focus_next_match(find_direction, block_sort_direction);
        }
        self.mark_results_changed();
        ctx.emit(FindEvent::UpdatedFocusedMatch);
    }

    /// Notifies every registered rich-content child view (e.g. AI blocks) to
    /// drop its cached find state and repaint, **without** touching the active
    /// find run's options/config.
    ///
    /// Callers that just need stale highlights to disappear (e.g.
    /// `close_find_bar`) must use this rather than [`Self::clear_matches`].
    /// On the async path, `clear_matches` routes through
    /// `AsyncFindController::clear_results`, which also drops
    /// `current_find_options` — losing the query that `open_find_bar` later
    /// reads back via [`Self::active_find_options`] to restore the previous
    /// search.
    pub fn clear_rich_content_matches(&self, ctx: &mut ModelContext<Self>) {
        for view in self.rich_content_views.values() {
            view.clear_matches(ctx);
        }
    }

    /// Clears matches in the active find run, if any.
    pub fn clear_matches(&mut self, ctx: &mut ModelContext<Self>) {
        if self.terminal_model.lock().is_alt_screen_active() {
            if let Some(run) = self.alt_screen_find_run.take() {
                self.alt_screen_find_run = Some(run.cleared());
            }
        } else if let Some(controller) = &mut self.async_find_controller {
            controller.clear_results(ctx);
            self.active_block_scan_state = None;
        } else if let Some(run) = self.block_list_find_run.take() {
            for (_, rich_content_view) in self.rich_content_views.iter() {
                rich_content_view.clear_matches(ctx);
            }
            self.block_list_find_run = Some(run.cleared());
        }
        self.mark_results_changed();
        ctx.emit(FindEvent::RanFind);
    }

    /// Updates matches in the active find run for the block at the `block_index`, which is
    /// presumed to be filtered.
    ///
    /// Under the hood, this does not result in a new find run, but updates state on the matches
    /// for the existing find run, which is used to determine if matches should be represented in
    /// the find bar UI (e.g. match count, focused match index).
    pub fn update_matches_for_filtered_block(
        &mut self,
        block_index: BlockIndex,
        ctx: &mut ModelContext<Self>,
    ) {
        // On the async path, recompute which matches are hidden by the
        // (already-applied) block filter and treat filtered rows as if they do
        // not exist for search — mirroring the sync path below. This keeps the
        // match count, focus traversal, and highlights consistent with sync.
        if let Some(controller) = self.async_find_controller.as_mut() {
            controller.recompute_filtered_for_block(block_index);
            self.mark_results_changed();
            ctx.emit(FindEvent::RanFind);
            return;
        }

        // Same shape as the sync path of `rerun_find_on_active_grid`: release
        // the lock guard before `mark_results_changed` needs `&mut self`.
        let changed = {
            let terminal_model = self.terminal_model.lock();
            if let (Some(block_list_find_run), Some(filtered_block)) = (
                self.block_list_find_run.as_mut(),
                terminal_model.block_list().block_at(block_index),
            ) {
                let block_sort_direction = InputModeSettings::as_ref(ctx)
                    .input_mode
                    .value()
                    .block_sort_direction();

                block_list_find_run.update_matches_for_filtered_block(
                    filtered_block,
                    block_index,
                    block_sort_direction,
                );
                true
            } else {
                false
            }
        };
        if changed {
            self.mark_results_changed();
            ctx.emit(FindEvent::RanFind);
        }
    }

    /// Returns true if an async find operation is currently scanning.
    pub fn is_async_find_scanning(&self) -> bool {
        self.async_find_controller
            .as_ref()
            .map(|c| c.is_scanning())
            .unwrap_or(false)
    }

    /// Invalidates results for a specific block in async find.
    ///
    /// This should be called when a block's content changes.
    ///
    /// # Arguments
    /// * `block_index` - The index of the block that changed.
    /// * `dirty_info` - If provided, a `(row_range, grid_type, num_lines_truncated)`
    ///   tuple describing the dirty region. If `None`, a full block rescan is enqueued.
    /// * `ctx` - The model context.
    ///
    /// Emits [`FindEvent::RanFind`] only if enqueuing the rescan itself changed
    /// observable state (a full rescan drops the block's stale matches and
    /// shows the scanning indicator; a dirty-range rescan does neither). The
    /// results of the rescan are announced by the controller's result stream
    /// once they arrive, again only if they differ (#815).
    pub fn invalidate_async_find_block(
        &mut self,
        block_index: BlockIndex,
        dirty_info: Option<(RangeInclusive<usize>, GridType, u64)>,
        ctx: &mut ModelContext<Self>,
    ) {
        let changed = match self.async_find_controller.as_mut() {
            Some(controller) => controller.invalidate_block(block_index, dirty_info),
            None => return,
        };
        if changed {
            self.mark_results_changed();
            ctx.emit(FindEvent::RanFind);
        }
    }

    /// Notifies async find that a block has completed.
    ///
    /// This should be called when a command finishes, so that the completed block
    /// (which now has its final output) gets scanned for matches if find is active.
    /// Uses the dirty range accumulated during execution for incremental scanning.
    pub fn notify_block_completed(
        &mut self,
        block_index: BlockIndex,
        ctx: &mut ModelContext<Self>,
    ) {
        if self.async_find_controller.is_none() {
            return;
        }

        // Check if there's an active find before acquiring the lock.
        let has_active_find = self
            .async_find_controller
            .as_ref()
            .map(|c| c.has_active_find())
            .unwrap_or(false);

        if !has_active_find {
            return;
        }

        log::trace!(
            "[async_find] notify_block_completed: block_index={:?}",
            block_index
        );

        // Get the dirty range from the completed block's output grid.
        // We need mutable access to consume the dirty range.
        let (dirty_range, num_lines_truncated) = {
            let mut model = self.terminal_model.lock();
            // The fork's `BlockList` exposes mutable block access by id rather
            // than by index, so resolve the block's id first (dropping the
            // immutable borrow) before taking the mutable borrow.
            let block_id = model
                .block_list()
                .block_at(block_index)
                .map(|block| block.id().clone());
            block_id
                .and_then(|id| model.block_list_mut().mut_block_from_id(&id))
                .and_then(|block| block.grid_of_type_mut(GridType::Output))
                .map(|output_grid| {
                    let dirty_range = output_grid.grid_handler_mut().take_find_dirty_rows_range();
                    let num_lines_truncated = output_grid.grid_handler().num_lines_truncated();
                    (dirty_range, num_lines_truncated)
                })
                .unwrap_or((None, 0))
        };

        // Use invalidate_async_find_block which handles the dirty range properly.
        let dirty_info = dirty_range.map(|range| (range, GridType::Output, num_lines_truncated));
        self.invalidate_async_find_block(block_index, dirty_info, ctx);
    }

    /// Returns the async find controller, if enabled.
    pub fn async_find_controller(&self) -> Option<&AsyncFindController> {
        self.async_find_controller.as_ref()
    }
}

impl Entity for TerminalFindModel {
    type Event = FindEvent;
}

/// Parameters for a find "run".
#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    /// The find query, if any.
    pub query: Option<Arc<String>>,

    /// `true` if the find should be case-sensitive.
    pub is_case_sensitive: bool,

    /// `true` if the query should be matched as a regex pattern.
    pub is_regex_enabled: bool,

    /// If `Some()`, the find run only surfaces matches that are in blocks with the provided
    /// indices. If `None`, the find run surfaces matches across the entire blocklist.
    ///
    /// This is ignored when applied to alt screen find runs.
    pub blocks_to_include_in_results: Option<Vec<BlockIndex>>,
}

impl FindOptions {
    pub fn with_is_case_sensitive(mut self, is_case_sensitive: bool) -> Self {
        self.is_case_sensitive = is_case_sensitive;
        self
    }

    pub fn with_is_regex_enabled(mut self, is_regex_enabled: bool) -> Self {
        self.is_regex_enabled = is_regex_enabled;
        self
    }

    pub fn with_query(mut self, query: Option<impl Into<Arc<String>>>) -> Self {
        self.query = query.map(Into::into);
        self
    }

    pub fn with_blocks_to_include_in_results(
        mut self,
        block_indices: Option<impl IntoIterator<Item = BlockIndex>>,
    ) -> Self {
        self.blocks_to_include_in_results =
            block_indices.map(|indices| indices.into_iter().collect());
        self
    }
}

impl std::fmt::Debug for TerminalFindModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FindModel")
            .field("terminal_model", &"<TerminalModel>")
            .field(
                "rich_content_views",
                &self.rich_content_views.keys().collect::<Vec<_>>(),
            )
            .field("alt_screen_find_run", &self.alt_screen_find_run)
            .field("block_list_find_run", &self.block_list_find_run)
            .field("is_find_bar_open", &self.is_find_bar_open)
            .field("results_revision", &self.results_revision)
            .finish()
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
