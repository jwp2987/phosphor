//! Renderable for a parked chunk of dehydrated lines that is in view before the hydration pass
//! has expanded it (#1340 P5).

use warpui::{AppContext, LayoutContext};

use super::{RenderContext, RenderableBlock};
use crate::render::model::{RenderState, viewport::ViewportItem};

/// Paints nothing and lays nothing out: it only holds the rows' place. The hydration pass runs at
/// the start of every element layout and expands every chunk in or near the viewport, so this is
/// seen at most for a frame (a scroll that moved after hydration); the next layout replaces it
/// with real paragraphs.
pub struct RenderableParked {
    viewport_item: ViewportItem,
}

impl RenderableParked {
    pub fn new(viewport_item: ViewportItem) -> Self {
        Self { viewport_item }
    }
}

impl RenderableBlock for RenderableParked {
    fn viewport_item(&self) -> &ViewportItem {
        &self.viewport_item
    }

    fn layout(&mut self, _model: &RenderState, _ctx: &mut LayoutContext, _app: &AppContext) {}

    fn paint(&mut self, _model: &RenderState, _ctx: &mut RenderContext, _app: &AppContext) {}
}
