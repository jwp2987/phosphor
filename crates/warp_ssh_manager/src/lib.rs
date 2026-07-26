//! SSH manager data layer — persisted server/folder tree + OS keychain
//! credential storage + command assembly. UI and PTY injection logic lives in
//! `app/src/ssh_manager/` and the `secret_injector` module; this crate stays
//! pure Rust with no warpui dependency, so it can be run with `cargo test` on its own.

pub mod db;
pub mod repository;
pub mod secrets;
pub mod ssh_command;
pub mod ssh_config_parser;
pub mod sync_provider;
pub mod types;

pub use db::{set_database_path, with_conn};
pub use repository::{SshRepository, SshRepositoryError, SyncMetaRepository};
pub use secrets::{KeychainSecretStore, SecretKind, SshSecretStore, SshSecretStoreError};
pub use ssh_command::{
    ConnectionTestResult, build_ssh_args, build_ssh_command_line, test_connection,
};
pub use ssh_config_parser::{
    LoadOutcome, LoadResult, SshConfigCandidate, default_ssh_config_path, load_candidates,
    load_candidates_from, parse_ssh_config,
};
pub use sync_provider::{
    DbVersionStore, SshSyncData, SshSyncProvider, SyncNode, SyncOneKeyCredential, SyncServer,
};
pub use types::ConnectionStatus;
pub use types::{
    AuthType, NodeKind, OneKeyCredentialKind, ResolvedSshAuth, SshNode, SshOneKeyCredential,
    SshServerInfo,
};
