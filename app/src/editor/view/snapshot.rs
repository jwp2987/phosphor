use super::model::EditorModel;
use super::{
    AutosuggestionLocation, AutosuggestionState, AutosuggestionType,
    BaselinePositionComputationMethod, Bias, DisplayPoint, DrawableSelection, ScrollState,
    ToBufferOffset, ToDisplayPoint,
};
use super::{ToCharOffset, ToPoint};
#[cfg(feature = "voice_input")]
use crate::editor::view::voice::VoiceInputState;

use crate::editor::soft_wrap::FrameLayouts;
use crate::terminal::grid_size_util::grid_compute_baseline_position_fn;

use parking_lot::Mutex;
use pathfinder_geometry::vector::{vec2f, Vector2F};

use anyhow::Result;
use core::f32;
use instant::Instant;
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::time::Duration;
use std::{
    cmp::{self},
    ops::Range,
    sync::Arc,
};
use warp_completer::completer::Description;
use warpui::text::point::Point;

use string_offset::ByteOffset;

use warpui::fonts::{FamilyId, Properties};
use warpui::platform::LineStyle;
use warpui::text_layout::{
    default_compute_baseline_position_fn, ClipConfig, ComputeBaselinePositionFn, StyleAndFont,
    TextAlignment, TextStyle, DEFAULT_TOP_BOTTOM_RATIO,
};
use warpui::EntityId;
use warpui::{
    fonts::Cache as FontCache,
    text_layout::{self, LayoutCache},
    AppContext, ModelHandle,
};

/// Ratio to calculate font size of cursor avatar.
/// Found experimentally to scale the best proportionally with
/// current font size and the avatar's size.
pub const CURSOR_AVATAR_FONT_RATIO: f32 = 0.8;
/// Offset to calculate size of cursor avatar.
/// Found experimentally to look the best with current font size.
pub const CURSOR_AVATAR_IMAGE_OFFSET: f32 = 4.;

/// Fudge factor to make the voice input icon slightly wider than it is tall.
const VOICE_INPUT_ICON_IMAGE_OFFSET_X: f32 = 5.;

/// Minimum size of voice input icon.
const MIN_VOICE_INPUT_ICON_SIZE: f32 = 16.;

/// Gap between voice input icon's botton and cursor's top.
pub const VOICE_INPUT_ICON_CURSOR_GAP: f32 = 2.;

/// The amount of time the editor height must have remained shrunken
/// before we actually shrink the height. This is to prevent jittering
/// before an autosuggestion is computed on keypress, if the autosuggestion would wrap
/// and cause the editor height to grow.
const EDITOR_HEIGHT_SHRINK_DELAY: Duration = Duration::from_millis(25);

/// A read-only snapshot of the [`EditorView`] that is needed
/// to render the [`EditorElement`].
pub struct ViewSnapshot {
    pub view_id: EntityId,
    pub is_focused: bool,
    pub editor_model: ModelHandle<EditorModel>,

    pub can_select: bool,

    pub font_size: f32,
    pub font_family: FamilyId,
    pub placeholder_font_family: FamilyId,
    pub font_properties: Properties,
    pub line_height: f32,
    pub line_height_ratio: f32,
    pub em_width: f32,

    pub autogrow: bool,
    pub is_empty: bool,

    /// Map from prefix to placeholder text. The view can have multiple active prefixes that when
    /// exactly matched in the buffer cause additional placeholder ghost text to be displayed. The
    /// empty string prefix "" is the default placeholder (shown when buffer is empty).
    pub placeholder_texts: Arc<HashMap<String, String>>,

    pub autosuggestion_state: Option<Arc<AutosuggestionState>>,
    pub command_xray: Option<Arc<Description>>,

    pub cached_buffer_points: HashMap<Cow<'static, str>, Point>,

    pub baseline_position_computation_method: BaselinePositionComputationMethod,

    #[cfg(feature = "voice_input")]
    pub voice_input_state: VoiceInputState,

    pub editor_height_shrink_delay: Arc<Mutex<EditorHeightShrinkDelay>>,
}

/// A struct to hold the editor height before it was shrunk and the time it was first shrunk.
/// This is used to delay shrinking the editor height until after it's remained shrunken for EDITOR_HEIGHT_SHRINK_DELAY.
pub struct EditorHeightShrinkDelay {
    pub editor_height_before_shrink: f32,
    pub editor_height_shrink_start: Option<Instant>,
}

