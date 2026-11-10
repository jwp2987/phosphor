//! Implementations of the [`SecureStorage`] service for the macOS platform.

use anyhow::anyhow;
use security_framework::os::macos::{
    keychain::SecKeychain, keychain_item::SecKeychainItem, passwords::SecKeychainItemPassword,
};

use super::{Error, availability};

/// Implementation of the SecureStorage service using macOS Security
/// framework keychains.
pub struct SecureStorage {
    /// The name of the service under which to store the values.
    service_name: String,
}

impl SecureStorage {
    pub fn new(service_name: &str) -> Self {
        Self {
            service_name: service_name.to_owned(),
        }
    }
}

impl super::SecureStorage for SecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        // Without a default keychain nothing was stored: the caller gets the error.
        let keychain = default_keychain()?;
        keychain
            .set_generic_password(self.service_name.as_str(), key, value.as_bytes())
            .map_err(Into::into)
    }

    fn read_value(&self, key: &str) -> Result<String, Error> {
        let (password, _) = self.get_password_item(key)?;
        String::from_utf8(password.as_ref().to_vec())
            .map_err(|err| Error::DecodeError(err.utf8_error()))
    }

    fn remove_value(&self, key: &str) -> Result<(), Error> {
        let (_, item) = self.get_password_item(key)?;
        item.delete();
        Ok(())
    }
}

impl SecureStorage {
    fn get_password_item(
        &self,
        key: &str,
    ) -> Result<(SecKeychainItemPassword, SecKeychainItem), Error> {
        // #1977: without a default keychain (a session with no login keychain, e.g. over ssh)
        // there is nothing to read or remove. That is "not found", as on Linux when the keyring
        // cannot be reached: the outage is on record in `availability` (one log line and one
        // notice per run), instead of every reader logging its own ERROR at every launch.
        let keychain = default_keychain().map_err(|_| Error::NotFound)?;
        keychain
            .find_generic_password(&self.service_name, key)
            .map_err(|_| Error::NotFound)
    }
}

/// The user's default keychain, with the outcome recorded in [`availability`].
fn default_keychain() -> Result<SecKeychain, Error> {
    note_keychain_open(SecKeychain::default())
}

/// Records whether the default keychain could be opened (#1977). The first failure of a run
/// moves the process-wide state to unavailable and is logged once, at WARN: the app works, only
/// stored credentials are missing, and the app shows its one keyring notice from that state. A
/// keychain that opens moves the state back, so a later launch of the lookup (the notice's retry,
/// or a keychain created meanwhile) recovers by itself.
fn note_keychain_open<T, E>(opened: Result<T, E>) -> Result<T, Error>
where
    E: Into<anyhow::Error>,
{
    match opened {
        Ok(keychain) => {
            availability::mark_available();
            Ok(keychain)
        }
        Err(err) => {
            let err: anyhow::Error = err.into();
            if availability::mark_unavailable() {
                log::warn!(
                    "The default keychain could not be opened: {err:#}. Stored credentials \
                     (API keys, proxy password) are unavailable until it can be."
                );
            } else {
                log::debug!("The default keychain is still unavailable: {err:#}");
            }
            Err(Error::Unknown(err))
        }
    }
}

impl From<security_framework::base::Error> for Error {
    fn from(value: security_framework::base::Error) -> Self {
        Error::Unknown(anyhow!(value))
    }
}

#[cfg(test)]
#[path = "mac_tests.rs"]
mod tests;
