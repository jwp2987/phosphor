//! Integration test scenarios that drive a real terminal session.
//!
//! ## Pinning the shell under test
//!
//! Some scenarios only run for a specific [`ShellType`](warp::terminal::shell::ShellType)
//! (see e.g. `test/bootstrapping.rs`'s `test_pwsh_vi_edit_mode_does_not_corrupt_commands`,
//! and the PowerShell-gated cases in `test/subshell.rs`, `test/typeahead.rs`,
//! `test/input.rs`, and `test/block_filtering.rs`). By default the shell used is
//! whatever `warp::integration_testing::terminal::util::current_shell_starter_and_version`
//! resolves as the runner's own default shell (via `$WARP_SHELL_PATH`, then the
//! current user's shell) — which means a shell-gated scenario silently never
//! executes unless the runner's default shell happens to match (see #796).
//!
//! Set `PHOSPHOR_TEST_SHELL` (a shell name resolvable on `$PATH`, e.g. `pwsh`,
//! or an absolute path to the shell executable) to pin the shell explicitly
//! for a run, regardless of the runner's default shell. See
//! `warp::integration_testing::terminal::util::TEST_SHELL_OVERRIDE_ENV_VAR`.

mod builder;
mod step;

pub mod test;
pub mod user_defaults;
pub mod util;

pub use builder::Builder;
pub use warp::integration_testing::view_getters;
pub use warpui::integration::TestStep;
