//! SSH data sync provider, implementing the SyncDataProvider trait
//!
// author: logic
// date: 2026-05-26

use crate::db::with_conn;
use crate::repository::{SshRepository, SyncMetaRepository};
use crate::secrets::{KeychainSecretStore, SecretKind, SshSecretStore};
use crate::types::{NodeKind, OneKeyCredentialKind};
use diesel::connection::{Connection, SimpleConnection};
use diesel::{QueryDsl, RunQueryDsl};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use zap_sync::crypto;
use zap_sync::{SyncDataProvider, SyncEngineError, SyncVersionStore};
use zeroize::Zeroizing;

/// The keychain's credential kinds, used to uniformly iterate during collect/apply/orphan-cleanup
const ALL_SECRET_KINDS: [SecretKind; 4] = [
    SecretKind::Password,
    SecretKind::Passphrase,
    SecretKind::RootPassword,
    SecretKind::OneKeyPassword,
];

/// Node data used for SSH sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub name: String,
    pub sort_order: i32,
    pub is_collapsed: bool,
}

/// Server data used for SSH sync (including the encrypted password)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncServer {
    pub node_id: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_type: String,
    pub key_path: Option<String>,
    pub startup_command: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub credential_id: Option<String>,
    pub password_encrypted: Option<String>,
    pub passphrase_encrypted: Option<String>,
    pub root_password_encrypted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOneKeyCredential {
    pub id: String,
    pub label: String,
    pub username: String,
    #[serde(default = "default_onekey_kind")]
    pub kind: String,
    #[serde(default)]
    pub key_path: Option<String>,
    pub password_encrypted: Option<String>,
}

/// SSH sync data
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SshSyncData {
    pub nodes: Vec<SyncNode>,
    pub servers: Vec<SyncServer>,
    #[serde(default)]
    pub onekey_credentials: Vec<SyncOneKeyCredential>,
}

/// SSH data sync provider
pub struct SshSyncProvider {
    secret_store: KeychainSecretStore,
}

impl SshSyncProvider {
    /// Creates a new SshSyncProvider instance
    pub fn new() -> Self {
        Self {
            secret_store: KeychainSecretStore::default(),
        }
    }
}

impl SyncDataProvider for SshSyncProvider {
    fn section_key(&self) -> &str {
        "ssh"
    }

    fn collect_data(&self, token: &str) -> Result<serde_json::Value, SyncEngineError> {
        let nodes = with_conn(|conn| Ok(SshRepository::list_nodes(conn)?))
            .map_err(|e| SyncEngineError::Provider(e.to_string()))?;

        let mut sync_nodes = Vec::new();
        let mut sync_servers = Vec::new();
        let mut sync_onekey_credentials = Vec::new();

        let onekey_credentials =
            with_conn(|conn| Ok(SshRepository::list_onekey_credentials(conn)?))
                .map_err(|e| SyncEngineError::Provider(e.to_string()))?;
        for credential in onekey_credentials {
            let secret_kind = onekey_secret_kind(credential.kind);
            let password = read_secret(&self.secret_store, &credential.id, secret_kind)?;
            sync_onekey_credentials.push(SyncOneKeyCredential {
                id: credential.id,
                label: credential.label,
                username: credential.username,
                kind: credential.kind.as_db_str().to_string(),
                key_path: credential.key_path,
                password_encrypted: encrypt_optional(token, password.as_deref())?,
            });
        }

        for node in &nodes {
            sync_nodes.push(SyncNode {
                id: node.id.clone(),
                parent_id: node.parent_id.clone(),
                kind: node.kind.as_db_str().to_string(),
                name: node.name.clone(),
                sort_order: node.sort_order,
                is_collapsed: node.is_collapsed,
            });

            if node.kind == NodeKind::Server {
                let server_result =
                    with_conn(|conn| Ok(SshRepository::get_server(conn, &node.id)?))
                        .map_err(|e| SyncEngineError::Provider(e.to_string()))?;
                if let Some(server) = server_result {
                    // Distinguishes a keychain error from "the user didn't set a password":
                    // - Ok(Some) = there's a password, upload it encrypted
                    // - Ok(None) = the user genuinely didn't set one, write the field as None
                    // - Err = abort the entire upload, avoiding serializing a
                    //   transient keychain failure as "no password" and
                    //   overwriting the real password on other devices (PR #161 review #5)
                    let password = read_secret(&self.secret_store, &node.id, SecretKind::Password)?;
                    let passphrase =
                        read_secret(&self.secret_store, &node.id, SecretKind::Passphrase)?;
                    let root_password =
                        read_secret(&self.secret_store, &node.id, SecretKind::RootPassword)?;

                    sync_servers.push(SyncServer {
                        node_id: server.node_id.clone(),
                        host: server.host.clone(),
                        port: server.port,
                        username: server.username.clone(),
                        auth_type: server.auth_type.as_db_str().to_string(),
                        key_path: server.key_path.clone(),
                        startup_command: server.startup_command.clone(),
                        notes: server.notes.clone(),
                        credential_id: server.credential_id.clone(),
                        password_encrypted: encrypt_optional(token, password.as_deref())?,
                        passphrase_encrypted: encrypt_optional(token, passphrase.as_deref())?,
                        root_password_encrypted: encrypt_optional(token, root_password.as_deref())?,
                    });
                }
            }
        }

        let data = SshSyncData {
            nodes: sync_nodes,
            servers: sync_servers,
            onekey_credentials: sync_onekey_credentials,
        };

        serde_json::to_value(&data)
            .map_err(|e: serde_json::Error| SyncEngineError::Serialization(e.to_string()))
    }

