use pathfinder_color::ColorU;
use warpui::{AppContext, Element, Entity, TypedActionView, View, elements::Rect};

pub struct RootView {}

// Implement the entity trait.
impl Entity for RootView {
    type Event = ();
}

// Implement the view trait so RootView could be considered as a view.
impl View for RootView {
    fn ui_name() -> &'static str {
        "RootView"
    }

    // Let's render a simple black rect background.
    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        Rect::new().with_background_color(ColorU::black()).finish()
    }
}

impl TypedActionView for RootView {
    type Action = ();
}
