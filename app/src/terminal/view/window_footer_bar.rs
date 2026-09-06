//! The window footer bar: a fixed-height strip along the bottom of every terminal
//! window, present whether or not it currently has anything to say.
//!
//! See `docs/DESIGN-PHOSPHOR-FORK.md` §8 for why this exists. The short version:
//! chrome that sometimes appears below a running program cannot be reserved for,
//! measured, or scrolled around without breaking something else -- seven attempts
//! were written and reverted, each failing differently. A *permanent* surface can
//! simply be subtracted, because `rows = pane - bar` is then a constant: there is
//! no resize when chrome appears, no measurement of a previous frame, and no
//! predicate anywhere near the row calculation.
//!
//! **The load-bearing property is that the bar is always present.** Adding a
//! predicate that makes it conditional reintroduces every failure mode in the §8
//! table -- in particular the last one, where a constant folded into an
//! unconditionally applied total cost every window its rows whether the bar was
//! shown or not.
//!
//! # How the rows are reserved
//!
//! Not arithmetically. The bar is a column sibling of the whole terminal content
//! area (see `TerminalView::render`), and the content area is the flex-shrinkable
//! child of that column, so the layout engine hands the content `pane - bar`. The
//! `TerminalSizeElement` that reports the pty size sits *inside* that shrinkable
//! child in both blocklist and alt-screen mode, so the size it reports has the
//! bar's pixels removed exactly once, by layout, in both modes. Nothing in
//! `create_size_info` / `create_size_info_for_blocklist` needs to know about the
//! bar, which is what keeps alt screen from double-counting it -- including the
//! `ALT_SCREEN_APPS_THAT_MUST_MATCH_BLOCKLIST_PADDING` apps (`k9s`, `lazygit`)
//! that take the blocklist padding while in alt screen.
//!
//! **Nothing subtracts the bar arithmetically, deliberately.** Session creation
//! (`terminal_manager::compute_block_size`) derives a `SizeInfo` from the pane size
//! before any layout has run, so it seeds the model a couple of rows too tall — and
//! the first layout pass corrects it through the same `TerminalSizeElement` path any
//! window resize uses. An arithmetic subtraction there was written and reverted: it
//! broke `pane_group/mod_tests.rs`'s `test_initial_widths_are_computed_correctly`,
//! which asserts `pane_height_px()` against values derived outside the terminal, and
//! it reintroduced a second derivation of one geometry — the hazard
//! `cell_size_and_padding`'s comment already warns about. One derivation, in layout.

use warpui::elements::{Empty, Point};
use warpui::event::DispatchedEvent;
use warpui::geometry::rect::RectF;
use warpui::geometry::vector::{Vector2F, vec2f};
use warpui::{
    AfterLayoutContext, AppContext, ClipBounds, Element, EventContext, LayoutContext, PaintContext,
    SizeConstraint,
};

/// Height of the window footer bar, in pixels.
///
/// This is the height of the existing Use Agent bar, which is the first surface
/// slated to move into the footer (§8, "Migration order"): a
/// [`ButtonSize::XSmall`](crate::view_components::action_button::ButtonSize::XSmall)
/// row (20px) inside a `Container::with_vertical_padding(4.)`, so 20 + 2 * 4.
///
/// **Pixels, deliberately, not lines.** §8 is explicit about this: a
/// line-denominated constant is only correct at one font size and line-height
/// ratio, and silently under-reserves at `line_height_ratio = 1.0`. What the bar
/// costs in rows is therefore not a constant -- it is this constant divided by the
/// cell height, which is exactly what the flex layout computes for us.
pub const WINDOW_FOOTER_BAR_HEIGHT_PX: f32 = 28.;