impl ViewSnapshot {
    /// Returns the editor height with the EDITOR_HEIGHT_SHRINK_DELAY applied, and updates internal state.
    pub fn get_editor_height_with_shrink_delay(&self, editor_height: f32) -> f32 {
        let mut editor_height_shrink_delay = self.editor_height_shrink_delay.lock();
        // If the editor_height stayed the same or grew, use editor_height and reset the shrink time.
        if editor_height >= editor_height_shrink_delay.editor_height_before_shrink {
            editor_height_shrink_delay.editor_height_before_shrink = editor_height;
            editor_height_shrink_delay.editor_height_shrink_start = None;
            return editor_height;
        }
        // From here we know the editor height shrank.
        // If there's no start time recorded, this is the first render where the editor height shrunk.
        // Record the current time but use the height before shrinking.
        let Some(editor_height_shrink_start) =
            editor_height_shrink_delay.editor_height_shrink_start
        else {
            editor_height_shrink_delay.editor_height_shrink_start = Some(Instant::now());
            return editor_height_shrink_delay.editor_height_before_shrink;
        };

        // If the height has been shrunken for a while, use the latest shrunken height.
        // It is our new baseline.
        if editor_height_shrink_start.elapsed() > EDITOR_HEIGHT_SHRINK_DELAY {
            editor_height_shrink_delay.editor_height_before_shrink = editor_height;
            editor_height_shrink_delay.editor_height_shrink_start = None;
            editor_height
        } else {
            editor_height_shrink_delay.editor_height_before_shrink
        }
    }

    /// Returns the time at which the editor height should be repainted with the shrunken editor height.
    pub fn get_editor_repaint_at(&self) -> Option<Instant> {
        self.editor_height_shrink_delay
            .lock()
            .editor_height_shrink_start
            .map(|start| start + EDITOR_HEIGHT_SHRINK_DELAY)
    }

    pub fn baseline_position_fn(&self) -> ComputeBaselinePositionFn {
        // Copy the font family ID so that it can be moved into the closure.
        let font_family = self.font_family;
        match self.baseline_position_computation_method {
            BaselinePositionComputationMethod::Grid => {
                grid_compute_baseline_position_fn(font_family)
            }
            BaselinePositionComputationMethod::Default => default_compute_baseline_position_fn(),
        }
    }

    /// Returns the placeholder text for the prefix that matches the current buffer content.
    pub fn matching_placeholder_text(&self, buffer_text: &str) -> Option<String> {
        // Find exact match - buffer content must equal the prefix exactly
        self.placeholder_texts
            .iter()
            .find(|(prefix, _)| buffer_text == prefix.as_str())
            .map(|(_, text)| text.clone())
    }

    pub fn placeholder_text_exists(&self) -> bool {
        // Only consider the default placeholder (empty prefix) for "exists" checks.
        // This is used when laying out the input and determining input height.
        self.placeholder_texts.contains_key("")
    }

    pub fn is_selecting(&self, app: &AppContext) -> bool {
        self.editor_model.as_ref(app).is_selecting(app)
    }

    pub fn rightmost_point(&self, app: &AppContext) -> DisplayPoint {
        self.editor_model
            .as_ref(app)
            .display_map(app)
            .rightmost_point()
    }

    pub fn max_point(&self, app: &AppContext) -> DisplayPoint {
        self.editor_model.as_ref(app).max_point(app)
    }

    pub fn autosuggestion_location(&self) -> Option<AutosuggestionLocation> {
        self.autosuggestion_state
            .as_ref()
            .map(|state| state.location)
    }

    pub fn active_autosuggestion(&self) -> bool {
        self.autosuggestion_state
            .as_ref()
            .is_some_and(|s| s.is_active())
    }

    pub fn active_next_command_suggestion(&self) -> bool {
        self.autosuggestion_state.as_ref().is_some_and(|s| {
            s.is_active()
                && matches!(
                    s.autosuggestion_type,
                    AutosuggestionType::Command {
                        was_intelligent_autosuggestion: true
                    }
                )
        })
    }