    fn apply_data(&self, token: &str, data: &serde_json::Value) -> Result<(), SyncEngineError> {
        let ssh_data: SshSyncData = serde_json::from_value(data.clone())
            .map_err(|e: serde_json::Error| SyncEngineError::Serialization(e.to_string()))?;

        // ---- Phase 0 ---- decrypt everything + collect the explicit-clear list
        // pending_secrets: the remote explicitly gave ciphertext → needs to be written to the keychain
        // explicit_clears: the remote explicitly gave None → needs a keychain
        //                  delete (the user cleared the password on another
        //                  device; not cleaning up would make the local side
        //                  keep using the old password, violating user intent; PR #161 seven rounds of review)
        struct PendingSecret {
            node_id: String,
            kind: SecretKind,
            value: String,
        }
        let mut pending_secrets: Vec<PendingSecret> = Vec::new();
        let mut explicit_clears: Vec<(String, SecretKind)> = Vec::new();
        for server in &ssh_data.servers {
            for (kind, enc) in [
                (SecretKind::Password, &server.password_encrypted),
                (SecretKind::Passphrase, &server.passphrase_encrypted),
                (SecretKind::RootPassword, &server.root_password_encrypted),
            ] {
                match enc {
                    Some(enc) => {
                        let value = crypto::decrypt(token, enc)
                            .map_err(|e| SyncEngineError::Crypto(e.to_string()))?;
                        pending_secrets.push(PendingSecret {
                            node_id: server.node_id.clone(),
                            kind,
                            value,
                        });
                    }
                    None => {
                        explicit_clears.push((server.node_id.clone(), kind));
                    }
                }
            }
        }
        for credential in &ssh_data.onekey_credentials {
            let secret_kind = onekey_secret_kind(
                OneKeyCredentialKind::parse(&credential.kind)
                    .unwrap_or(OneKeyCredentialKind::Password),
            );
            match &credential.password_encrypted {
                Some(enc) => {
                    let value = crypto::decrypt(token, enc)
                        .map_err(|e| SyncEngineError::Crypto(e.to_string()))?;
                    pending_secrets.push(PendingSecret {
                        node_id: credential.id.clone(),
                        kind: secret_kind,
                        value,
                    });
                }
                None => {
                    explicit_clears.push((credential.id.clone(), secret_kind));
                }
            }
        }

        // ---- Phase 0.5 ---- topologically sort nodes, parents before
        // children; orphans (parent not in the dataset) are treated as root
        // nodes and inserted, avoiding a whole-transaction rollback from a SQLite FK violation
        let sorted_nodes = topologically_sort_nodes(&ssh_data.nodes);

        // ---- Phase 0.6 ---- collect the local pre-existing keychain owner ids, for later orphan-keychain cleanup
        let mut existing_secret_owner_ids: Vec<String> = with_conn(|conn| {
            Ok(persistence::schema::ssh_nodes::table
                .select(persistence::schema::ssh_nodes::id)
                .load::<String>(conn)?)
        })
        .map_err(|e| SyncEngineError::Provider(e.to_string()))?;
        let existing_credential_ids: Vec<String> = with_conn(|conn| {
            Ok(persistence::schema::ssh_onekey_credentials::table
                .select(persistence::schema::ssh_onekey_credentials::id)
                .load::<String>(conn)?)
        })
        .map_err(|e| SyncEngineError::Provider(e.to_string()))?;
        existing_secret_owner_ids.extend(existing_credential_ids);

        // ---- Phase 1 ---- write to the keychain first. Any failure → abort immediately, don't touch the DB.
        // Tracks a (node_id, kind, prior_value) list; if the DB phase fails:
        // - prior_value=Some(v) → restore to the old value (avoid overwriting the user's existing password)
        // - prior_value=None    → delete (avoid pollution)
        // The real "atomic rollback" is based on secret_store.set's idempotent overwrite semantics (PR #161 three rounds of review)
        let mut written_secrets: Vec<WrittenSecret> = Vec::new();
        for s in &pending_secrets {
            // Snapshot the prior value before writing, so that a later
            // rollback can genuinely restore the old value.
            // A real keychain error aborts the whole process, but NoBackend
            // (headless Linux, etc.) is treated as "no prior value".
            // This design matches collect_data's read_secret — the same environment constraint.
            let prior_value = match self.secret_store.get(&s.node_id, s.kind) {
                // store.get already returns Option<Zeroizing<String>>; used directly, preserving zeroizing semantics
                Ok(opt) => opt,
                Err(e) => {
                    // Equally strict as read_secret: any keychain error aborts, avoiding an unrollbackable state
                    rollback_keychain_writes(&self.secret_store, &written_secrets);
                    return Err(SyncEngineError::Provider(format!(
                        "Failed to read prior keychain value ({}, {:?}): {e}. Rolled back {} item(s); please confirm the keystore is available and retry the download",
                        s.node_id,
                        s.kind,
                        written_secrets.len()
                    )));
                }
            };
            if let Err(e) = self.secret_store.set(&s.node_id, s.kind, &s.value) {
                rollback_keychain_writes(&self.secret_store, &written_secrets);
                return Err(SyncEngineError::Provider(format!(
                    "Failed to write to keychain ({}, {:?}): {e}. Please check keystore permissions and retry the download",
                    s.node_id, s.kind
                )));
            }
            written_secrets.push(WrittenSecret {
                node_id: s.node_id.clone(),
                kind: s.kind,
                prior_value,
            });
        }

        // ---- Phase 2 ---- DB transaction: DELETE + INSERT in topological order
        let db_result = with_conn(|conn| {
            conn.transaction::<(), anyhow::Error, _>(|conn| {
                conn.batch_execute(
                    "DELETE FROM ssh_servers; DELETE FROM ssh_nodes; DELETE FROM ssh_onekey_credentials;",
                )?;

                for credential in &ssh_data.onekey_credentials {
                    diesel::insert_into(persistence::schema::ssh_onekey_credentials::table)
                        .values(persistence::model::NewSshOneKeyCredential {
                            id: &credential.id,
                            label: &credential.label,
                            username: &credential.username,
                            kind: OneKeyCredentialKind::parse(&credential.kind)
                                .unwrap_or(OneKeyCredentialKind::Password)
                                .as_db_str(),
                            key_path: credential.key_path.as_deref(),
                        })
                        .execute(conn)?;
                }

                for node in &sorted_nodes {
                    let kind = NodeKind::parse(&node.kind)
                        .ok_or_else(|| anyhow::anyhow!("Invalid kind: {}", node.kind))?;
                    diesel::insert_into(persistence::schema::ssh_nodes::table)
                        .values(persistence::model::NewSshNode {
                            id: &node.id,
                            parent_id: node.parent_id.as_deref(),
                            kind: kind.as_db_str(),
                            name: &node.name,
                            sort_order: node.sort_order,
                        })
                        .execute(conn)?;
                    if node.is_collapsed {
                        SshRepository::set_collapsed(conn, &node.id, true)?;
                    }
                }

                for server in &ssh_data.servers {
                    diesel::insert_into(persistence::schema::ssh_servers::table)
                        .values(persistence::model::NewSshServer {
                            node_id: &server.node_id,
                            host: &server.host,
                            port: server.port as i32,
                            username: &server.username,
                            auth_type: &server.auth_type,
                            key_path: server.key_path.as_deref(),
                            startup_command: server.startup_command.as_deref(),
                            notes: server.notes.as_deref(),
                            credential_id: server.credential_id.as_deref(),
                        })
                        .execute(conn)?;
                }
                Ok(())
            })
        });
        if let Err(e) = db_result {
            // DB failure → roll back the keychain writes just made, avoiding a
            // long-lived leftover secret pointing at a nonexistent node
            let rolled = written_secrets.len();
            rollback_keychain_writes(&self.secret_store, &written_secrets);
            return Err(SyncEngineError::Provider(format!(
                "DB write failed ({e}); rolled back {rolled} keychain write(s)"
            )));
        }

        // ---- Phase 3a ---- clean up explicit-clear: the node still exists
        // but the remote set the corresponding *_encrypted to None
        // The user cleared a password on another device → must delete the
        // local keychain entry, otherwise connect would keep using the old
        // password, violating the user's clear intent (PR #161 seven rounds of review)
        for (node_id, kind) in &explicit_clears {
            if let Err(e) = self.secret_store.delete(node_id, *kind) {
                log::warn!(
                    "Failed to clean up explicit-clear keychain entry {node_id}/{:?}: {e}",
                    kind
                );
            }
        }

        // ---- Phase 3b ---- clean up orphan keychain entries: passwords for
        // owner ids that existed locally but were deleted remotely must be
        // explicitly deleted, otherwise a node reappearing with the same UUID
        // would read a stale password (PR #161 review #4)
        let mut new_secret_owner_ids: HashSet<&str> =
            ssh_data.nodes.iter().map(|n| n.id.as_str()).collect();
        new_secret_owner_ids.extend(
            ssh_data
                .onekey_credentials
                .iter()
                .map(|credential| credential.id.as_str()),
        );
        for old_id in &existing_secret_owner_ids {
            if new_secret_owner_ids.contains(old_id.as_str()) {
                continue;
            }
            for kind in ALL_SECRET_KINDS {
                if let Err(e) = self.secret_store.delete(old_id, kind) {
                    log::warn!("Failed to clean up orphan keychain entry {old_id}/{:?}: {e}", kind);
                }
            }
        }

        Ok(())
    }
}

