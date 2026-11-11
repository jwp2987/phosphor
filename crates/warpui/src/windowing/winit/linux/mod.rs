mod app;
pub mod clipboard;
mod cursor_theme;
mod window_manager;
mod zbus;

// The compositor-free half of the vendored winit's Wayland drop target (#2085: the uri-list
// parser, the mime choice, the event order of one drag), compiled here only so that its unit
// tests run with this crate's: `lib/winit` is its own workspace and no CI job runs its tests.
// The file uses nothing but `std`. Nothing in this crate calls it; winit does.
#[cfg(test)]
#[allow(dead_code)]
#[path = "../../../../../../lib/winit/src/platform_impl/linux/wayland/dnd/logic.rs"]
mod wayland_dnd_logic;

pub use app::{maybe_register_xlib_error_hook, take_encountered_bad_match_from_dri3_fence_from_fd};
pub use clipboard::*;
pub use cursor_theme::*;
pub(crate) use window_manager::*;
pub use zbus::*;
