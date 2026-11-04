//! What the grid renderer reads from a grid, as a trait, so that it can paint from either the
//! live [`GridHandler`] (under the terminal-model lock) or a [`GridSnapshot`] copied out from
//! under the lock (see `BlockListElement::paint`).
//!
//! The trait mirrors the `GridHandler` methods `render_grid` calls, with their exact signatures,
//! so that the generic renderer is the same code for both sources.

use std::borrow::Cow;
use std::ops::{Range, RangeInclusive};

use super::Dimensions;
use super::grid_handler::{ContentGeneration, GridHandler, ImagePlacement};
use super::row::Row;
use crate::terminal::model::image_map::ImagePlacementData;
use crate::terminal::model::index::Point;
use crate::terminal::model::secrets::{ObfuscateSecrets, Secret, SecretAndHandle, SecretHandle};

/// Everything `render_grid` reads from a grid, besides its [`Dimensions`].
pub trait GridSource: Dimensions {
    fn cursor_point(&self) -> Point;
    fn cursor_render_point(&self) -> Point;
    fn row(&self, index: usize) -> Option<Cow<'_, Row>>;
    fn marked_text(&self) -> Option<&str>;
    fn has_secrets(&self) -> bool;
    fn secret_at_displayed_point(&self, displayed_point: Point) -> Option<SecretAndHandle<'_>>;
    fn secret_by_handle(&self, secret_handle: SecretHandle) -> Option<&Secret>;
    fn get_image_ids_in_range(
        &self,
        displayed_start_row: usize,
        displayed_end_row: usize,
    ) -> Vec<ImagePlacement>;
    fn get_image_placement_data(
        &self,
        image_id: u32,
        placement_id: u32,
    ) -> Option<&ImagePlacementData>;
    fn filter_matches(&self) -> Option<&[RangeInclusive<Point>]>;
    fn filter_has_context_lines(&self) -> bool;
    fn displayed_output_rows(&self) -> Option<impl DoubleEndedIterator<Item = usize> + '_>;
    fn len_displayed(&self) -> Option<usize>;
    fn max_content_row(&self) -> usize;
}

impl GridSource for GridHandler {
    fn cursor_point(&self) -> Point {
        GridHandler::cursor_point(self)
    }

    fn cursor_render_point(&self) -> Point {
        GridHandler::cursor_render_point(self)
    }

    fn row(&self, index: usize) -> Option<Cow<'_, Row>> {
        GridHandler::row(self, index)
    }

    fn marked_text(&self) -> Option<&str> {
        GridHandler::marked_text(self)
    }

    fn has_secrets(&self) -> bool {
        GridHandler::has_secrets(self)
    }

    fn secret_at_displayed_point(&self, displayed_point: Point) -> Option<SecretAndHandle<'_>> {
        GridHandler::secret_at_displayed_point(self, displayed_point)
    }

    fn secret_by_handle(&self, secret_handle: SecretHandle) -> Option<&Secret> {
        GridHandler::secret_by_handle(self, secret_handle)
    }

    fn get_image_ids_in_range(
        &self,
        displayed_start_row: usize,
        displayed_end_row: usize,
    ) -> Vec<ImagePlacement> {
        GridHandler::get_image_ids_in_range(self, displayed_start_row, displayed_end_row)
    }

    fn get_image_placement_data(
        &self,
        image_id: u32,
        placement_id: u32,
    ) -> Option<&ImagePlacementData> {
        GridHandler::get_image_placement_data(self, image_id, placement_id)
    }

    fn filter_matches(&self) -> Option<&[RangeInclusive<Point>]> {
        GridHandler::filter_matches(self)
    }

    fn filter_has_context_lines(&self) -> bool {
        GridHandler::filter_has_context_lines(self)
    }

    fn displayed_output_rows(&self) -> Option<impl DoubleEndedIterator<Item = usize> + '_> {
        GridHandler::displayed_output_rows(self)
    }

    fn len_displayed(&self) -> Option<usize> {
        GridHandler::len_displayed(self)
    }

    fn max_content_row(&self) -> usize {
        GridHandler::max_content_row(self)
    }
}

