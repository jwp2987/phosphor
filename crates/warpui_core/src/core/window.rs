use core::fmt;
use std::{
    cell::Cell,
    collections::HashMap,
    sync::atomic::{AtomicUsize, Ordering},
};

use serde::{Deserialize, Serialize};

use crate::{core::view::AnyViewHandle, AnyView, EntityId};

/// A unique identifier for a window.
///
/// These are globally unique and not reused across the lifetime of the
/// application.
#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct WindowId(usize);

impl WindowId {
    /// Constructs a new globally-unique window ID.
    #[allow(clippy::new_without_default)]
    pub fn new() -> WindowId {
        static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
        let raw = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        WindowId(raw)
    }

    pub fn from_usize(value: usize) -> WindowId {
        WindowId(value)
    }
}

impl fmt::Display for WindowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

thread_local! {
    /// The window currently being rendered on this thread, if any.
    ///
    /// Rendering happens on a single thread (the main loop), one window at a
    /// time, so an ambient thread-local is a safe, zero-plumbing way to make the
    /// active window available to window-blind theme accessors. It is set by the
    /// render chokepoints (see `CurrentRenderWindowGuard`) and consulted by
    /// per-window appearance overrides in higher crates.
    static CURRENT_RENDER_WINDOW: Cell<Option<WindowId>> = const { Cell::new(None) };
}

/// Returns the window currently being rendered on this thread, if any.
pub fn current_render_window() -> Option<WindowId> {
    CURRENT_RENDER_WINDOW.with(|cell| cell.get())
}

/// Sets the window currently being rendered on this thread. Prefer
/// [`CurrentRenderWindowGuard`] so the ambient is always restored.
pub fn set_current_render_window(window_id: Option<WindowId>) {
    CURRENT_RENDER_WINDOW.with(|cell| cell.set(window_id));
}

/// RAII guard that sets the ambient current render window for the duration of a
/// render pass and restores the previous value on drop (covering every early
/// return). Restoring the *previous* value (rather than forcing `None`) keeps
/// nested render passes correct.
#[must_use = "the guard must be held for the duration of the render pass"]
pub struct CurrentRenderWindowGuard(Option<WindowId>);

impl CurrentRenderWindowGuard {
    pub fn new(window_id: WindowId) -> Self {
        let previous = current_render_window();
        set_current_render_window(Some(window_id));
        Self(previous)
    }
}

impl Drop for CurrentRenderWindowGuard {
    fn drop(&mut self) {
        set_current_render_window(self.0);
    }
}

/// A structure holding all application state that is linked to a particular
/// window.
#[derive(Default)]
pub(super) struct Window {
    /// The set of views owned by this window, keyed by view ID.
    pub views: HashMap<EntityId, Box<dyn AnyView>>,

    /// TUI views owned by this window, kept in a separate map so GUI view storage
    /// is untouched. Only the TUI front-end populates this. See
    /// specs/warp-oss-sync/SCOPE.md.
    #[cfg(feature = "tui")]
    pub tui_views: HashMap<EntityId, Box<dyn crate::core::view::AnyTuiView>>,

    /// A handle to the window's root view (top of the view hierarchy), if any.
    pub root_view: Option<AnyViewHandle>,

    /// The ID of the currently focused view, if any.
    pub focused_view: Option<EntityId>,
}