    /// Lays out the given ghosted text, which can be a placeholder or autosuggestion.
    ///
    /// Text over [`MAX_GHOST_TEXT_BYTES`] is cut and ends in an ellipsis, so no ghost or placeholder
    /// path can hand the layout engine an unbounded string on the main thread (#1398).
    fn layout_ghosted_text(
        &self,
        text: &str,
        size: &Vector2F,
        soft_wrap: bool,
        first_line_head_indent: Option<f32>,
        font_cache: &FontCache,
        layout_cache: &LayoutCache,
    ) -> Vec<Arc<text_layout::Line>> {
        let font_size = self.font_size;
        let bounded = bounded_ghost_text(text);
        let text = &*bounded;

        if soft_wrap {
            let text_frame = layout_cache.layout_text(
                text,
                LineStyle {
                    font_size: self.font_size,
                    line_height_ratio: self.line_height_ratio,
                    baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                    fixed_width_tab_size: None,
                },
                &[(
                    0..text.chars().count(),
                    StyleAndFont::new(
                        self.placeholder_font_family,
                        self.font_properties,
                        TextStyle::new(),
                    ),
                )],
                size.x(),
                f32::MAX,
                Default::default(),
                first_line_head_indent,
                &font_cache.text_layout_system(),
            );

            // Return a vec of lines to be backward compatible with laying out placeholder
            // without soft_wrapping and autosuggestion. In the future we may want to
            // return a TextFrame here so we don't have to clone the text_frame lines
            text_frame
                .lines()
                .iter()
                .map(|l| Arc::new(l.clone()))
                .collect()
        } else {
            text.lines()
                .map(|line| {
                    layout_cache.layout_line(
                        line,
                        LineStyle {
                            font_size,
                            line_height_ratio: self.line_height_ratio,
                            baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                            fixed_width_tab_size: None,
                        },
                        &[(
                            0..line.chars().count(),
                            StyleAndFont::new(
                                self.placeholder_font_family,
                                self.font_properties,
                                TextStyle::new(),
                            ),
                        )],
                        f32::MAX,
                        ClipConfig::default(),
                        &font_cache.text_layout_system(),
                    )
                })
                .collect()
        }
    }

    pub fn layout_autosuggestion(
        &self,
        preceding_text_width: f32,
        font_cache: &FontCache,
        layout_cache: &LayoutCache,
        size: &Vector2F,
        soft_wrap: bool,
    ) -> Vec<Arc<text_layout::Line>> {
        // The autosuggestion is laid out on the same line as editor text and then soft wraps.
        // This means the first line of the autosuggestion has preceding_text_width less width to use before it needs to wrap.
        self.autosuggestion_state
            .as_ref()
            .and_then(|state| state.current_autosuggestion_text.as_ref())
            .map(|current_autosuggestion_text| {
                self.layout_ghosted_text(
                    current_autosuggestion_text,
                    size,
                    soft_wrap,
                    Some(preceding_text_width),
                    font_cache,
                    layout_cache,
                )
            })
            .unwrap_or_default()
    }

    /// Layout placeholder text with an optional indent for the first line.
    pub fn layout_placeholder_text(
        &self,
        placeholder_text: &str,
        first_line_indent: f32,
        font_cache: &FontCache,
        layout_cache: &LayoutCache,
        size: &Vector2F,
        soft_wrap: bool,
    ) -> Vec<Arc<text_layout::Line>> {
        self.layout_ghosted_text(
            placeholder_text,
            size,
            soft_wrap,
            if first_line_indent > 0. {
                Some(first_line_indent)
            } else {
                None
            },
            font_cache,
            layout_cache,
        )
    }

    pub(super) fn clamp_scroll_left(&self, scroll_state: &ScrollState, max: f32) {
        let mut scroll_position = scroll_state.scroll_position.lock();
        let scroll_left = scroll_position.x();
        scroll_position.set_x(scroll_left.min(max));
    }

