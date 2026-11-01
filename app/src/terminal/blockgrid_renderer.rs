use crate::settings::EnforceMinimumContrast;
use crate::terminal::color;
use crate::terminal::grid_renderer::{
    render_cursor, render_grid, CellGlyphCache, SharedCellGlyphCache,
};
use crate::terminal::model::blockgrid::{BlockGrid, CursorDisplayPoint};
use crate::terminal::model::grid::grid_handler::{GridHandler, Link};
use crate::terminal::model::grid::grid_source::GridSnapshot;
use crate::terminal::model::index::Point;
use crate::terminal::model::terminal_model::BlockIndex;
use crate::terminal::model::ObfuscateSecrets;
use crate::terminal::SizeInfo;
use crate::themes::theme::WarpTheme;
use pathfinder_color::ColorU;
use rustc_hash::FxHashMap;
use std::collections::HashMap;
use std::ops::Neg;
use std::ops::{Range, RangeInclusive};
use std::rc::Rc;
use std::sync::OnceLock;
use warpui::LayerInsertionPoint;

use super::block_list_element::GridType;
use warpui::fonts::{FamilyId, Properties, Weight};
use warpui::geometry::rect::RectF;
use warpui::geometry::vector::{vec2f, Vector2F};
use warpui::{AppContext, Element, EntityId, PaintContext};

use super::model::ansi::{CursorShape, CursorStyle};
use super::model::grid::RespectDisplayedOutput;
use super::model::image_map::StoredImageMetadata;
use super::model::SecretHandle;

pub struct GridRenderParams {
    /// Shared with `Appearance` (see `Appearance::shared_theme`) so building the params each
    /// frame doesn't clone the theme.
    pub warp_theme: std::sync::Arc<WarpTheme>,
    pub font_family: FamilyId,
    pub font_size: f32,
    pub font_weight: Weight,
    pub line_height_ratio: f32,
    pub enforce_minimum_contrast: EnforceMinimumContrast,
    pub obfuscate_secrets: ObfuscateSecrets,
    pub size_info: SizeInfo,
    pub cell_size: Vector2F,
    pub use_ligature_rendering: bool,
    /// When true, suppresses cursor rendering for CLI agents when rich input is open. For agents that draw their own cursor (SHOW_CURSOR off),
    /// the cursor cell is skipped. For agents that let Zap draw the cursor
    /// (SHOW_CURSOR on), the `draw_cursor` call and cursor contrast colouring
    /// are suppressed instead.
    pub hide_cursor_cell: bool,
}

pub struct BlockGridParams {
    pub grid_render_params: GridRenderParams,
    pub colors: color::List,
    pub override_colors: color::OverrideList,
    pub bounds: RectF,
}

