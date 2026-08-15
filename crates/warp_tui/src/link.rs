#![allow(dead_code)]
// Staged port: this module came across from the pinned oracle (see `8c6d3a4c
// feat(tui): stage warp_tui crate ... (phase 0)` and the `port(tui)` commits) with
// the upstream API surface intact, but only the paths the TUI actually drives are
// wired up yet. The unused items here are upstream's, not ours.
//
// Kept rather than pruned because this fork re-pins against upstream roughly
// weekly (`ORACLE.md`); deleting upstream's helpers would turn each one into a
// re-pin conflict for no gain. Drop this attribute once the module is fully wired
// and check what is genuinely dead then.

use warpui_core::AppContext;
use warpui_core::elements::MouseStateHandle;
use warpui_core::elements::tui::{Modifier, TuiElement, TuiEventContext, TuiHoverable, TuiText};

use crate::tui_builder::TuiUiBuilder;

/// Reusable link presentation with persistent hover state.
#[derive(Clone, Default)]
pub(crate) struct TuiLink {
    hover_state: MouseStateHandle,
}

impl TuiLink {
    /// Renders caller-provided link text and invokes `on_open` on click.
    pub(crate) fn render(
        &self,
        label: impl Into<String>,
        app: &AppContext,
        on_open: impl FnMut(&mut TuiEventContext, &AppContext) + 'static,
    ) -> Box<dyn TuiElement> {
        let builder = TuiUiBuilder::from_app(app);
        let style = builder.muted_text_style();
        let is_hovered = self
            .hover_state
            .lock()
            .is_ok_and(|state| state.is_hovered());
        let style = if is_hovered {
            style
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            style.add_modifier(Modifier::UNDERLINED)
        };
        TuiHoverable::new(
            self.hover_state.clone(),
            TuiText::new(label.into()).with_style(style).finish(),
        )
        .on_click(on_open)
        .finish()
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;