    pub fn max_scroll_top(total_lines: f32, visible_lines: f32) -> f32 {
        (total_lines - visible_lines).max(0.)
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn autoscroll_horizontally(
        &self,
        scroll_state: &ScrollState,
        start_row: u32,
        viewport_width: f32,
        scroll_width: f32,
        max_glyph_width: f32,
        layouts: Vec<&text_layout::Line>,
        app: &AppContext,
    ) {
        let map = self.editor_model.as_ref(app).display_map(app);

        let mut target_left = f32::INFINITY;
        let mut target_right = 0.0_f32;
        for selection in self.editor_model.as_ref(app).selections(app) {
            let head = selection.head().to_display_point(map, app).unwrap();
            let start_column = head.column().saturating_sub(3);
            let end_column = cmp::min(map.line_len(head.row(), app).unwrap(), head.column() + 3);

            if let Some(line) = layouts.get((head.row() - start_row) as usize) {
                target_left = target_left.min(line.x_for_index(start_column as usize));
            }
            if let Some(line) = layouts.get((head.row() - start_row) as usize) {
                target_right =
                    target_right.max(line.x_for_index(end_column as usize) + max_glyph_width);
            }
        }
        target_right = target_right.min(scroll_width);

        if target_right - target_left > viewport_width {
            return;
        }

        let mut scroll_position = scroll_state.scroll_position.lock();
        let scroll_left = scroll_position.x() * max_glyph_width;
        let scroll_right = scroll_left + viewport_width;

        if target_left < scroll_left {
            scroll_position.set_x(target_left / max_glyph_width);
        } else if target_right > scroll_right {
            scroll_position.set_x((target_right - viewport_width) / max_glyph_width);
        }
    }

    pub(super) fn autoscroll_vertically(
        &self,
        scroll_state: &ScrollState,
        total_lines: f32,
        visible_lines: f32,
        top_section_height_lines: f32,
        frame_layouts: &FrameLayouts,
        app: &AppContext,
    ) -> bool {
        let mut scroll_position = scroll_state.scroll_position.lock();
        let scroll_top = scroll_position.y();
        scroll_position.set_y(scroll_top.min(total_lines - visible_lines).max(0.));

        let mut autoscroll_requested = scroll_state.autoscroll_requested.lock();
        if *autoscroll_requested {
            *autoscroll_requested = false;
        } else {
            return false;
        }

        let map = self.editor_model.as_ref(app).display_map(app);

        let first_selection = self.editor_model.as_ref(app).first_selection(app);
        let first_selection_clamp_direction = first_selection.clamp_direction;
        let first_cursor = first_selection.head().to_display_point(map, app);

        let first_cursor_top = match first_cursor {
            Ok(first_cursor) => {
                match frame_layouts
                    .to_soft_wrap_point(first_cursor, first_selection_clamp_direction)
                {
                    Some(point) => point.row() as f32 + top_section_height_lines,
                    None => {
                        log::error!("Failed to get softwrapped point from display point");
                        return false;
                    }
                }
            }
            Err(err) => {
                log::error!("Error trying to turn selection into display point {err:?}");
                return false;
            }
        };

        let last_selection = self.editor_model.as_ref(app).last_selection(app);
        let last_selection_clamp_direction = last_selection.clamp_direction;
        let last_cursor = last_selection.head().to_display_point(map, app);

        let last_cursor_bottom = match last_cursor {
            Ok(last_cursor) => {
                match frame_layouts.to_soft_wrap_point(last_cursor, last_selection_clamp_direction)
                {
                    Some(point) => point.row() as f32 + 1.0 + top_section_height_lines,
                    None => {
                        log::error!("Failed to get softwrapped point from display point");
                        return false;
                    }
                }
            }
            Err(err) => {
                log::error!("Error trying to turn selection into display point {err:?}");
                return false;
            }
        };

        let margin = ((visible_lines - (last_cursor_bottom - first_cursor_top)) / 2.0)
            .floor()
            .min(3.0);
        if margin < 0.0 {
            return false;
        }

        let target_top = (first_cursor_top - margin).max(0.0);
        let target_bottom = last_cursor_bottom + margin;
        let start_row = scroll_position.y();
        let end_row = start_row + visible_lines;

        if target_top < start_row {
            scroll_position.set_y(target_top.min(Self::max_scroll_top(total_lines, visible_lines)));
        } else if target_bottom >= end_row {
            scroll_position.set_y(
                (target_bottom - visible_lines)
                    .min(Self::max_scroll_top(total_lines, visible_lines)),
            );
        }
        true
    }

    /// If the range of rows passed does not exist, we just layout the rows that do exist
    /// within the provided range.
    pub fn layout_text_frames(
        &self,
        rows: Range<u32>,
        layout_cache: &LayoutCache,
        max_width: f32,
        left_notch_width_px: f32,
        app: &AppContext,
    ) -> Result<Vec<Arc<text_layout::TextFrame>>> {
        self.layout_text_frames_windowed(
            rows,
            None,
            layout_cache,
            max_width,
            left_notch_width_px,
            app,
        )
    }

    /// As [`Self::layout_text_frames`], but with `real_rows` set only rows inside one of the
    /// ranges are laid out; every other row gets an empty one-line frame (#1191). That is only
    /// right when each of those rows is exactly one visual line and nothing reads their glyphs
    /// (see [`large_buffer_real_rows`]); the caller guarantees both.
    pub fn layout_text_frames_windowed(
        &self,
        rows: Range<u32>,
        real_rows: Option<&[Range<u32>]>,
        layout_cache: &LayoutCache,
        max_width: f32,
        left_notch_width_px: f32,
        app: &AppContext,
    ) -> Result<Vec<Arc<text_layout::TextFrame>>> {
        let frames = self.layout_text_frames_inner(
            rows.clone(),
            real_rows,
            layout_cache,
            max_width,
            left_notch_width_px,
            app,
        )?;
        // The windowing rests on every row being one visual line (a row index is then its line
        // index). The caller proved it from font-independent bounds; this checks the proof on the
        // rows that were really laid out, and if one wrapped after all, throws the window away
        // and lays out every row, as before the windowing existed.
        if let Some(window) = real_rows {
            let a_real_row_wrapped = frames.iter().enumerate().any(|(index, frame)| {
                row_is_laid_out(Some(window), rows.start + index as u32) && frame.lines().len() != 1
            });
            if a_real_row_wrapped {
                log::warn!("a row wrapped in a windowed editor layout; laying out every row");
                return self.layout_text_frames_inner(
                    rows,
                    None,
                    layout_cache,
                    max_width,
                    left_notch_width_px,
                    app,
                );
            }
        }
        Ok(frames)
    }

    fn layout_text_frames_inner(
        &self,
        mut rows: Range<u32>,
        real_rows: Option<&[Range<u32>]>,
        layout_cache: &LayoutCache,
        max_width: f32,
        left_notch_width_px: f32,
        app: &AppContext,
    ) -> Result<Vec<Arc<text_layout::TextFrame>>> {
        let display_map = self.editor_model.as_ref(app).display_map(app);

        rows.end = cmp::min(rows.end, display_map.max_point(app).row() + 1);
        if rows.start >= rows.end {
            return Ok(Vec::new());
        }

        let family_id = self.font_family;
        let properties = self.font_properties;
        let font_size = self.font_size;

        // Collect style information for lines. A row longer than `MAX_LAID_OUT_ROW_CHARS` is laid
        // out only up to that cap (#955/#968): CoreText line breaking and shaping both cost
        // seconds-to-minutes on a multi-megabyte row. The row's true char count is kept so the
        // unlaid tail can be re-attached to the frame (see `TextFrame::with_unlaid_tail`); the
        // buffer itself is never truncated.
        let rows_start = rows.start;
        let stylized_lines = rows
            .map(|row| {
                if !row_is_laid_out(real_rows, row) {
                    return (Vec::new(), 0, None);
                }
                let mut chars = display_map
                    .chars_with_styles_at(DisplayPoint::new(row, 0), app)
                    .unwrap()
                    // Stop when we hit the end of the row.
                    .take_while(|c| c.char() != '\n')
                    .take(MAX_LAID_OUT_ROW_CHARS)
                    .collect::<Vec<_>>();
                let true_len = if chars.len() == MAX_LAID_OUT_ROW_CHARS {
                    display_map
                        .line_len(row, app)
                        .map_or(chars.len(), |len| len as usize)
                } else {
                    chars.len()
                };
                // #1036: lay out the START and the END of an over-long row, not only its
                // start. Text pasted or typed at the end of a huge row is where the cursor is;
                // laying out only the head left it invisible until Ctrl-End. The middle is
                // elided and the tail keeps its true offsets (`with_elided_middle`).
                let mut elided_head = None;
                if true_len > MAX_LAID_OUT_ROW_CHARS {
                    let head = MAX_LAID_OUT_ROW_CHARS - ROW_TAIL_LAID_OUT_CHARS;
                    let tail = display_map
                        .chars_with_styles_at(
                            DisplayPoint::new(row, (true_len - ROW_TAIL_LAID_OUT_CHARS) as u32),
                            app,
                        )
                        .map(|chars| {
                            chars
                                .take_while(|c| c.char() != '\n')
                                .take(ROW_TAIL_LAID_OUT_CHARS)
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if tail.len() == ROW_TAIL_LAID_OUT_CHARS {
                        chars.truncate(head);
                        chars.extend(tail);
                        elided_head = Some(head);
                    }
                }
                (chars, true_len, elided_head)
            })
            .collect::<Vec<_>>();

        let font_cache = app.font_cache();

        let line_height_ratio = self.line_height_ratio;
        let first_row = rows_start;

        let placeholder_frame =
            Arc::new(text_layout::TextFrame::empty(font_size, line_height_ratio));

        // Use rayon parallel iterators to efficiently lay out all of the lines.
        let text_layout_system = font_cache.text_layout_system();
        let layouts = stylized_lines
            .into_par_iter()
            .enumerate()
            .map(|(cur_line_idx, (stylized_chars, true_len, elided_head))| {
                if !row_is_laid_out(real_rows, first_row + cur_line_idx as u32) {
                    // One shared frame: tens of thousands of rows would each allocate their own.
                    return (Arc::clone(&placeholder_frame), None, None);
                }
                let laid_chars = stylized_chars.len();
                let (line, style_runs) = line_and_style_runs(
                    stylized_chars
                        .into_iter()
                        .map(|stylized_char| (stylized_char.char(), stylized_char.style())),
                    family_id,
                    properties,
                );

                let frame = layout_cache.layout_text(
                    &line,
                    LineStyle {
                        font_size,
                        line_height_ratio,
                        baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                        fixed_width_tab_size: None,
                    },
                    &style_runs,
                    max_width,
                    f32::MAX,
                    TextAlignment::default(),
                    // Push over text by left notch width, if we're on the first line. Note notch width will be 0 if same line prompt is disabled.
                    (cur_line_idx == 0).then_some(left_notch_width_px),
                    &text_layout_system,
                );
                let elided = elided_head.map(|head| (head, laid_chars, true_len));
                let capped =
                    (elided.is_none() && true_len > laid_chars).then_some((laid_chars, true_len));
                (frame, capped, elided)
            })
            .collect::<Vec<_>>();

        // Re-attach the unlaid tail of capped rows serially: the font cache's glyph lookup is not
        // `Sync`, so it can't be used inside the parallel layout above.
        let layouts = layouts
            .into_iter()
            .map(|(frame, capped, elided)| {
                if let Some((head, laid_len, true_len)) = elided {
                    return Arc::new(frame.with_elided_middle(head, laid_len, true_len));
                }
                match capped {
                    Some((laid_chars, true_len)) => {
                        Arc::new(frame.with_unlaid_tail(laid_chars, true_len, |font_id| {
                            font_cache
                                .glyph_for_char(font_id, ' ', false)
                                .map(|(glyph_id, _)| glyph_id)
                        }))
                    }
                    None => frame,
                }
            })
            .collect();

        Ok(layouts)
    }

    pub fn line(&self, display_row: u32, app: &AppContext) -> Result<String> {
        self.editor_model
            .as_ref(app)
            .display_map(app)
            .line(display_row, app)
    }

    pub fn all_drawable_selections_intersecting_range<'a>(
        &'a self,
        range: Range<DisplayPoint>,
        app: &'a AppContext,
    ) -> impl 'a + Iterator<Item = DrawableSelection> {
        self.editor_model
            .as_ref(app)
            .all_drawable_selections_intersecting_range(range, app)
    }

    // todo: dedup this with EditorView. it's really: given an editor model + row, get the line len
    pub fn line_len(&self, display_row: u32, app: &AppContext) -> Result<u32> {
        self.editor_model.as_ref(app).line_len(display_row, app)
    }

    pub fn layout_line(
        &self,
        row: u32,
        text_layout_cache: &LayoutCache,
        app: &AppContext,
    ) -> Result<Arc<text_layout::Line>> {
        let font_cache = app.font_cache();
        let line = self.line(row, app)?;

        Ok(text_layout_cache.layout_line(
            &line,
            LineStyle {
                font_size: self.font_size,
                line_height_ratio: self.line_height_ratio,
                baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                fixed_width_tab_size: None,
            },
            &[(
                0..self.line_len(row, app)? as usize,
                StyleAndFont::new(self.font_family, Default::default(), TextStyle::new()),
            )],
            f32::MAX,
            ClipConfig::default(),
            &font_cache.text_layout_system(),
        ))
    }

    /// Finds the byte offset directly under the given point
    pub fn byte_offset_at_point(
        &self,
        point: &DisplayPoint,
        app: &AppContext,
    ) -> Option<ByteOffset> {
        let model = self.editor_model.as_ref(app);
        let buffer = model.buffer(app);
        let map = model.display_map(app);
        point
            .to_buffer_point(map, Bias::Left, app)
            .and_then(|point| point.to_byte_offset(buffer))
            .ok()
    }

    pub fn display_point_at_byte_offset(
        &self,
        byte_offset: &ByteOffset,
        app: &AppContext,
    ) -> Option<DisplayPoint> {
        let model = self.editor_model.as_ref(app);
        let buffer = model.buffer(app);
        let map = model.display_map(app);
        let char_offset = byte_offset.to_char_offset(buffer).ok()?;
        let point = char_offset.to_point(buffer).ok()?;
        point.to_display_point(map, app).ok()
    }

    pub fn vim_visual_tails<'a>(
        &self,
        app: &'a AppContext,
    ) -> impl Iterator<Item = DisplayPoint> + 'a + use<'a> {
        let editor_model = self.editor_model.as_ref(app);
        let map = editor_model.display_map(app);
        editor_model
            .vim_visual_tails()
            .iter()
            .filter_map(|anchor| anchor.to_display_point(map, app).ok())
    }

