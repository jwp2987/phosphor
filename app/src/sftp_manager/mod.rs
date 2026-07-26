//! SFTP file browser module
//!
//! Provides SFTP connection management, remote file browsing, upload/download,
//! and related functionality.
//! author: logic
//! date: 2026-05-26

pub mod breadcrumb;
pub mod browser;
pub mod context_menu;
pub mod dialogs;
pub mod drop_target;
pub mod file_list;
pub mod sftp_backend;
pub mod sftp_ops;
pub mod transfer_panel;
pub mod types;

#[cfg(test)]
#[path = "browser_tests.rs"]
mod browser_tests;

#[cfg(test)]
#[path = "browser_integration_tests.rs"]
mod browser_integration_tests;

#[allow(unused_imports)]
pub use browser::{SftpBrowserAction, SftpBrowserView};
#[allow(unused_imports)]
pub use types::*;