/// Record of keychain entries written during apply_data Phase 1, with a
/// snapshot of the prior value for a genuine rollback.
/// `prior_value` is held as `Zeroizing<String>`, guaranteeing the plaintext
/// password is zeroized when dropped along the rollback chain.
struct WrittenSecret {
    node_id: String,
    kind: SecretKind,
    prior_value: Option<Zeroizing<String>>,
}

/// The real "rollback": for each entry that was overwritten:
/// - prior_value=Some → write back the old value, avoiding the user's existing password being lost
/// - prior_value=None → delete, avoiding an orphan
/// Any step's failure is only logged, not blocking the caller (best-effort).
fn rollback_keychain_writes<S: SshSecretStore + ?Sized>(store: &S, written: &[WrittenSecret]) {
    for entry in written {
        let res = match &entry.prior_value {
            Some(v) => store.set(&entry.node_id, entry.kind, v.as_str()),
            None => store.delete(&entry.node_id, entry.kind),
        };
        if let Err(e) = res {
            log::warn!(
                "Failed to roll back keychain write {}/{:?}: {e} (the secret may keep the new value or become an orphan)",
                entry.node_id,
                entry.kind
            );
        }
    }
}

/// Reads a keychain credential.
/// - `Ok(Some)` = there's a password, upload it encrypted
/// - `Ok(None)` = the user didn't set a password (a legal state), write the field as None
/// - `Err` = keychain failure (NoBackend / Locked / permission denied)
///
/// Note: no fallback for NoBackend. The underlying keyring crate maps both a
/// locked keychain and a completely absent backend to NoBackend, which can't
/// be reliably distinguished (documented behavior of keyring 3.6). Treating
/// NoBackend as Ok(None) would let a transient failure like "locked" silently
/// drop the password → wiped in the cloud, unrecoverable after a reinstall
/// (KDF/format is still a pending optimization item).
/// If a headless Linux / CI user has no password throughout, upload won't
/// trigger this function; once an Err is encountered, the error message
/// clearly directs the user to unlock/enable the keychain.
fn read_secret(
    store: &dyn SshSecretStore,
    node_id: &str,
    kind: SecretKind,
) -> Result<Option<String>, SyncEngineError> {
    match store.get(node_id, kind) {
        Ok(opt) => Ok(opt.map(|z| z.to_string())),
        Err(e) => Err(SyncEngineError::Provider(format!(
            "Failed to read keychain ({node_id}, {kind:?}): {e}. \
             The keychain may be locked or the current environment has no backend (headless Linux / WSL, etc.). \
             Please unlock the keychain or enable secret-service / Credential Manager and retry the upload. \
             If this server genuinely doesn't need password sync, you can clear this field in the SSH manager."
        ))),
    }
}

