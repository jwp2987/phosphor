mod keyboard;
mod keycode_cache;
mod mouse;
mod screenshot;
mod util;

use async_trait::async_trait;
use warpui::r#async::Timer;

use crate::{Action, ActionResult, Options, TargetedAction};

pub fn is_supported_on_current_platform() -> bool {
    true
}

/// Reports whether background, per-window control is available.
///
/// TODO(#349): the macOS background input stack (focus-without-raise plus window-targeted
/// posting) is not ported yet, so this reports `false` and every action falls back to the
/// legacy whole-screen path.
pub fn background_supported() -> bool {
    false
}

/// Ends the background computer-use session owned by `owner`. No-op until the macOS background
/// activation stack is ported (#349).
pub fn end_background_session(owner: &str) {
    let _ = owner;
}

/// Enumerates the on-screen windows. Empty until macOS window enumeration is ported (#349).
pub fn enumerate_windows() -> Vec<crate::WindowInfo> {
    Vec::new()
}

/// Experimental: lists on-screen windows for diagnosing PID/window targeting. Empty until macOS
/// window enumeration is ported (#349).
pub fn list_windows() -> String {
    String::new()
}

pub struct Actor {
    keyboard: keyboard::Keyboard,
    mouse: mouse::Mouse,
}

impl Actor {
    pub fn new() -> Self {
        Self {
            keyboard: keyboard::Keyboard::new(),
            mouse: mouse::Mouse::new(),
        }
    }
}

#[async_trait]
impl super::Actor for Actor {
    fn platform(&self) -> Option<super::Platform> {
        Some(super::Platform::Mac)
    }

    async fn perform_actions(
        &mut self,
        actions: &[TargetedAction],
        options: Options,
    ) -> Result<ActionResult, String> {
        for targeted in actions {
            // Per-window targeting is not ported on macOS yet (#349); act on the screen /
            // frontmost application regardless of the requested target.
            let action: &Action = &targeted.action;
            match action {
                Action::Wait(duration) => {
                    Timer::after(*duration).await;
                }
                Action::MouseDown { button, at } => {
                    self.mouse.move_to(*at).await?;
                    self.mouse.button_down(button)?;
                }
                Action::MouseUp { button } => self.mouse.button_up(button)?,
                Action::MouseMove { to } => self.mouse.move_to(*to).await?,
                Action::MouseWheel {
                    at,
                    direction,
                    distance,
                } => {
                    self.mouse.move_to(*at).await?;
                    self.mouse.scroll(direction, distance)?;
                }
                Action::TypeText { text } => {
                    self.keyboard.type_text(text)?;
                }
                Action::KeyDown { key } => {
                    self.keyboard.key_down(key)?;
                }
                Action::KeyUp { key } => {
                    self.keyboard.key_up(key)?;
                }
            }
        }

        let screenshot = if let Some(params) = options.screenshot_params {
            Some(screenshot::take(params)?)
        } else {
            None
        };

        Ok(ActionResult::legacy(
            screenshot,
            Some(self.mouse.current_position()?),
        ))
    }
}