impl BlockGrid {
    #[allow(clippy::too_many_arguments)]
    pub fn draw<'a>(
        &self,
        grid_origin: Vector2F,
        origin: Vector2F,
        glyphs: &mut CellGlyphCache,
        alpha: u8,
        highlighted_url: Option<&Link>,
        link_tool_tip: Option<&Link>,
        hovered_secret: Option<SecretHandle>,
        ordered_matches: Option<impl Iterator<Item = &'a RangeInclusive<Point>>>,
        focused_match_range: Option<&RangeInclusive<Point>>,
        properties: Properties,
        block_grid_params: &BlockGridParams,
        visible_cursor_shape: Option<CursorShape>,
        image_metadata: &HashMap<u32, StoredImageMetadata>,
        ctx: &mut PaintContext,
        app: &AppContext,
    ) {
        let start_row = hidden_rows_above(
            grid_origin,
            origin,
            block_grid_params.grid_render_params.cell_size,
        );
        let hidden_rows_below = hidden_rows_below(
            grid_origin,
            origin,
            block_grid_params.grid_render_params.cell_size,
            block_grid_params.bounds,
            self.len_displayed(),
        );
        let end_row = self.len_displayed().saturating_sub(hidden_rows_below);

        let grid = self.grid_handler();

        let obfuscate_secrets = block_grid_params
            .grid_render_params
            .obfuscate_secrets
            .and(&grid.get_secret_obfuscation());
        render_grid(
            grid,
            start_row,
            end_row,
            &block_grid_params.colors,
            &block_grid_params.override_colors,
            &block_grid_params.grid_render_params.warp_theme,
            properties,
            block_grid_params.grid_render_params.font_family,
            block_grid_params.grid_render_params.font_size,
            block_grid_params.grid_render_params.line_height_ratio,
            block_grid_params.grid_render_params.cell_size,
            block_grid_params
                .grid_render_params
                .size_info
                .padding_x_px(),
            grid_origin,
            glyphs,
            alpha,
            highlighted_url,
            link_tool_tip,
            ordered_matches,
            focused_match_range,
            block_grid_params
                .grid_render_params
                .enforce_minimum_contrast,
            obfuscate_secrets,
            hovered_secret,
            block_grid_params.grid_render_params.use_ligature_rendering,
            visible_cursor_shape,
            RespectDisplayedOutput::Yes,
            image_metadata,
            None,
            block_grid_params.grid_render_params.hide_cursor_cell,
            ctx,
            app,
        );
    }

    /// The displayed rows [`Self::draw`] paints for this origin, or `None` while a block filter
    /// is active (rows then differ from the original coordinates of find matches).
    pub fn visible_row_window(
        &self,
        grid_origin: Vector2F,
        origin: Vector2F,
        block_grid_params: &BlockGridParams,
    ) -> Option<std::ops::Range<usize>> {
        if self.grid_handler().len_displayed().is_some() {
            return None;
        }
        let cell_size = block_grid_params.grid_render_params.cell_size;
        let start_row = hidden_rows_above(grid_origin, origin, cell_size);
        let below = hidden_rows_below(
            grid_origin,
            origin,
            cell_size,
            block_grid_params.bounds,
            self.len_displayed(),
        );
        Some(start_row..self.len_displayed().saturating_sub(below))
    }

    /// What [`Self::draw`] would paint, captured under the model lock to be painted once it is
    /// released (see `BlockListElement::paint`), or `None` when this grid has to be painted from
    /// the model (see [`GridSnapshot::capture`]).
    #[allow(clippy::too_many_arguments)]
    pub fn capture_draw<'a>(
        &self,
        grid_origin: Vector2F,
        origin: Vector2F,
        alpha: u8,
        highlighted_url: Option<&Link>,
        link_tool_tip: Option<&Link>,
        hovered_secret: Option<SecretHandle>,
        ordered_matches: Option<impl Iterator<Item = &'a RangeInclusive<Point>>>,
        focused_match_range: Option<&RangeInclusive<Point>>,
        properties: Properties,
        block_grid_params: &BlockGridParams,
        visible_cursor_shape: Option<CursorShape>,
        snapshots: &mut GridSnapshotCache,
        key: GridSnapshotKey,
        insertion_point: LayerInsertionPoint,
    ) -> Option<DeferredGridDraw> {
        let start_row = hidden_rows_above(
            grid_origin,
            origin,
            block_grid_params.grid_render_params.cell_size,
        );
        let hidden_rows_below = hidden_rows_below(
            grid_origin,
            origin,
            block_grid_params.grid_render_params.cell_size,
            block_grid_params.bounds,
            self.len_displayed(),
        );
        let end_row = self.len_displayed().saturating_sub(hidden_rows_below);

        let snapshot = snapshots.get_or_capture(key, self.grid_handler(), start_row..end_row)?;
        let obfuscate_secrets = block_grid_params
            .grid_render_params
            .obfuscate_secrets
            .and(snapshot.secret_obfuscation());
        Some(DeferredGridDraw {
            snapshot,
            start_row,
            end_row,
            grid_origin,
            alpha,
            highlighted_url: highlighted_url.cloned(),
            link_tool_tip: link_tool_tip.cloned(),
            hovered_secret,
            ordered_matches: ordered_matches.map(|matches| matches.cloned().collect()),
            focused_match_range: focused_match_range.cloned(),
            properties,
            visible_cursor_shape,
            obfuscate_secrets,
            insertion_point,
        })
    }

    pub fn draw_with_default_params(
        &self,
        grid_origin: Vector2F,
        origin: Vector2F,
        block_grid_params: &BlockGridParams,
        ctx: &mut PaintContext,
        app: &AppContext,
    ) {
        let mut glyphs = SharedCellGlyphCache::acquire();
        self.draw(
            grid_origin,
            origin,
            &mut glyphs,
            255,
            None,
            None,
            None,
            None::<std::iter::Empty<&RangeInclusive<Point>>>,
            None,
            Properties::default(),
            block_grid_params,
            None,
            &HashMap::new(),
            ctx,
            app,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn draw_cursor(
        &self,
        grid_origin: Vector2F,
        grid_render_params: &GridRenderParams,
        ctx: &mut PaintContext,
        terminal_view_id: EntityId,
        cursor_hint_text: Option<&mut Box<dyn Element>>,
        color: ColorU,
        app: &AppContext,
    ) {
        let cursor_style = self.cursor_style();
        let (cursor_display_point, is_cursor_on_wide_char, cursor_style, cursor_hint_text) =
            match self.cursor_display_point() {
                Some(CursorDisplayPoint::Visible(cursor_display_point)) => (
                    cursor_display_point,
                    self.grid_handler().is_cursor_on_wide_char(),
                    cursor_style,
                    cursor_hint_text,
                ),
                Some(CursorDisplayPoint::HiddenCache(cursor_display_point)) => (
                    cursor_display_point,
                    false,
                    CursorStyle {
                        shape: CursorShape::Hidden,
                        ..cursor_style
                    },
                    None,
                ),
                None => return,
            };

        render_cursor(
            grid_render_params,
            cursor_display_point,
            is_cursor_on_wide_char,
            cursor_style,
            grid_render_params.size_info.padding_x_px(),
            grid_origin,
            color,
            ctx,
            terminal_view_id,
            cursor_hint_text,
            app,
        )
    }
}

fn lines_to_pixels(line: usize, cell_size: Vector2F) -> Vector2F {
    vec2f(0., line as f32 * cell_size.y())
}

fn pixels_to_lines(coord: Vector2F, cell_size: Vector2F) -> usize {
    (coord.y() / cell_size.y()) as usize
}

/// Computes the number of rows that are not visible in the current grid above the viewport.
fn hidden_rows_above(grid_origin: Vector2F, origin: Vector2F, cell_size: Vector2F) -> usize {
    ((grid_origin.y() - origin.y()) / cell_size.y())
        .neg()
        .max(0.)
        .floor() as usize
}

/// Computes the number of rows that are not visible in the grid below the viewport.
fn hidden_rows_below(
    grid_origin: Vector2F,
    origin: Vector2F,
    cell_size: Vector2F,
    bounds: RectF,
    grid_len: usize,
) -> usize {
    let grid_end = grid_origin + lines_to_pixels(grid_len, cell_size);
    let lower_left = origin.y() + bounds.height();
    pixels_to_lines(grid_end - lower_left, cell_size)
}

/// Block grids are painted from snapshots after the model lock is released. Setting
/// `PHOSPHOR_BLOCK_LIST_PAINT_UNDER_LOCK=1` paints them from the locked model as before.
pub fn block_list_paint_from_snapshot() -> bool {
    static FROM_SNAPSHOT: OnceLock<bool> = OnceLock::new();
    *FROM_SNAPSHOT.get_or_init(|| {
        std::env::var("PHOSPHOR_BLOCK_LIST_PAINT_UNDER_LOCK")
            .ok()
            .is_none_or(|flag| flag == "0")
    })
}

/// Which block grid a snapshot is of, for reuse across frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GridSnapshotKey {
    pub block_index: BlockIndex,
    pub grid: GridType,
}