/// A copy of what the renderer reads from a grid for a range of rows, taken under the model lock
/// and painted after it is released.
///
/// Only grids whose paint needs nothing but their cells are snapshotted: a grid with secrets, a
/// block filter (displayed-output mapping, matches, context lines), marked text or image
/// placements answers those queries live, so [`Self::capture`] declines and the caller paints it
/// from the locked model as before. That keeps every trivial answer below exact.
#[derive(Clone, Debug)]
pub struct GridSnapshot {
    first_row: usize,
    /// `None` where the grid had no such row, as `GridHandler::row` would have said.
    rows: Vec<Option<Row>>,
    columns: usize,
    visible_rows: usize,
    total_rows: usize,
    cursor_point: Point,
    cursor_render_point: Point,
    is_cursor_on_wide_char: bool,
    max_content_row: usize,
    secret_obfuscation: ObfuscateSecrets,
    content_generation: ContentGeneration,
}

impl GridSnapshot {
    /// Copies rows `rows` of `grid`, or `None` when this grid has to be painted from the model.
    pub fn capture(grid: &GridHandler, rows: Range<usize>) -> Option<Self> {
        if grid.has_secrets()
            || grid.has_displayed_output()
            || grid.filter_matches().is_some()
            || grid.filter_has_context_lines()
            || grid.marked_text().is_some()
            || !GridHandler::get_image_ids_in_range(grid, rows.start, rows.end).is_empty()
        {
            return None;
        }
        Some(Self {
            first_row: rows.start,
            rows: rows
                .clone()
                .map(|index| grid.row(index).map(Cow::into_owned))
                .collect(),
            columns: grid.columns(),
            visible_rows: grid.visible_rows(),
            total_rows: grid.total_rows(),
            cursor_point: grid.cursor_point(),
            cursor_render_point: grid.cursor_render_point(),
            is_cursor_on_wide_char: grid.is_cursor_on_wide_char(),
            max_content_row: grid.max_content_row(),
            secret_obfuscation: grid.get_secret_obfuscation(),
            content_generation: grid.content_generation(),
        })
    }

    /// The rows this snapshot holds.
    pub fn rows(&self) -> Range<usize> {
        self.first_row..self.first_row + self.rows.len()
    }

    /// The generation of the grid when it was captured; equal to the grid's current generation
    /// only if nothing has mutated the grid since.
    pub fn content_generation(&self) -> ContentGeneration {
        self.content_generation
    }

    pub fn is_cursor_on_wide_char(&self) -> bool {
        self.is_cursor_on_wide_char
    }

    pub fn secret_obfuscation(&self) -> &ObfuscateSecrets {
        &self.secret_obfuscation
    }
}

impl Dimensions for GridSnapshot {
    fn total_rows(&self) -> usize {
        self.total_rows
    }

    fn visible_rows(&self) -> usize {
        self.visible_rows
    }

    fn columns(&self) -> usize {
        self.columns
    }
}

impl GridSource for GridSnapshot {
    fn cursor_point(&self) -> Point {
        self.cursor_point
    }

    fn cursor_render_point(&self) -> Point {
        self.cursor_render_point
    }

    fn row(&self, index: usize) -> Option<Cow<'_, Row>> {
        let offset = index.checked_sub(self.first_row)?;
        self.rows.get(offset)?.as_ref().map(Cow::Borrowed)
    }

    fn marked_text(&self) -> Option<&str> {
        None
    }

    fn has_secrets(&self) -> bool {
        false
    }

    fn secret_at_displayed_point(&self, _displayed_point: Point) -> Option<SecretAndHandle<'_>> {
        None
    }

    fn secret_by_handle(&self, _secret_handle: SecretHandle) -> Option<&Secret> {
        None
    }

    fn get_image_ids_in_range(
        &self,
        _displayed_start_row: usize,
        _displayed_end_row: usize,
    ) -> Vec<ImagePlacement> {
        Vec::new()
    }

    fn get_image_placement_data(
        &self,
        _image_id: u32,
        _placement_id: u32,
    ) -> Option<&ImagePlacementData> {
        None
    }

    fn filter_matches(&self) -> Option<&[RangeInclusive<Point>]> {
        None
    }

    fn filter_has_context_lines(&self) -> bool {
        false
    }

    fn displayed_output_rows(&self) -> Option<impl DoubleEndedIterator<Item = usize> + '_> {
        None::<std::iter::Empty<usize>>
    }

    fn len_displayed(&self) -> Option<usize> {
        None
    }

    fn max_content_row(&self) -> usize {
        self.max_content_row
    }
}

#[cfg(test)]
#[path = "grid_source_tests.rs"]
mod tests;