fn encrypt_optional(token: &str, value: Option<&str>) -> Result<Option<String>, SyncEngineError> {
    match value {
        None => Ok(None),
        // An empty string is treated as "no password" and isn't uploaded
        // (compatible with prior behavior, avoiding pollution from an empty-string ciphertext)
        Some(s) if s.is_empty() => Ok(None),
        Some(s) => Ok(Some(
            crypto::encrypt(token, s).map_err(|e| SyncEngineError::Crypto(e.to_string()))?,
        )),
    }
}

fn default_onekey_kind() -> String {
    OneKeyCredentialKind::Password.as_db_str().to_string()
}

fn onekey_secret_kind(kind: OneKeyCredentialKind) -> SecretKind {
    match kind {
        OneKeyCredentialKind::Password => SecretKind::OneKeyPassword,
        OneKeyCredentialKind::Key => SecretKind::Passphrase,
    }
}

/// BFS topological sort: parents before children. Orphan nodes whose
/// parent_id references a node outside the dataset are treated as root nodes
/// and appended at the end, with parent_id cleared, avoiding a SQLite FK
/// constraint failure that would roll back the whole download.
fn topologically_sort_nodes(nodes: &[SyncNode]) -> Vec<SyncNode> {
    use std::collections::HashMap;
    let mut by_parent: HashMap<Option<&str>, Vec<&SyncNode>> = HashMap::new();
    for n in nodes {
        by_parent.entry(n.parent_id.as_deref()).or_default().push(n);
    }

    let mut result: Vec<SyncNode> = Vec::with_capacity(nodes.len());
    let mut seen: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<&SyncNode> = VecDeque::new();
    if let Some(roots) = by_parent.get(&None) {
        for r in roots {
            queue.push_back(*r);
        }
    }
    while let Some(node) = queue.pop_front() {
        if !seen.insert(node.id.clone()) {
            continue;
        }
        result.push(node.clone());
        if let Some(children) = by_parent.get(&Some(node.id.as_str())) {
            for c in children {
                queue.push_back(*c);
            }
        }
    }

    // Remaining nodes are either orphans (parent_id points outside the
    // dataset) or belong to a cycle. Both cases have parent_id cleared and are
    // demoted to root inserts (recoverable with no data loss), with an
    // explicit log warning so the user can see in the logs that data was structurally reset.
    for n in nodes {
        if !seen.contains(&n.id) {
            if has_cycle_membership(n, nodes) {
                log::warn!(
                    "apply_data: node {} is in a reference cycle (parent_id {:?}), demoted to a root node",
                    n.id,
                    n.parent_id
                );
            } else {
                log::warn!(
                    "apply_data: node {}'s parent_id {:?} doesn't exist in the dataset, inserted as a root node",
                    n.id,
                    n.parent_id
                );
            }
            let mut orphan = n.clone();
            orphan.parent_id = None;
            result.push(orphan);
        }
    }

    result
}