    /// Returns the font size for a cursor avatar. Value is based on the snapshot's
    /// font size and an avatar-specific ratio.
    pub fn cursor_avatar_font_size(&self) -> f32 {
        self.font_size * CURSOR_AVATAR_FONT_RATIO
    }

    /// Returns the size (diameter) for a cursor avatar. Value is based on the snapshot's
    /// font size and an avatar-specific offset.
    pub fn cursor_avatar_size(&self) -> f32 {
        self.font_size + CURSOR_AVATAR_IMAGE_OFFSET
    }

    /// Returns the size to render the voice input icon at, scaled by the font size.
    pub fn voice_input_icon_size(&self) -> Vector2F {
        let scaled_size = (self.font_size + 1.).max(MIN_VOICE_INPUT_ICON_SIZE);
        vec2f(scaled_size + VOICE_INPUT_ICON_IMAGE_OFFSET_X, scaled_size)
    }
}

/// Buffers with at least this many rows lay out only the rows near the viewport, the cursor and
/// the ends (#1191). Below it every row is laid out, as it always was.
pub(crate) const LARGE_BUFFER_ROWS: u32 = 2000;

/// Rows laid out above and below the viewport and around each selection end in a large buffer,
/// so paging and cursor moves land on real layouts. At least this, and four viewports.
pub(crate) const OFFSCREEN_ROW_MARGIN: u32 = 300;

