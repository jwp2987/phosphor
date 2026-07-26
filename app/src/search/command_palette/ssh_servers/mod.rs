//! Command palette data source: SSH servers (openWarp-only).
//!
//! The user fuzzy-matches by server name / host in Ctrl+Shift+P; selecting
//! one emits `WorkspaceAction::OpenSshTerminal`, opening a new tab and
//! connecting (via SecretInjector auto-injecting the password, exactly
//! equivalent to right-clicking "Connect" in the SSH manager).

pub mod data_source;
pub mod search_item;

pub use data_source::SshServersDataSource;
pub use search_item::SshServerSearchItem;
