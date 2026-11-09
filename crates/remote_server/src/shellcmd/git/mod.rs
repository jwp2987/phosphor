//! Pure git logic shared by the app, the helper and the shell backend.
//!
//! One set of argv builders, output parsers and git algorithms, run through a [`GitRunner`] so
//! the local app, the helper and the shell backend execute identical commands and read identical
//! results. Nothing in this module spawns a process or touches the filesystem itself: the
//! [`GitRunner`] is the only door to the outside, and each environment supplies its own
//! (`LocalGitRunner` in the app, `daemon::git_runner::HelperGitRunner` in the helper,
//! `backend::ShellGitRunner` over the out-of-band executor).
//!
//! * [`runner`]: the [`GitRunner`] trait, [`GitOutput`], and the exit-status policy shared by all.
//! * [`types`]: the plain data (file status, staging request, commit, PR info, chain mode).
//! * [`parse`]: porcelain v2, name-status, numstat, shortstat, branch headers, commit log.
//! * [`ops`]: the algorithms (main-branch detection, commit chain, stage, discard, ...).
//! * [`write`]: the six git write requests (commit chain, push, pull, create PR, discard, stage)
//!   as proto-in, proto-out functions over a runner, shared by the helper and the shell backend.
//!
//! Wave H parity, package P5. The app keeps thin wrappers (`util::git`, `code_review::diff_state`)
//! so its behaviour is byte-identical.

pub mod ops;
pub mod parse;
pub mod runner;
pub mod types;
pub mod write;

pub use ops::*;
pub use parse::*;
pub use runner::{
    GIT_BASE_ARGS, GIT_ENV, GIT_NOPROMPT_ENV, GIT_SSH_COMMAND_SH_ASSIGNMENT, GIT_UNSET_ENV,
    GitOutput, GitRunner, gh_text, git_ssh_command, git_text, git_text_stdin,
};
pub use types::*;

#[cfg(test)]
#[path = "parse_tests.rs"]
mod parse_tests;

#[cfg(test)]
#[path = "ops_tests.rs"]
mod ops_tests;
