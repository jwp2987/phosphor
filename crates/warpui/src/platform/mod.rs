pub mod app;
#[cfg(any(target_os = "linux", target_os = "freebsd"))]
pub mod linux;
#[cfg(target_os = "macos")]
pub mod mac;
#[cfg(target_os = "windows")]
pub mod windows;

pub mod headless;
pub mod session_end_gate;
pub mod shutdown_progress;
pub(crate) mod termination_signals;

pub mod current {
    cfg_if::cfg_if! {
        if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            pub use super::linux::*;
        } else if #[cfg(target_os = "macos")] {
            pub use super::mac::*;
        } else if #[cfg(target_os = "windows")] {
            pub use super::windows::*;
        } else {
            pub use warpui_core::platform::test::*;
        }
    }
}

pub use warpui_core::platform::*;

pub use app::AppBuilder;

/// Creates a handle to the OS clipboard for the current platform.
///
/// Returns an error on platforms without a system clipboard (e.g. headless).
pub fn create_system_clipboard() -> anyhow::Result<Box<dyn crate::Clipboard + Send>> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "macos")] {
            Ok(Box::new(mac::clipboard::Clipboard::new()?))
        } else if #[cfg(any(target_os = "linux", target_os = "freebsd"))] {
            Ok(Box::new(crate::windowing::winit::linux::LinuxClipboard::new()?))
        } else if #[cfg(target_os = "windows")] {
            Ok(Box::new(crate::windowing::winit::windows::WindowsClipboard::new()?))
        } else {
            anyhow::bail!("System clipboard is unavailable on this platform")
        }
    }
}

/// Returns whether the current device is a mobile device with touch input. Always `false`:
/// no supported target is a mobile device.
pub fn is_mobile_device() -> bool {
    false
}

/// A trait for accessing internal per-platform concrete implementations
/// through a wrapper type.
trait AsInnerMut<Inner: ?Sized> {
    fn as_inner_mut(&mut self) -> &mut Inner;
}