/// Renders the window footer bar.
///
/// `content` is whatever surface currently has something to say -- today only the
/// Use Agent / CLI-agent toolbar (§8, "Migration order"). When it is `None` the bar
/// still reserves its height and draws nothing: §8 leaves "visible when empty, or
/// collapsed to a hairline" open, and notes that reserving-but-not-drawing is the
/// simplest thing that preserves the constant.
///
/// Whatever ends up here **cannot** change the bar's height, and that is enforced
/// rather than trusted: the content is wrapped in [`FixedHeightBar`], which lays it
/// out under a hard `max.y` of [`WINDOW_FOOTER_BAR_HEIGHT_PX`] and reports exactly
/// that height to its parent no matter what the child returns. Content adapts to the
/// bar, never the reverse; the moment the bar can grow, the row count varies again
/// and every failure in the §8 table returns.
pub fn render_window_footer_bar(content: Option<Box<dyn Element>>) -> Box<dyn Element> {
    FixedHeightBar::new(content.unwrap_or_else(|| Empty::new().finish())).finish()
}

/// The size [`FixedHeightBar`] reports to its parent.
///
/// Split out from `layout` so the clamp can be tested without a `LayoutContext`:
/// the whole point of the element is that this is *not* a function of the child's
/// height, and that is the property worth asserting.
///
/// The width is taken from the incoming constraint when it is finite so the bar
/// spans the pane (a background painted by the content then covers the full width);
/// an unbounded width -- which a `Flex::column` never hands a child, but a `Stack`
/// or a scrollable might -- falls back to the child's own width rather than
/// returning infinity.
fn fixed_bar_size(child_size: Vector2F, constraint: SizeConstraint) -> Vector2F {
    let width = if constraint.max.x().is_finite() {
        constraint.max.x()
    } else {
        child_size.x()
    };

    vec2f(width, WINDOW_FOOTER_BAR_HEIGHT_PX)
}

/// An element that is exactly [`WINDOW_FOOTER_BAR_HEIGHT_PX`] tall, whatever its
/// child does.
///
/// This exists because nothing in `warpui` clamps a *returned* size.
/// `ConstrainedBox::with_height` narrows the constraint it passes down and then
/// returns `self.child.layout(..)` verbatim; `Clipped` clips at paint but also
/// returns the child's size from `layout`. A `Flex` adds whatever a non-flexible
/// child returns to `fixed_space` (`flex/mod.rs`), so a child that ignores the
/// constraint and returns 56px would take 56px out of the terminal -- i.e. the
/// pty's row count would depend on the bar's content, which is precisely the bug
/// §8 exists to remove.
///
/// Two things happen here:
///
/// 1. **Truncation by constraint.** The child is laid out with `max.y` (and `min.y`)
///    pinned to the bar height. `Wrap` -- which is what
///    `AgentInputFooter::render_cli_mode_footer` builds its chips with -- already
///    honours the cross-axis maximum: it stops adding runs once the next one would
///    exceed it (`flex/wrap.rs`, "If the new size would cause the element to exceed
///    the max size along the cross axis"). So in a narrow pane the chips that would
///    have wrapped to a second or third run are simply not laid out, instead of the
///    toolbar growing to two or three rows.
/// 2. **A hard clamp, and a clip.** Whatever the child returns, this reports
///    [`fixed_bar_size`], and paints the child inside a clip layer of exactly those
///    bounds. Step 1 covers every element that respects its constraint; step 2 is
///    what makes the guarantee unconditional for the ones that do not.
struct FixedHeightBar {
    child: Box<dyn Element>,
    size: Option<Vector2F>,
    origin: Option<Point>,
}

impl FixedHeightBar {
    fn new(child: Box<dyn Element>) -> Self {
        Self {
            child,
            size: None,
            origin: None,
        }
    }
}

impl Element for FixedHeightBar {
    fn layout(
        &mut self,
        mut constraint: SizeConstraint,
        ctx: &mut LayoutContext,
        app: &AppContext,
    ) -> Vector2F {
        constraint.max.set_y(WINDOW_FOOTER_BAR_HEIGHT_PX);
        constraint.min.set_y(WINDOW_FOOTER_BAR_HEIGHT_PX);

        let child_size = self.child.layout(constraint, ctx, app);
        let size = fixed_bar_size(child_size, constraint);
        self.size = Some(size);
        size
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext, app: &AppContext) {
        self.child.after_layout(ctx, app);
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, app: &AppContext) {
        self.origin = Some(Point::from_vec2f(origin, ctx.scene.z_index()));

        let size = self.size.expect("size should be set at paint time");
        // Intersect with the active layer rather than replacing it, so the bar is
        // still clipped by whatever clips the terminal pane (split panes rely on
        // this -- see the `Clipped` around the terminal column).
        ctx.scene
            .start_layer(ClipBounds::BoundedByActiveLayerAnd(RectF::new(
                origin, size,
            )));
        self.child.paint(origin, ctx, app);
        ctx.scene.stop_layer();
    }

