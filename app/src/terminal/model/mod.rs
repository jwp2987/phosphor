pub use grid::GridStorage;
pub use terminal_model::TerminalModel;

#[cfg(test)]
#[macro_export]
macro_rules! assert_lines_approx_eq {
    ($actual:expr_2021, $expected:expr_2021) => {{
        float_cmp::assert_approx_eq!(
            warpui::units::Lines,
            $actual,
            warpui::units::IntoLines::into_lines($expected)
        )
    }};
}

pub mod alt_screen;
pub mod ansi;
pub mod block;
pub mod blockgrid;
pub mod blocks;
pub mod bootstrap;
pub mod header_grid;
pub mod rich_content;
pub mod tmux;

pub mod early_output;
pub mod find;
pub mod grid;
pub mod image_map;
pub mod index;
// Ported from Warp: the block-lifecycle transition coordinator. See `lifecycle/mod.rs`.
pub(in crate::terminal) mod lifecycle;
pub mod secrets;
pub mod selection;
pub mod session;
pub mod terminal_model;
#[cfg(any(test, feature = "test-util"))]
pub mod test_utils;

pub use secrets::{
    set_user_and_enterprise_secret_regexes, ObfuscateSecrets, RespectObfuscatedSecrets, Secret,
    SecretHandle,
};
pub use warp_terminal::model::{
    char_or_str, completions, escape_sequences, grid::cell, iterm_image, kitty, mouse, BlockId,
};
