//! A running command's elapsed-time counter that ticks with paint-only repaints (#1142).
//!
//! The counter used to be a `Text` wrapped in a `LiveElement`, which asks for a
//! layout-required repaint every second: one full-window layout per second for as long as a
//! command runs, competing with typing. Here every glyph the counter can show is laid out
//! once (the counter is drawn in the monospace font, so each glyph is one advance wide), and
//! paint composes the current text from them. A tick that keeps the text's length is
//! paint-only; only a tick that changes the length (`9s` -> `10s`, `59s` -> `1m 0.00s`) asks
//! for a layout, so the element's size and the block header around it stay exact.
//!
//! `PHOSPHOR_LIVE_DURATION_PAINT_ONLY=0` restores the old `LiveElement` counter.

use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Local};
use pathfinder_color::ColorU;
use pathfinder_geometry::vector::{Vector2F, vec2f};
use warpui::elements::{Element, Point, Text};
use warpui::event::DispatchedEvent;
use warpui::fonts::{FamilyId, Properties};
use warpui::{
    AfterLayoutContext, AppContext, EventContext, LayoutContext, PaintContext, SizeConstraint,
};

use crate::terminal::model::block::Block;

/// Every character `Block::format_duration` can produce for a whole-second elapsed time.
const GLYPHS: &str = " ()0123456789.hms";

/// The kill switch: `PHOSPHOR_LIVE_DURATION_PAINT_ONLY=0` (or `false`/`off`) keeps the
/// layout-per-second `LiveElement` counter.
pub fn paint_only_counter_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        !matches!(
            std::env::var("PHOSPHOR_LIVE_DURATION_PAINT_ONLY").as_deref(),
            Ok("0") | Ok("false") | Ok("off")
        )
    })
}

/// The counter text `elapsed` after `start`, exactly as the block label shows it (whole
/// seconds, `Block::format_duration`). `None` before the first whole second.
pub fn live_duration_text(start: DateTime<Local>, now: DateTime<Local>) -> Option<String> {
    let whole_secs = now.signed_duration_since(start).num_seconds();
    (whole_secs > 0).then(|| Block::format_duration(chrono::Duration::seconds(whole_secs)))
}

/// How long until the counter's text next changes: the next whole second after `start`.
pub fn until_next_tick(start: DateTime<Local>, now: DateTime<Local>) -> Duration {
    let elapsed_ms = now.signed_duration_since(start).num_milliseconds().max(0) as u64;
    // A few ms past the boundary so the tick lands on the new second, never just before it.
    Duration::from_millis(1000 - elapsed_ms % 1000 + 5)
}

pub struct LiveDurationText {
    start: DateTime<Local>,
    family_id: FamilyId,
    font_size: f32,
    properties: Properties,
    color: ColorU,
    /// One laid-out single-glyph `Text` per character of [`GLYPHS`].
    glyphs: HashMap<char, Text>,
    /// The width of one glyph (the monospace advance).
    advance: f32,
    /// The text at the last layout; paint keeps drawing it if the length has changed since.
    laid_out_text: String,
    /// A layout was asked for since the last one (at most once per layout, so an element that
    /// is somehow not laid out again can never spin on zero-delay repaints).
    relayout_requested: bool,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl LiveDurationText {
    pub fn new(
        start: DateTime<Local>,
        family_id: FamilyId,
        font_size: f32,
        properties: Properties,
        color: ColorU,
    ) -> Self {
        Self {
            start,
            family_id,
            font_size,
            properties,
            color,
            glyphs: HashMap::new(),
            advance: 0.,
            laid_out_text: String::new(),
            relayout_requested: false,
            size: None,
            origin: None,
        }
    }

    fn current_text(&self) -> String {
        live_duration_text(self.start, Local::now()).unwrap_or_default()
    }
}

impl Element for LiveDurationText {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        ctx: &mut LayoutContext,
        app: &AppContext,
    ) -> Vector2F {
        let mut height: f32 = 0.;
        let mut advance: f32 = 0.;
        self.glyphs = GLYPHS
            .chars()
            .map(|c| {
                let mut text = Text::new_inline(c.to_string(), self.family_id, self.font_size)
                    .with_style(self.properties)
                    .with_color(self.color);
                let size = text.layout(constraint, ctx, app);
                height = height.max(size.y());
                if c.is_ascii_digit() {
                    advance = advance.max(size.x());
                }
                (c, text)
            })
            .collect();
        self.advance = advance;
        self.laid_out_text = self.current_text();
        self.relayout_requested = false;
        let size = vec2f(
            self.advance * self.laid_out_text.chars().count() as f32,
            height,
        );
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext, app: &AppContext) {
        for text in self.glyphs.values_mut() {
            text.after_layout(ctx, app);
        }
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));
        let now = Local::now();
        let current = live_duration_text(self.start, now).unwrap_or_default();
        let same_length = current.chars().count() == self.laid_out_text.chars().count();
        // A longer or shorter text needs a new size: draw what was laid out and ask for a layout
        // now; the next frame draws the new text at its new width.
        let shown = if same_length {
            current
        } else {
            self.laid_out_text.clone()
        };
        for (i, c) in shown.chars().enumerate() {
            if c == ' ' {
                continue;
            }
            if let Some(text) = self.glyphs.get_mut(&c) {
                text.paint(origin + vec2f(self.advance * i as f32, 0.), ctx, app);
            }
        }
        if same_length || self.relayout_requested {
            ctx.repaint_after_paint_only(until_next_tick(self.start, now));
        } else {
            self.relayout_requested = true;
            ctx.repaint_after(Duration::ZERO);
        }
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        _: &DispatchedEvent,
        _: &mut EventContext,
        _: &AppContext,
    ) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "live_duration_tests.rs"]
mod tests;