/// Whether a `PHOSPHOR_EDITOR_FULL_LAYOUT` value forces the full layout of every row (the
/// kill switch for the windowed layout, #1191): `1`, `true`, `yes` or `on`, any case.
pub(crate) fn full_layout_flag_enabled(value: Option<&str>) -> bool {
    value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

/// `PHOSPHOR_EDITOR_FULL_LAYOUT=1` turns the windowed layout off. Read once.
pub(crate) fn editor_full_layout_forced() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| {
        full_layout_flag_enabled(std::env::var("PHOSPHOR_EDITOR_FULL_LAYOUT").ok().as_deref())
    })
}

/// Whether `row` gets a real layout: always when `real_rows` is `None`.
fn row_is_laid_out(real_rows: Option<&[Range<u32>]>, row: u32) -> bool {
    real_rows.is_none_or(|ranges| ranges.iter().any(|range| range.contains(&row)))
}

/// Whether no row can wrap, given the length in chars of the longest row: even if every one of
/// its chars were as wide as the font's widest glyph, the row would fit in `max_width`. The
/// advance bound is 1 em for ASCII text (no common font has a wider ASCII glyph) and 1.5 em
/// otherwise (wide and emoji glyphs). A tab advances to the next tab stop, so any tab gives up.
pub(crate) fn rows_cannot_wrap(
    longest_row_chars: u32,
    all_ascii: bool,
    tab_free: bool,
    font_size: f32,
    max_width: f32,
) -> bool {
    rows_cannot_wrap_measured(
        longest_row_chars,
        all_ascii,
        tab_free,
        font_size,
        max_width,
        None,
    )
}