/// Determines whether node `start` is in a cycle (following the parent_id
/// chain from it eventually loops back to itself or onto the cycle).
/// Used to distinguish "orphan" vs "cycle" in logs; bounds the max traversal
/// steps to prevent exponential complexity.
fn has_cycle_membership(start: &SyncNode, all: &[SyncNode]) -> bool {
    let by_id: std::collections::HashMap<&str, &SyncNode> =
        all.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut current = start;
    let mut visited: HashSet<&str> = HashSet::new();
    let max_steps = all.len() + 1;
    for _ in 0..max_steps {
        let Some(pid) = current.parent_id.as_deref() else {
            return false;
        };
        if !visited.insert(current.id.as_str()) {
            // Visited the same node again → a cycle
            return true;
        }
        match by_id.get(pid) {
            Some(parent) => current = parent,
            None => return false, // parent is outside the dataset → orphan, not a cycle
        }
    }
    // Didn't finish within max_steps → there must be a cycle
    true
}

/// Database sync version store adapter
pub struct DbVersionStore;

impl SyncVersionStore for DbVersionStore {
    fn get_sync_version(&self) -> Result<i64, SyncEngineError> {
        with_conn(|c| Ok(SyncMetaRepository::get_sync_version(c)?))
            .map_err(|e| SyncEngineError::VersionStore(e.to_string()))
    }

