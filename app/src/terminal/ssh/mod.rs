use std::time::Duration;

pub mod error;
pub mod ssh_detection;
pub mod util;
pub mod warpify;
#[cfg(unix)]
pub mod wrapper_master;

pub const SSH_WARPIFY_TIMEOUT_DURATION: Duration = Duration::from_secs(8);

/// A key event seen while an ssh warpification block is showing; only ^C is told apart.
pub struct SshKeyEvent {
    is_ctrl_c: bool,
}

impl SshKeyEvent {
    pub fn from_chars(chars: &str) -> Self {
        Self {
            is_ctrl_c: chars == "\x03",
        }
    }

    pub fn from_bytes(chars: &[u8]) -> Self {
        Self {
            is_ctrl_c: chars == [crate::terminal::model::escape_sequences::C0::ETX],
        }
    }

    pub fn is_ctrl_c(&self) -> bool {
        self.is_ctrl_c
    }
}