/// As [`rows_cannot_wrap`], but an all-ASCII row is bounded by `ascii_advance_ems` (the widest
/// ASCII advance of the real font, in ems, see [`widest_ascii_advance_ems`]) instead of the
/// 1 em worst case. A monospace font is ~0.6 em, so the 1 em bound failed for any 100-char row in
/// an ordinary window and left the whole 1 MB buffer fully laid out on every frame (#1333).
pub(crate) fn rows_cannot_wrap_measured(
    longest_row_chars: u32,
    all_ascii: bool,
    tab_free: bool,
    font_size: f32,
    max_width: f32,
    ascii_advance_ems: Option<f32>,
) -> bool {
    if !tab_free || !font_size.is_finite() || font_size <= 0. || !max_width.is_finite() {
        return false;
    }
    let max_advance_ems = if all_ascii {
        match ascii_advance_ems {
            Some(ems) if ems.is_finite() && ems > 0. => ems.min(1.0),
            _ => 1.0,
        }
    } else {
        1.5
    };
    longest_row_chars as f32 * font_size * max_advance_ems <= max_width
}

/// Safety factor on the measured widest ASCII advance, for rounding and hinting differences
/// between the advance a glyph reports and the one the shaper lays out. (Kerning and ligatures
/// only narrow ASCII text; and the layout re-checks the rows it really lays out.)
const ASCII_ADVANCE_SAFETY: f32 = 1.15;

/// The widest advance, in ems times [`ASCII_ADVANCE_SAFETY`], among the printable ASCII glyphs of
/// the font the editor draws with. `None` when any of them is missing (a fallback font could then
/// be wider) or has no usable advance, which keeps the 1 em bound.
pub(crate) fn widest_ascii_advance_ems(
    font_cache: &warpui::fonts::Cache,
    family_id: FamilyId,
    properties: Properties,
    font_size: f32,
) -> Option<f32> {
    if !font_size.is_finite() || font_size <= 0. {
        return None;
    }
    let font_id = font_cache.select_font(family_id, properties);
    let mut widest = 0f32;
    for ch in ' '..='~' {
        let (glyph_id, glyph_font) = font_cache.glyph_for_char(font_id, ch, false)?;
        if glyph_font != font_id {
            return None;
        }
        let advance = font_cache
            .glyph_advance(font_id, font_size, glyph_id)
            .ok()?;
        if !advance.x().is_finite() {
            return None;
        }
        widest = widest.max(advance.x());
    }
    (widest > 0.).then(|| widest / font_size * ASCII_ADVANCE_SAFETY)
}

