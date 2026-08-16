//! Exports helper test-only methods for use in unit and integration tests.
use itertools::Itertools;

use crate::terminal::model::terminal_model::BlockIndex;

use super::{block_list::BlockListMatch, BlockListFindRun, TerminalFindModel};

impl TerminalFindModel {
    /// Number of block-list matches that are not hidden by an active block filter.
    ///
    /// Branches on the async controller exactly as the production accessors in
    /// `model.rs` do. It used to read `block_list_find_run` unconditionally --
    /// but `run_find` returns early on the async path and never populates that
    /// field, so with `FeatureFlag::AsyncFind` on (it is in `enabled_features()`
    /// behind the default-on `async_find` cargo feature) this returned 0 for
    /// every query, no matter how many matches the find actually found.
    /// `AsyncFindResults::total_match_count` already excludes filtered matches
    /// "so the count matches the sync path", so the two branches agree.
    pub fn visible_block_list_match_count(&self) -> usize {
        if let Some(controller) = &self.async_find_controller {
            return controller.match_count();
        }
        self.block_list_find_run
            .as_ref()
            .map(|run| {
                run.matches()
                    .filter(|find_match| !find_match.is_filtered())
                    .collect_vec()
                    .len()
            })
            .unwrap_or(0)
    }
}

impl BlockListFindRun {
    pub fn matches_for_block(&self, index: BlockIndex) -> impl Iterator<Item = &BlockListMatch> {
        self.matches()
            .filter(move |find_match| find_match.matches_block(index))
    }

    pub fn focused_match_block_index(&self) -> Option<BlockIndex> {
        self.focused_match_index().and_then(|index| {
            self.matches().nth(index).and_then(|m| {
                if let BlockListMatch::CommandBlock(block_match) = m {
                    Some(block_match.block_index)
                } else {
                    None
                }
            })
        })
    }
}
