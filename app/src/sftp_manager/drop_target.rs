//! SFTP browser drop-target Element, intercepting OS-level file drag events.
//!
//! Modeled after `terminal_size_element.rs`: captures `DragFiles` /
//! `DragFileExit` / `DragAndDropFiles` events in `dispatch_event` and
//! forwards them as `SftpBrowserAction`.
//! author: logic
//! date: 2026-05-27

use std::any::Any;
use std::path::PathBuf;

use warpui::{
    elements::Point,
    event::DispatchedEvent,
    geometry::vector::Vector2F,
    AfterLayoutContext, AppContext, Element, Event, EventContext, LayoutContext, PaintContext,
    SizeConstraint,
};

use super::browser::SftpBrowserAction;

/// SFTP drop-target Element
pub struct SftpDropTargetElement {
    child: Box<dyn Element>,
}

impl SftpDropTargetElement {
    /// Creates a drop-target Element
    pub fn new(child: Box<dyn Element>) -> Self {
        Self { child }
    }
}

impl Element for SftpDropTargetElement {
    fn layout(
        &mut self,
        constraint: SizeConstraint,
        ctx: &mut LayoutContext,
        app: &AppContext,
    ) -> Vector2F {
        self.child.layout(constraint, ctx, app)
    }

    fn after_layout(&mut self, ctx: &mut AfterLayoutContext, app: &AppContext) {
        self.child.after_layout(ctx, app);
    }

    fn paint(&mut self, origin: Vector2F, ctx: &mut PaintContext, app: &AppContext) {
        self.child.paint(origin, ctx, app)
    }

    fn size(&self) -> Option<Vector2F> {
        self.child.size()
    }

    fn origin(&self) -> Option<Point> {
        self.child.origin()
    }

    fn bounds(&self) -> Option<warpui::geometry::rect::RectF> {
        self.child.bounds()
    }

    fn parent_data(&self) -> Option<&dyn Any> {
        self.child.parent_data()
    }

    fn dispatch_event(
        &mut self,
        event: &DispatchedEvent,
        ctx: &mut EventContext,
        app: &AppContext,
    ) -> bool {
        let handled_by_child = self.child.dispatch_event(event, ctx, app);

        if !handled_by_child {
            let Some(z_index) = self.z_index() else {
                return false;
            };
            if let Some(event_at_z_index) = event.at_z_index(z_index, ctx) {
                match event_at_z_index {
                    Event::DragFiles { location } => {
                        if self.mouse_position_is_in_bounds(*location) {
                            ctx.dispatch_typed_action(SftpBrowserAction::DragFilesEnter);
                        } else {
                            ctx.dispatch_typed_action(SftpBrowserAction::DragFilesLeave);
                        }
                        return true;
                    }
                    Event::DragFileExit => {
                        ctx.dispatch_typed_action(SftpBrowserAction::DragFilesLeave);
                        return true;
                    }
                    Event::DragAndDropFiles { paths, location } => {
                        if self.mouse_position_is_in_bounds(*location) && !paths.is_empty() {
                            let paths: Vec<PathBuf> =
                                paths.iter().map(PathBuf::from).collect();
                            ctx.dispatch_typed_action(SftpBrowserAction::DragAndDropFiles(paths));
                        }
                        return true;
                    }
                    _ => {}
                };
            }
        }
        handled_by_child
    }
}

impl SftpDropTargetElement {
    /// Checks whether the mouse position is within the Element's bounds
    fn mouse_position_is_in_bounds(&self, position: Vector2F) -> bool {
        let Some(bounds) = self.bounds() else {
            return false;
        };
        bounds.contains_point(position)
    }
}