/// The row ranges of a `total_rows` buffer that need a real layout when it is large (#1191):
/// the viewport (`visible`, in rows) plus a margin, the first and last rows, and a margin around
/// every row in `selection_rows`. `None` when the buffer is below [`LARGE_BUFFER_ROWS`]. Only
/// valid when every row is one visual line, so a row index is its visual line.
pub(crate) fn large_buffer_real_rows(
    total_rows: u32,
    visible: Range<u32>,
    selection_rows: &[u32],
) -> Option<Vec<Range<u32>>> {
    if total_rows < LARGE_BUFFER_ROWS {
        return None;
    }
    let viewport_rows = visible.end.saturating_sub(visible.start);
    let margin = OFFSCREEN_ROW_MARGIN.max(viewport_rows.saturating_mul(4));
    let edge = viewport_rows.saturating_add(64);
    let around = |row: u32| row.saturating_sub(margin)..row.saturating_add(margin);
    let mut ranges = vec![
        visible.start.saturating_sub(margin)..visible.end.saturating_add(margin),
        0..edge,
        total_rows.saturating_sub(edge)..total_rows,
    ];
    ranges.extend(selection_rows.iter().copied().map(around));
    Some(ranges)
}

/// The most characters of one buffer row that are handed to the platform text layout (#955,
/// #968). Rows past it are laid out up to the cap and keep true offsets via
/// `TextFrame::with_unlaid_tail`. Large enough for the #864 30k-char single line.
pub(crate) const MAX_LAID_OUT_ROW_CHARS: usize = 32 * 1024;

/// Of the [`MAX_LAID_OUT_ROW_CHARS`] laid out for an over-long row, how many come from the END of
/// the row (#1036), so the cursor and what is typed after a huge paste are visible.
pub(crate) const ROW_TAIL_LAID_OUT_CHARS: usize = 4 * 1024;

/// Collects one row's characters into the string to lay out, together with the style
/// runs (char-index ranges, as `layout_text` expects) that describe it.
///
/// Adjacent characters with the same style extend the current run instead of each
/// becoming a run of their own. The layout cache hashes every run to key its entries
/// and the text layout system shapes run by run, so one run per character made both
/// linear in the row length on every frame -- for a prompt that is normally a single
/// style from end to end (#813).
fn line_and_style_runs(
    stylized_chars: impl IntoIterator<Item = (char, TextStyle)>,
    family_id: FamilyId,
    properties: Properties,
) -> (String, Vec<(Range<usize>, StyleAndFont)>) {
    let stylized_chars = stylized_chars.into_iter();
    let (expected_len, _) = stylized_chars.size_hint();
    let mut line = String::with_capacity(expected_len);
    // Pre-size for the common case of a row with one style; a row with a second
    // style grows this once, which is still far fewer allocations than a run per
    // character.
    let mut style_runs: Vec<(Range<usize>, StyleAndFont)> = Vec::with_capacity(1);
    for (idx, (ch, style)) in stylized_chars.enumerate() {
        let style_and_font = StyleAndFont::new(family_id, properties, style);
        match style_runs.last_mut() {
            Some((run, run_style)) if *run_style == style_and_font => run.end = idx + 1,
            _ => style_runs.push((idx..idx + 1, style_and_font)),
        }
        line.push(ch);
    }
    (line, style_runs)
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod tests;

/// The most ghost text (an autosuggestion's remaining suffix, a placeholder) the editor offers or
/// lays out, in bytes (#1398). A 1.3 MB history entry as the suggestion for its first two letters
/// froze the Mac UI for ~76 s in text layout (ICU line breaking re-scans the string at every
/// break). 4 KB is far above any command worth completing from a ghost (a long one-liner, a
/// multi-line script of ~50 lines) and costs well under a millisecond to lay out; a suggestion
/// longer than that is not offered at all, since accepting a truncated one would insert a
/// command the user never saw in full.
pub(crate) const MAX_GHOST_TEXT_BYTES: usize = 4096;

/// `text`, cut at the last char boundary within [`MAX_GHOST_TEXT_BYTES`] and ended with an
/// ellipsis when longer. Defence in depth behind the autosuggestion cap in `set_autosuggestion`.
pub(super) fn bounded_ghost_text(text: &str) -> Cow<'_, str> {
    if text.len() <= MAX_GHOST_TEXT_BYTES {
        return Cow::Borrowed(text);
    }
    let mut end = MAX_GHOST_TEXT_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    Cow::Owned(format!("{}\u{2026}", &text[..end]))
}
