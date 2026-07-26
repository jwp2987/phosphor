//! OS keychain wrapper — cross-platform via the `keyring` crate.
//! Windows = Credential Manager / macOS = Keychain / Linux = Secret Service.
//!
//! The account key looks like `<node_uuid>:password` or
//! `<node_uuid>:passphrase`, and doesn't depend on host/username — **renaming
//! a node or changing its host never loses the password**.

use thiserror::Error;
use zeroize::Zeroizing;

const SERVICE: &str = "zap.ssh";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SecretKind {
    Password,
    Passphrase,
    RootPassword,
    OneKeyPassword,
}

impl SecretKind {
    fn suffix(&self) -> &'static str {
        match self {
            SecretKind::Password => "password",
            SecretKind::Passphrase => "passphrase",
            SecretKind::RootPassword => "root_password",
            SecretKind::OneKeyPassword => "onekey_password",
        }
    }
}

#[derive(Debug, Error)]
pub enum SshSecretStoreError {
    /// No keychain backend available on this platform (common on Linux
    /// headless / WSL without Secret Service). The UI layer should prompt the
    /// user to switch to a private key.
    #[error("no keychain backend available on this platform")]
    NoBackend,
    #[error("keyring error: {0}")]
    Keyring(String),
}

impl From<keyring::Error> for SshSecretStoreError {
    fn from(e: keyring::Error) -> Self {
        match e {
            keyring::Error::NoStorageAccess(_) | keyring::Error::PlatformFailure(_) => {
                SshSecretStoreError::NoBackend
            }
            other => SshSecretStoreError::Keyring(other.to_string()),
        }
    }
}

/// Credential storage abstraction — `KeychainSecretStore` is the default implementation; tests can use a mock.
pub trait SshSecretStore: Send + Sync {
    fn set(&self, node_id: &str, kind: SecretKind, secret: &str)
    -> Result<(), SshSecretStoreError>;

    fn get(
        &self,
        node_id: &str,
        kind: SecretKind,
    ) -> Result<Option<Zeroizing<String>>, SshSecretStoreError>;

    fn delete(&self, node_id: &str, kind: SecretKind) -> Result<(), SshSecretStoreError>;
}

#[derive(Default, Clone, Copy, Debug)]
pub struct KeychainSecretStore;

fn account_key(node_id: &str, kind: SecretKind) -> String {
    format!("{node_id}:{}", kind.suffix())
}

impl SshSecretStore for KeychainSecretStore {
    fn set(
        &self,
        node_id: &str,
        kind: SecretKind,
        secret: &str,
    ) -> Result<(), SshSecretStoreError> {
        let entry = keyring::Entry::new(SERVICE, &account_key(node_id, kind))?;
        entry.set_password(secret)?;
        Ok(())
    }

    fn get(
        &self,
        node_id: &str,
        kind: SecretKind,
    ) -> Result<Option<Zeroizing<String>>, SshSecretStoreError> {
        let entry = keyring::Entry::new(SERVICE, &account_key(node_id, kind))?;
        match entry.get_password() {
            Ok(s) => Ok(Some(Zeroizing::new(s))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete(&self, node_id: &str, kind: SecretKind) -> Result<(), SshSecretStoreError> {
        let entry = keyring::Entry::new(SERVICE, &account_key(node_id, kind))?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    //! An in-process in-memory mock that bypasses the OS keychain — for CI / unit tests.

    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct InMemorySecretStore {
        inner: Mutex<HashMap<String, String>>,
    }

    impl SshSecretStore for InMemorySecretStore {
        fn set(
            &self,
            node_id: &str,
            kind: SecretKind,
            secret: &str,
        ) -> Result<(), SshSecretStoreError> {
            self.inner
                .lock()
                .unwrap()
                .insert(account_key(node_id, kind), secret.to_string());
            Ok(())
        }

        fn get(
            &self,
            node_id: &str,
            kind: SecretKind,
        ) -> Result<Option<Zeroizing<String>>, SshSecretStoreError> {
            Ok(self
                .inner
                .lock()
                .unwrap()
                .get(&account_key(node_id, kind))
                .cloned()
                .map(Zeroizing::new))
        }

        fn delete(&self, node_id: &str, kind: SecretKind) -> Result<(), SshSecretStoreError> {
            self.inner
                .lock()
                .unwrap()
                .remove(&account_key(node_id, kind));
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::InMemorySecretStore;
    use super::*;

    #[test]
    fn set_get_delete_roundtrip() {
        let store = InMemorySecretStore::default();
        let node = "abc-123";
        store.set(node, SecretKind::Password, "hunter2").unwrap();
        let got = store.get(node, SecretKind::Password).unwrap().unwrap();
        assert_eq!(&*got, "hunter2");
        store.delete(node, SecretKind::Password).unwrap();
        assert!(store.get(node, SecretKind::Password).unwrap().is_none());
    }

    #[test]
    fn password_and_passphrase_have_separate_keys() {
        let store = InMemorySecretStore::default();
        store.set("n", SecretKind::Password, "pw").unwrap();
        store.set("n", SecretKind::Passphrase, "pp").unwrap();
        store
            .set("n", SecretKind::OneKeyPassword, "onekey-pw")
            .unwrap();
        assert_eq!(
            &*store.get("n", SecretKind::Password).unwrap().unwrap(),
            "pw"
        );
        assert_eq!(
            &*store.get("n", SecretKind::Passphrase).unwrap().unwrap(),
            "pp"
        );
        assert_eq!(
            &*store.get("n", SecretKind::OneKeyPassword).unwrap().unwrap(),
            "onekey-pw"
        );
    }

    #[test]
    fn delete_missing_is_idempotent() {
        let store = InMemorySecretStore::default();
        // Should not error on absent key.
        store.delete("never-stored", SecretKind::Password).unwrap();
    }
}