struct CachedGridSnapshot {
    snapshot: Rc<GridSnapshot>,
    used_this_frame: bool,
}

/// The snapshots the last paint used, by block grid. A finished block's grid never changes, so
/// its visible rows are copied out of the model once and reused while the grid's content
/// generation and the visible row range stay the same; the active block's grid changes generation
/// with every write and is recaptured each frame.
#[derive(Default)]
pub struct GridSnapshotCache {
    entries: FxHashMap<GridSnapshotKey, CachedGridSnapshot>,
}

impl GridSnapshotCache {
    /// Forgets every snapshot no paint used since the previous call.
    pub fn begin_frame(&mut self) {
        self.entries
            .retain(|_, entry| std::mem::take(&mut entry.used_this_frame));
    }

    fn get_or_capture(
        &mut self,
        key: GridSnapshotKey,
        grid: &GridHandler,
        rows: Range<usize>,
    ) -> Option<Rc<GridSnapshot>> {
        if let Some(entry) = self.entries.get_mut(&key)
            && entry.snapshot.content_generation() == grid.content_generation()
            && entry.snapshot.rows() == rows
        {
            entry.used_this_frame = true;
            return Some(Rc::clone(&entry.snapshot));
        }
        let snapshot = Rc::new(GridSnapshot::capture(grid, rows)?);
        self.entries.insert(
            key,
            CachedGridSnapshot {
                snapshot: Rc::clone(&snapshot),
                used_this_frame: true,
            },
        );
        Some(snapshot)
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

/// A block grid paint captured by [`BlockGrid::capture_draw`]: the snapshot and every argument
/// `render_grid` needs, plus where in the scene the grid belongs.
pub struct DeferredGridDraw {
    snapshot: Rc<GridSnapshot>,
    start_row: usize,
    end_row: usize,
    grid_origin: Vector2F,
    alpha: u8,
    highlighted_url: Option<Link>,
    link_tool_tip: Option<Link>,
    hovered_secret: Option<SecretHandle>,
    ordered_matches: Option<Vec<RangeInclusive<Point>>>,
    focused_match_range: Option<RangeInclusive<Point>>,
    properties: Properties,
    visible_cursor_shape: Option<CursorShape>,
    obfuscate_secrets: ObfuscateSecrets,
    insertion_point: LayerInsertionPoint,
}

impl DeferredGridDraw {
    /// Paints the grid into a scratch scene and splices it into `ctx.scene` where it was
    /// captured, so the scene ends up as if the grid had been painted there.
    pub fn paint(
        &self,
        glyphs: &mut CellGlyphCache,
        block_grid_params: &BlockGridParams,
        ctx: &mut PaintContext,
        app: &AppContext,
    ) {
        let params = &block_grid_params.grid_render_params;
        let mut scratch = ctx.scene.scratch();
        std::mem::swap(ctx.scene, &mut scratch);
        render_grid(
            &*self.snapshot,
            self.start_row,
            self.end_row,
            &block_grid_params.colors,
            &block_grid_params.override_colors,
            &params.warp_theme,
            self.properties,
            params.font_family,
            params.font_size,
            params.line_height_ratio,
            params.cell_size,
            params.size_info.padding_x_px(),
            self.grid_origin,
            glyphs,
            self.alpha,
            self.highlighted_url.as_ref(),
            self.link_tool_tip.as_ref(),
            self.ordered_matches.as_ref().map(|matches| matches.iter()),
            self.focused_match_range.as_ref(),
            params.enforce_minimum_contrast,
            self.obfuscate_secrets,
            self.hovered_secret,
            params.use_ligature_rendering,
            self.visible_cursor_shape,
            RespectDisplayedOutput::Yes,
            // A snapshot is only taken of a grid without image placements.
            &HashMap::new(),
            None,
            params.hide_cursor_cell,
            ctx,
            app,
        );
        std::mem::swap(ctx.scene, &mut scratch);
        ctx.scene.splice_at(self.insertion_point, scratch);
    }
}

#[cfg(test)]
#[path = "blockgrid_renderer_tests.rs"]
mod tests;