    fn set_sync_version(&self, version: i64) -> Result<(), SyncEngineError> {
        with_conn(|c| Ok(SyncMetaRepository::set_sync_version(c, version)?))
            .map_err(|e| SyncEngineError::VersionStore(e.to_string()))
    }

    fn update_sync_meta(&self, time: &str, platform: &str) -> Result<(), SyncEngineError> {
        with_conn(|c| Ok(SyncMetaRepository::update_sync_meta(c, time, platform)?))
            .map_err(|e| SyncEngineError::VersionStore(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_section_key() {
        let provider = SshSyncProvider::new();
        assert_eq!(provider.section_key(), "ssh");
    }

    #[test]
    fn test_sync_node_serialization_roundtrip() {
        let node = SyncNode {
            id: "n1".to_string(),
            parent_id: Some("p1".to_string()),
            kind: "folder".to_string(),
            name: "Prod".to_string(),
            sort_order: 0,
            is_collapsed: true,
        };
        let json = serde_json::to_string(&node).unwrap();
        let parsed: SyncNode = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.id, "n1");
        assert_eq!(parsed.parent_id, Some("p1".to_string()));
        assert_eq!(parsed.kind, "folder");
        assert_eq!(parsed.name, "Prod");
        assert_eq!(parsed.sort_order, 0);
        assert!(parsed.is_collapsed);
    }

    #[test]
    fn test_sync_server_serialization_with_secrets() {
        let server = SyncServer {
            node_id: "s1".to_string(),
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            auth_type: "password".to_string(),
            key_path: Some("/key".to_string()),
            startup_command: None,
            notes: Some("test".to_string()),
            credential_id: None,
            password_encrypted: Some("enc123".to_string()),
            passphrase_encrypted: None,
            root_password_encrypted: Some("enc456".to_string()),
        };
        let json = serde_json::to_string(&server).unwrap();
        let parsed: SyncServer = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.node_id, "s1");
        assert_eq!(parsed.port, 22);
        assert_eq!(parsed.password_encrypted, Some("enc123".to_string()));
        assert_eq!(parsed.passphrase_encrypted, None);
        assert_eq!(parsed.root_password_encrypted, Some("enc456".to_string()));
    }

    #[test]
    fn test_sync_server_no_secrets() {
        let server = SyncServer {
            node_id: "s2".to_string(),
            host: "host".to_string(),
            port: 2222,
            username: "admin".to_string(),
            auth_type: "key".to_string(),
            key_path: None,
            startup_command: None,
            notes: None,
            credential_id: None,
            password_encrypted: None,
            passphrase_encrypted: None,
            root_password_encrypted: None,
        };
        let json = serde_json::to_string(&server).unwrap();
        let parsed: SyncServer = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.password_encrypted, None);
        assert_eq!(parsed.passphrase_encrypted, None);
        assert_eq!(parsed.root_password_encrypted, None);
    }

