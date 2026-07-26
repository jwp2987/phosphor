//! OneKey credential loading: reads all saved server credentials from the
//! SSH Manager persistence layer + Keychain/DPAPI/Linux Keyring, for
//! `TerminalView` to show a selection menu when it detects a PTY password prompt.
//!
//! ## Note
//!
//! - Internally calls `warp_ssh_manager::with_conn` (sync Mutex + SQLite)
//!   and `KeychainSecretStore::get` (sync OS API) — these **must not** be
//!   called synchronously directly on the UI main thread, since with many
//!   servers it would cause stutter. Callers should go through
//!   `tokio::task::spawn_blocking`.
//! - Secrets are held as `Zeroizing<String>` throughout, zeroed automatically on drop.

use anyhow::Result;
use zeroize::Zeroizing;

use warp_ssh_manager::{
    AuthType, KeychainSecretStore, NodeKind, OneKeyCredentialKind as StoredOneKeyCredentialKind,
    SecretKind, SshRepository, SshSecretStore,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OneKeyCredentialKind {
    Password,
    Passphrase,
}

pub struct OneKeyCredential {
    pub label: String,
    pub subtitle: String,
    pub secret: Zeroizing<String>,
    pub kind: OneKeyCredentialKind,
}

pub fn load_saved_ssh_credentials() -> Result<Vec<OneKeyCredential>> {
    let store = KeychainSecretStore;
    load_saved_ssh_credentials_with_store(&store)
}

fn load_saved_ssh_credentials_with_store(
    store: &dyn SshSecretStore,
) -> Result<Vec<OneKeyCredential>> {
    warp_ssh_manager::with_conn(|conn| {
        let nodes = SshRepository::list_nodes(conn)?;
        let mut credentials = Vec::new();

        for credential in SshRepository::list_onekey_credentials(conn)? {
            let (secret_kind, kind) = match credential.kind {
                StoredOneKeyCredentialKind::Password => {
                    (SecretKind::OneKeyPassword, OneKeyCredentialKind::Password)
                }
                StoredOneKeyCredentialKind::Key => {
                    (SecretKind::Passphrase, OneKeyCredentialKind::Passphrase)
                }
            };
            let secret = match store.get(&credential.id, secret_kind) {
                Ok(Some(secret)) if !secret.is_empty() => secret,
                Ok(Some(_)) | Ok(None) => continue,
                Err(e) => {
                    log::warn!("onekey: failed to read shared ssh credential: {e}");
                    continue;
                }
            };
            let subtitle = match credential.kind {
                StoredOneKeyCredentialKind::Password => credential.username,
                StoredOneKeyCredentialKind::Key => {
                    let key_path = credential.key_path.as_deref().unwrap_or("key");
                    if credential.username.is_empty() {
                        key_path.to_string()
                    } else {
                        format!("{key_path} for {}", credential.username)
                    }
                }
            };
            credentials.push(OneKeyCredential {
                label: credential.label,
                subtitle,
                secret,
                kind,
            });
        }

        for node in nodes {
            if node.kind != NodeKind::Server {
                continue;
            }
            let Some(server) = SshRepository::get_server(conn, &node.id)? else {
                continue;
            };
            let kind = match server.auth_type {
                AuthType::Password => SecretKind::Password,
                AuthType::Key => SecretKind::Passphrase,
                AuthType::OneKey => continue,
            };
            let secret = match store.get(&node.id, kind) {
                Ok(Some(secret)) if !secret.is_empty() => secret,
                Ok(Some(_)) | Ok(None) => continue,
                Err(e) => {
                    log::warn!("onekey: failed to read saved ssh credential: {e}");
                    continue;
                }
            };
            let target = if server.username.is_empty() {
                format!("{}:{}", server.host, server.port)
            } else {
                format!("{}@{}:{}", server.username, server.host, server.port)
            };
            // kind is derived from auth_type and can only be Password or
            // Passphrase; RootPassword is outside OneKey's own scope (it
            // goes through the separate su confirmation popup flow).
            let (subtitle, kind) = match server.auth_type {
                AuthType::Password => (target, OneKeyCredentialKind::Password),
                AuthType::Key => {
                    let key_path = server.key_path.as_deref().unwrap_or("key");
                    (
                        format!("{key_path} for {target}"),
                        OneKeyCredentialKind::Passphrase,
                    )
                }
                AuthType::OneKey => continue,
            };
            credentials.push(OneKeyCredential {
                label: node.name,
                subtitle,
                secret,
                kind,
            });
        }

        Ok(credentials)
    })
}