    fn size(&self) -> Option<Vector2F> {
        self.size
    }

    fn origin(&self) -> Option<Point> {
        self.origin
    }

    fn dispatch_event(
        &mut self,
        event: &DispatchedEvent,
        ctx: &mut EventContext,
        app: &AppContext,
    ) -> bool {
        self.child.dispatch_event(event, ctx, app)
    }

    #[cfg(any(test, feature = "test-util"))]
    fn debug_text_content(&self) -> Option<String> {
        self.child.debug_text_content()
    }
}

#[cfg(test)]
mod tests {
    use warpui::geometry::vector::vec2f;
    use warpui::units::IntoPixels as _;
    use warpui::units::Pixels;

    use super::*;
    use crate::terminal::SizeInfo;

    /// The whole point of [`FixedHeightBar`]: the height it reports is not a function
    /// of the child's height. The CLI-agent toolbar's chips are laid out with
    /// `Wrap::row()`, so in a narrow pane they wrap to two or three runs -- roughly 28,
    /// 52 and 76px of content. If any of those reached the parent `Flex`, the flex
    /// would hand the terminal `pane - that`, and the pty's row count would depend on
    /// how many chips the user has enabled. That is the §8 bug with a new trigger.
    #[test]
    fn bar_height_does_not_follow_the_content_height() {
        let constraint = SizeConstraint::new(vec2f(0., 0.), vec2f(400., f32::INFINITY));

        for content_height in [0., 20., 28., 52., 76., 400.] {
            assert_eq!(
                fixed_bar_size(vec2f(320., content_height), constraint).y(),
                WINDOW_FOOTER_BAR_HEIGHT_PX,
                "content of {content_height}px must not change the bar height",
            );
        }
    }

    /// The bar spans the pane so that a background painted by its content (the
    /// alt-screen background the toolbar applies, for one) covers the full width
    /// rather than only the buttons.
    #[test]
    fn bar_takes_the_full_offered_width() {
        let constraint = SizeConstraint::new(vec2f(0., 0.), vec2f(400., f32::INFINITY));

        assert_eq!(fixed_bar_size(vec2f(120., 28.), constraint).x(), 400.);
    }

    /// A `Flex::column` always offers a finite cross-axis maximum, but a `Stack` or a
    /// scrollable need not. Returning `INFINITY` from `layout` poisons every ancestor's
    /// arithmetic, so fall back to the child's own width instead.
    #[test]
    fn unbounded_width_falls_back_to_the_content_width() {
        let constraint = SizeConstraint::new(vec2f(0., 0.), vec2f(f32::INFINITY, f32::INFINITY));

        assert_eq!(fixed_bar_size(vec2f(120., 28.), constraint).x(), 120.);
    }

    /// The reservation is denominated in pixels, so what it costs in rows depends on
    /// the cell height. This is the property §8 asks for and the reason the constant
    /// is not written in lines: at a 14px cell the bar costs two rows, at a 28px cell
    /// it costs one, and a line-denominated constant could only have been right for
    /// one of them.
    #[test]
    fn footer_bar_costs_rows_at_the_cell_height() {
        fn rows_lost(cell_height_px: f32) -> usize {
            let pane = vec2f(1000., 600.);
            let size_info = |size| {
                SizeInfo::new(
                    size,
                    8.0.into_pixels(),
                    cell_height_px.into_pixels(),
                    Pixels::zero(),
                    Pixels::zero(),
                )
            };

            // The bar's height comes off the pane by layout rather than arithmetic, so
            // model that here the way the flex column does: the content child is offered
            // the pane minus the bar, and that is what reaches `SizeInfo`.
            let content = vec2f(pane.x(), pane.y() - WINDOW_FOOTER_BAR_HEIGHT_PX);
            size_info(pane).rows() - size_info(content).rows()
        }

        assert_eq!(rows_lost(14.), 2);
        assert_eq!(rows_lost(28.), 1);
    }
}