    #[test]
    fn test_ssh_sync_data_roundtrip() {
        let data = SshSyncData {
            nodes: vec![SyncNode {
                id: "n1".to_string(),
                parent_id: None,
                kind: "folder".to_string(),
                name: "Root".to_string(),
                sort_order: 0,
                is_collapsed: false,
            }],
            servers: vec![SyncServer {
                node_id: "s1".to_string(),
                host: "h".to_string(),
                port: 22,
                username: "u".to_string(),
                auth_type: "password".to_string(),
                key_path: None,
                startup_command: None,
                notes: None,
                credential_id: None,
                password_encrypted: Some("enc".to_string()),
                passphrase_encrypted: None,
                root_password_encrypted: None,
            }],
            onekey_credentials: Vec::new(),
        };
        let json = serde_json::to_string(&data).unwrap();
        let parsed: SshSyncData = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.nodes.len(), 1);
        assert_eq!(parsed.servers.len(), 1);
        assert_eq!(parsed.nodes[0].id, "n1");
        assert_eq!(
            parsed.servers[0].password_encrypted,
            Some("enc".to_string())
        );
    }

    #[test]
    fn test_ssh_sync_data_deserializes_legacy_payload_without_onekey_fields() {
        let json = r#"{
            "nodes": [
                {
                    "id": "s1",
                    "parent_id": null,
                    "kind": "server",
                    "name": "legacy",
                    "sort_order": 0,
                    "is_collapsed": false
                }
            ],
            "servers": [
                {
                    "node_id": "s1",
                    "host": "example.com",
                    "port": 22,
                    "username": "root",
                    "auth_type": "password",
                    "key_path": null,
                    "startup_command": null,
                    "notes": null,
                    "password_encrypted": null,
                    "passphrase_encrypted": null,
                    "root_password_encrypted": null
                }
            ]
        }"#;

        let parsed: SshSyncData = serde_json::from_str(json).unwrap();

        assert!(parsed.onekey_credentials.is_empty());
        assert_eq!(parsed.servers[0].credential_id, None);
    }

    #[test]
    fn test_onekey_credential_serialization_roundtrip() {
        let data = SshSyncData {
            nodes: Vec::new(),
            servers: Vec::new(),
            onekey_credentials: vec![SyncOneKeyCredential {
                id: "cred-1".to_string(),
                label: "prod-root".to_string(),
                username: "root".to_string(),
                kind: "key".to_string(),
                key_path: Some("/home/root/.ssh/id_ed25519".to_string()),
                password_encrypted: Some("enc".to_string()),
            }],
        };

        let json = serde_json::to_string(&data).unwrap();
        let parsed: SshSyncData = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.onekey_credentials.len(), 1);
        assert_eq!(parsed.onekey_credentials[0].id, "cred-1");
        assert_eq!(parsed.onekey_credentials[0].label, "prod-root");
        assert_eq!(parsed.onekey_credentials[0].username, "root");
        assert_eq!(parsed.onekey_credentials[0].kind, "key");
        assert_eq!(
            parsed.onekey_credentials[0].key_path.as_deref(),
            Some("/home/root/.ssh/id_ed25519")
        );
        assert_eq!(
            parsed.onekey_credentials[0].password_encrypted,
            Some("enc".to_string())
        );
    }

    #[test]
    fn test_onekey_credential_deserializes_legacy_payload_as_password() {
        let json = r#"{
            "id": "cred-1",
            "label": "prod-root",
            "username": "root",
            "password_encrypted": null
        }"#;

        let parsed: SyncOneKeyCredential = serde_json::from_str(json).unwrap();

        assert_eq!(parsed.kind, "password");
        assert_eq!(parsed.key_path, None);
    }

    #[test]
    fn test_onekey_key_credentials_use_passphrase_secret_slot() {
        assert_eq!(
            onekey_secret_kind(OneKeyCredentialKind::Password),
            SecretKind::OneKeyPassword
        );
        assert_eq!(
            onekey_secret_kind(OneKeyCredentialKind::Key),
            SecretKind::Passphrase
        );
    }

    #[test]
    fn test_ssh_sync_data_default_empty() {
        let data = SshSyncData::default();
        assert!(data.nodes.is_empty());
        assert!(data.servers.is_empty());
    }

    #[test]
    fn test_sync_node_null_parent() {
        let node = SyncNode {
            id: "root".to_string(),
            parent_id: None,
            kind: "folder".to_string(),
            name: "R".to_string(),
            sort_order: 0,
            is_collapsed: false,
        };
        let json = serde_json::to_string(&node).unwrap();
        assert!(
            json.contains("\"parent_id\":null"),
            "parent_id=None should serialize to null"
        );
        let parsed: SyncNode = serde_json::from_str(&json).unwrap();
        assert!(parsed.parent_id.is_none());
    }
}
