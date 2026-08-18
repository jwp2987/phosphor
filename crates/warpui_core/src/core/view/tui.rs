//! The TUI view layer, additive behind the `tui` feature.
//!
//! [`TuiView`] is the TUI sibling of [`View`](super::View): it shares all of
//! the neutral entity machinery (entity IDs, ref counts, handles,
//! subscriptions/observations, focus, the responder chain, typed actions, and
//! the unified [`ViewContext`]) and differs only in its render output
//! ([`TuiElement`] instead of `Element`).

use std::any::Any;

use super::{AccessibilityData, BlurContext, FocusContext, ViewContext};
use crate::elements::tui::TuiElement;
use crate::{AppContext, CursorInfo, Entity, EntityId, WindowId, keymap};

/// An interactive, renderable TUI component. The TUI counterpart of
/// [`View`](crate::View); registered with
/// [`AppContext::add_tui_view`](crate::AppContext::add_tui_view) or
/// [`AppContext::add_typed_action_tui_view`](crate::AppContext::add_typed_action_tui_view).
pub trait TuiView: Entity {
    /// Returns a unique name for this implementation of TuiView.
    fn ui_name() -> &'static str;

    /// Produces the [`TuiElement`] representation of this view.
    ///
    /// Terminal resizes flow through the layout pass: the presenter lays out
    /// against the current terminal size every frame, and each
    /// [`TuiElement::layout`] receives the [`AppContext`], so width-dependent
    /// read-only state (e.g. a char-cell editor's terminal width) is refreshed
    /// there. A size-driven *side effect* that must run once with the settled
    /// geometry — e.g. committing a PTY resize — belongs in
    /// [`TuiElement::after_layout`], the post-layout pass the presenter runs
    /// after arranging the tree and before paint (mirroring the GUI's
    /// `Element::after_layout`).
    fn render(&self, app: &AppContext) -> Box<dyn TuiElement>;

    /// Handles the view or its descendent receiving focus.
    fn on_focus(&mut self, _focus_ctx: &FocusContext, _ctx: &mut ViewContext<Self>) {}

    /// Handles the view or its descendent losing focus.
    fn on_blur(&mut self, _blur_ctx: &BlurContext, _ctx: &mut ViewContext<Self>) {}

    /// Handles the view's containing window closing.
    ///
    /// The TUI sibling of [`View::on_window_closed`](crate::View::on_window_closed);
    /// `AppContext::handle_window_closed` runs it for every view in the closing
    /// window, whichever registry the view lives in.
    fn on_window_closed(&mut self, _ctx: &mut ViewContext<Self>) {}

    /// Reports the active cursor position for the view, if any.
    ///
    /// The TUI sibling of
    /// [`View::active_cursor_position`](crate::View::active_cursor_position).
    /// A TUI view can be the focused view of a window that has a platform
    /// window behind it (a TUI leaf embedded under a GUI root), and that
    /// window's IME is positioned from whatever the focused view reports — so
    /// the hook has to exist on this side of the registry too.
    ///
    /// We intentionally provide _immutable_ access to the [`ViewContext`];
    /// querying the active cursor position shouldn't necessitate writes.
    fn active_cursor_position(&self, _ctx: &ViewContext<Self>) -> Option<CursorInfo> {
        None
    }

    /// Allows a view to hook into any interactions with it or its children.
    ///
    /// The TUI sibling of
    /// [`View::self_or_child_interacted_with`](crate::View::self_or_child_interacted_with),
    /// with the same contract: it fires on every view of the responder chain
    /// once the event has been handled somewhere in that chain.
    fn self_or_child_interacted_with(&self, _ctx: &mut ViewContext<Self>) {}

    /// Returns the current [`AccessibilityData`] for this view, if `Some`.
    ///
    /// The TUI sibling of
    /// [`View::accessibility_data`](crate::View::accessibility_data). A TUI
    /// view in a GUI window's responder chain is walked by
    /// `AppContext::focused_view_accessibility_data` alongside its GUI
    /// ancestors, so it needs to be able to contribute (and, by defaulting to
    /// `None`, to defer to those ancestors).
    fn accessibility_data(&self, _ctx: &mut ViewContext<Self>) -> Option<AccessibilityData> {
        None
    }

    /// Returns a representation of the current UI context for use in computing
    /// the set of valid actions/keyboard shortcuts.
    fn keymap_context(&self, _: &AppContext) -> keymap::Context {
        Self::default_keymap_context()
    }

    /// Returns the default context for a view.
    fn default_keymap_context() -> keymap::Context {
        let mut ctx = keymap::Context::default();
        ctx.set.insert(Self::ui_name());
        ctx
    }

    /// Returns the ids of child views this view directly owns via
    /// [`ViewHandle`]s that are not registered in the structural parent/child
    /// graph, regardless of whether they are currently being rendered.
    ///
    /// See [`View::child_view_ids`](crate::View::child_view_ids) for the full
    /// contract. The semantics are identical for TUI views.
    fn child_view_ids(&self, _app: &AppContext) -> Vec<EntityId> {
        Vec::new()
    }
}

/// The object-safe, type-erased TUI view object stored per window: the TUI
/// counterpart of [`AnyView`](crate::AnyView), with hook signatures that match
/// it so the shared dispatch paths treat both uniformly.
pub trait AnyTuiView {
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn ui_name(&self) -> &'static str;
    fn render(&self, app: &AppContext) -> Box<dyn TuiElement>;
    fn on_focus(
        &mut self,
        focus_ctx: &FocusContext,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    );
    fn on_blur(
        &mut self,
        blur_ctx: &BlurContext,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    );
    fn on_window_closed(&mut self, app: &mut AppContext, window_id: WindowId, view_id: EntityId);
    fn active_cursor_position(
        &self,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) -> Option<CursorInfo>;
    fn self_or_child_interacted_with(
        &self,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    );
    fn accessibility_data(
        &self,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) -> Option<AccessibilityData>;
    fn keymap_context(&self, app: &AppContext) -> keymap::Context;
    fn child_view_ids(&self, app: &AppContext) -> Vec<EntityId>;
}

impl<T> AnyTuiView for T
where
    T: TuiView,
{
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn ui_name(&self) -> &'static str {
        T::ui_name()
    }

    fn render(&self, app: &AppContext) -> Box<dyn TuiElement> {
        TuiView::render(self, app)
    }

    fn on_focus(
        &mut self,
        focus_ctx: &FocusContext,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) {
        let mut ctx = ViewContext::new(app, window_id, view_id);
        TuiView::on_focus(self, focus_ctx, &mut ctx);
    }

    fn on_blur(
        &mut self,
        blur_ctx: &BlurContext,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) {
        let mut ctx = ViewContext::new(app, window_id, view_id);
        TuiView::on_blur(self, blur_ctx, &mut ctx);
    }

    fn on_window_closed(&mut self, app: &mut AppContext, window_id: WindowId, view_id: EntityId) {
        let mut ctx = ViewContext::new(app, window_id, view_id);
        TuiView::on_window_closed(self, &mut ctx);
    }

    fn active_cursor_position(
        &self,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) -> Option<CursorInfo> {
        let ctx = ViewContext::new(app, window_id, view_id);
        TuiView::active_cursor_position(self, &ctx)
    }

    fn self_or_child_interacted_with(
        &self,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) {
        let mut ctx = ViewContext::new(app, window_id, view_id);
        TuiView::self_or_child_interacted_with(self, &mut ctx)
    }

    fn accessibility_data(
        &self,
        app: &mut AppContext,
        window_id: WindowId,
        view_id: EntityId,
    ) -> Option<AccessibilityData> {
        let mut ctx = ViewContext::new(app, window_id, view_id);
        TuiView::accessibility_data(self, &mut ctx)
    }

    fn keymap_context(&self, app: &AppContext) -> keymap::Context {
        TuiView::keymap_context(self, app)
    }

    fn child_view_ids(&self, app: &AppContext) -> Vec<EntityId> {
        TuiView::child_view_ids(self, app)
    }
}
