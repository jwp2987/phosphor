//! Implementation of the [`SecureStorage`] service for the Linux platform.

use std::{
    cell::OnceCell,
    collections::HashMap,
    fs::OpenOptions,
    io::{Read as _, Write as _},
    os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use anyhow::{Context, anyhow};
use rand::RngCore;
use ring::aead;
use secret_service::{
    EncryptionType,
    blocking::{Item, SecretService},
};

use super::{Error, availability};

/// Give-up net for reaching the Secret Service at all (#1805). With a D-Bus session but no
/// keyring that answers, the first call waits for the bus's service-activation timeout (120 s by
/// default); before this net that wait sat on the startup path. Reaching the service involves no
/// prompt (only `unlock` can prompt, and it is not covered by this net), so a service that has
/// not answered in this long is treated as unavailable for the run. If it does answer later the
/// keyring is marked available again and the next lookup uses it.
const CONNECT_GIVE_UP: Duration = Duration::from_secs(10);

/// The Secret Service operations [`SecureStorage`] needs, behind a seam so tests can stand in for
/// the keyring (#1805). `read` and `delete` return [`Error::NotFound`] when the keyring answered
/// and holds no such item; any other error means the keyring could not be used.
trait Keyring {
    fn read(&self, key: &str) -> Result<String, Error>;
    fn write(&self, key: &str, value: &str) -> Result<(), Error>;
    fn delete(&self, key: &str) -> Result<(), Error>;
}

/// Implementation of the SecureStorage service using the Secret Service API, with an encrypted
/// owner-only file fallback for when the keyring cannot be used.
pub struct SecureStorage {
    /// The value to set for the "service" attribute, used to define a
    /// namespace for keys for the application.
    service_name: String,

    /// The Secret Service ([`SecretServiceKeyring`] outside tests).
    keyring: Box<dyn Keyring>,

    /// The fallback path to a directory in case a secret collection is
    /// not available.
    fallback_dir: Option<PathBuf>,

    /// The encryption fallback key.
    encryption_key: OnceCell<Option<aead::LessSafeKey>>,
}

/// What the fallback directory holds for one key.
enum FallbackEntry {
    /// A value written while the keyring could not be used: newer than any keyring copy.
    Value(String),
    /// A removal made while the keyring could not be used: any keyring copy is stale.
    Removed,
    /// Nothing: the keyring is authoritative.
    Absent,
}

impl SecureStorage {
    /// Creates a new [`SecureStorage`] instance.
    ///
    /// This does not eagerly open a connection to dbus or the underlying
    /// Secret Service provider.
    pub fn new(service_name: &str) -> Self {
        Self::with_keyring(
            service_name,
            None,
            Box::new(SecretServiceKeyring::new(
                service_name,
                open_default_collection_bounded,
            )),
        )
    }

    /// Creates a new [`SecureStorage`] instance with disk fallback
    ///
    /// Does the same work as [`SecureStorage::new`], as well as storing
    /// a path to a fallback directory.
    pub fn new_with_fallback(service_name: &str, fallback_dir: PathBuf) -> Self {
        Self::with_keyring(
            service_name,
            Some(fallback_dir),
            Box::new(SecretServiceKeyring::new(
                service_name,
                open_default_collection_bounded,
            )),
        )
    }

    fn with_keyring(
        service_name: &str,
        fallback_dir: Option<PathBuf>,
        keyring: Box<dyn Keyring>,
    ) -> Self {
        Self {
            service_name: service_name.to_owned(),
            keyring,
            fallback_dir,
            encryption_key: OnceCell::new(),
        }
    }

    /// Test hook: a handle whose Secret Service collection opener is `open_collection`.
    #[cfg(test)]
    fn with_opener(
        service_name: &str,
        fallback_dir: Option<PathBuf>,
        open_collection: fn() -> Result<Collection, Error>,
    ) -> Self {
        Self::with_keyring(
            service_name,
            fallback_dir,
            Box::new(SecretServiceKeyring::new(service_name, open_collection)),
        )
    }

    /// Returns an encryption key that can be used to encrypt
    /// values if the default secret collection is not available
    /// or otherwise not working. The key is lazy initialized since
    /// it does not need to be created unless the main method of
    /// storing secrets has failed
    fn encryption_key(&self) -> Result<&aead::LessSafeKey, Error> {
        self.encryption_key
            .get_or_init(|| {
                // We can use whatever super duper foolproof secure key we want here.
                // Here we are specifically choosing a value that will look inconspicuous
                // in case someone chooses to scan our binary for strings.
                //
                // DO NOT RENAME THIS LITERAL. It looks exactly like a leftover
                // "zap" string that the Phosphor rebrand should have swept up,
                // but it is not user-visible: it *is* the AES-256-GCM key
                // material for every fallback blob already written to disk on
                // hosts with no Secret Service provider. Changing a single byte
                // makes those blobs permanently undecryptable, silently — the
                // read just starts reporting the secret as missing.
                let mut key_bytes = Vec::from("zap-local-secure-storage-fallback-key");
                key_bytes.resize(aead::AES_256_GCM.key_len(), 0);
                match aead::UnboundKey::new(&aead::AES_256_GCM, key_bytes.as_slice()) {
                    Ok(key) => Some(aead::LessSafeKey::new(key)),
                    Err(_) => {
                        log::error!("Failed to initialize fallback encryption key");
                        None
                    }
                }
            })
            .as_ref()
            .ok_or_else(|| Error::Unknown(anyhow!("Invalid encryption key")))
    }

    fn fallback_encrypt(&self, value: &str) -> Result<Vec<u8>, Error> {
        let encryption_key = self.encryption_key()?;

        // Generates nonce by randomly generating numbers
        // This is not the official best way to do this, but it should
        // be fine for our purposes.
        let mut rng = rand::thread_rng();
        let mut nonce_bytes = [0u8; aead::NONCE_LEN];
        rng.fill_bytes(&mut nonce_bytes);
        let nonce = aead::Nonce::assume_unique_for_key(nonce_bytes);

        let mut data = value.as_bytes().to_vec();
        encryption_key
            .seal_in_place_append_tag(nonce, aead::Aad::empty(), &mut data)
            .map_err(Into::<Error>::into)
            .context("Fallback encryption failed")?;

        // We serialize this to disk as the 12 byte nonce followed by the message.
        let mut output = Vec::<u8>::with_capacity(aead::NONCE_LEN + data.len());
        output.extend_from_slice(&nonce_bytes);
        output.append(&mut data);

        Ok(output)
    }

    fn fallback_decrypt(&self, value: &[u8]) -> Result<String, Error> {
        if value.len() < aead::NONCE_LEN + 1 {
            return Err(Error::Unknown(anyhow!(
                "Attempting to decrypt too small value for fallback decryption"
            )));
        }

        let encryption_key = self.encryption_key()?;

        // The first 12 bytes of the message are the nonce.
        let nonce_bytes = &value[0..aead::NONCE_LEN];
        let nonce = aead::Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(Into::<Error>::into)
            .context("Failed to parse nonce for fallback decryption")?;

        // The remaining bytes in the message are the data.
        // We convert this to owned b/c the decryption happens in place.
        let mut data_bytes = value[aead::NONCE_LEN..].to_owned();
        let decrypted_length = encryption_key
            .open_in_place(nonce, aead::Aad::empty(), &mut data_bytes)
            .map_err(Into::<Error>::into)
            .context("Fallback decryption failed")?
            .len();

        // The decryption happens in place, but does not resize the vec.
        // Meanwhile, a slice referring to the decrypted data is returned.
        // We use the length of that slice to resize the currently owned Vec,
        // so it can be consumed by String::from_utf8 later on.
        data_bytes.resize(decrypted_length, 0);

        String::from_utf8(data_bytes).map_err(|err| Error::DecodeError(err.utf8_error()))
    }

    fn fallback_file(&self, key: &str) -> Result<PathBuf, Error> {
        let Some(dir) = &self.fallback_dir else {
            return Err(Error::NotFound);
        };
        let filename = format!("{}-{key}", self.service_name);
        let mut path = dir.clone();
        path.push(filename);
        Ok(path)
    }

    /// Writes the encrypted fallback blob for `key` with owner-only (`0o600`)
    /// file permissions.
    ///
    /// `create_dir` additionally creates the fallback directory when it is
    /// missing and forces it to `0o700`; see
    /// [`Self::write_owner_only_fallback_value`].
    fn write_fallback_file(&self, key: &str, value: &str, create_dir: bool) -> Result<(), Error> {
        let fallback_file = self.fallback_file(key)?;

        let encrypted = self.fallback_encrypt(value)?;

        if create_dir {
            let Some(fallback_dir) = fallback_file.parent() else {
                return Err(Error::Unknown(anyhow!(
                    "Invalid fallback secure-storage directory"
                )));
            };
            std::fs::create_dir_all(fallback_dir).map_err(|err| {
                Error::Unknown(anyhow!(
                    "cannot create secure-storage directory {}: {err}",
                    fallback_dir.display()
                ))
            })?;
            let mut dir_permissions = std::fs::metadata(fallback_dir)
                .map_err(|err| Error::Unknown(err.into()))?
                .permissions();
            dir_permissions.set_mode(0o700);
            std::fs::set_permissions(fallback_dir, dir_permissions)
                .map_err(|err| Error::Unknown(err.into()))?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&fallback_file)
            .map_err(|err| {
                Error::Unknown(anyhow!(
                    "cannot write secure-storage file {}: {err}",
                    fallback_file.display()
                ))
            })?;
        // `.mode()` applies only when *this* call creates the file. A blob left
        // behind by an older build is already `0o644` and `truncate(true)` keeps
        // that mode, so tighten explicitly through the open descriptor -- and do
        // it BEFORE `write_all`, so freshly encrypted bytes are never even
        // momentarily present in a group/other-readable file.
        let mut file_permissions = file
            .metadata()
            .map_err(|err| Error::Unknown(err.into()))?
            .permissions();
        file_permissions.set_mode(0o600);
        file.set_permissions(file_permissions)
            .map_err(|err| Error::Unknown(err.into()))?;
        file.write_all(&encrypted)
            .map_err(|err| Error::Unknown(err.into()))
    }

    /// Writes the fallback blob, owner-only, without creating the fallback
    /// directory.
    ///
    /// This is the path every real credential takes when the Secret Service is
    /// unavailable: BYOP API keys, agent-provider secrets, MCP OAuth refresh
    /// tokens and the proxy password all reach it through
    /// [`super::SecureStorage::write_value`]. Upstream -- and this fork until
    /// 2026-08-21 -- used a plain `std::fs::write` here, which takes the process
    /// umask and therefore lands at `0o644` under the default `022`, leaving
    /// every one of those secrets readable by any other local user. The
    /// `0o600` variant existed but its only caller was a non-secret mode enum.
    ///
    /// Deliberately does NOT create or chmod the fallback directory: the
    /// registered fallback dir is `warp_core::paths::state_dir()`, shared with
    /// unrelated application state, so forcing it to `0o700` as a side effect
    /// of writing one secret is a wider change than this path needs. The file
    /// mode alone protects the ciphertext; only the *key names* remain listable.
    fn write_fallback_value(&self, key: &str, value: &str) -> Result<(), Error> {
        self.write_fallback_file(key, value, false)
    }

    /// Like [`Self::write_fallback_value`], but additionally creates the fallback directory
    /// with owner-only permissions (`0o700`) when it does not already exist.
    fn write_owner_only_fallback_value(&self, key: &str, value: &str) -> Result<(), Error> {
        self.write_fallback_file(key, value, true)
    }

    /// Best-effort tightening of an already-written fallback blob to `0o600`.
    ///
    /// Hardening only the write path would leave blobs written by earlier
    /// builds at `0o644` until the credential happens to be rewritten, which
    /// for a long-lived API key may be never. Reads run at startup, so this is
    /// where the migration can actually happen -- and it is a migration that
    /// changes no path, no filename and no ciphertext, so nothing becomes
    /// unreadable.
    ///
    /// Operates on the open descriptor rather than the path, so a symlink
    /// swapped in between the open and the chmod cannot redirect it.
    ///
    /// Failure is deliberately not propagated: the caller already holds the
    /// bytes, and a read-only filesystem must not turn a readable credential
    /// into a missing one.
    fn tighten_fallback_permissions(file: &std::fs::File, path: &std::path::Path) {
        let Ok(metadata) = file.metadata() else {
            return;
        };
        if !metadata.is_file() || metadata.permissions().mode() & 0o077 == 0 {
            return;
        }
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        match file.set_permissions(permissions) {
            Ok(()) => log::info!(
                "Tightened group/other-readable secure-storage fallback file to 0600: {}",
                path.display()
            ),
            Err(err) => log::warn!(
                "Failed to tighten group/other-readable secure-storage fallback file {}: {err}",
                path.display()
            ),
        }
    }

    fn read_fallback_value(&self, key: &str) -> Result<String, Error> {
        let fallback_file = self.fallback_file(key)?;

        let mut file = std::fs::File::open(&fallback_file).map_err(|_| Error::NotFound)?;
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(|_| Error::NotFound)?;
        Self::tighten_fallback_permissions(&file, &fallback_file);
        self.fallback_decrypt(&data)
    }

    fn delete_fallback_value(&self, key: &str) -> Result<(), Error> {
        let fallback_file = self.fallback_file(key)?;
        std::fs::remove_file(fallback_file).map_err(|err| match err {
            ref io_error if io_error.kind() == std::io::ErrorKind::NotFound => Error::NotFound,
            io_error => Error::Unknown(io_error.into()),
        })
    }

    /// Records, at warn level, that a secret is going to the on-disk fallback
    /// rather than to the OS keyring.
    ///
    /// Without this the two backends are indistinguishable from the outside:
    /// a write into the Secret Service and a write of a locally-encrypted blob
    /// both simply succeed. They are not equivalent -- the fallback's key is a
    /// build-time constant in an open-source binary (see [`Self::encryption_key`])
    /// -- so an operator has to be able to tell which one is in use.
    fn warn_fallback_backend(&self, key: &str, err: &Error) {
        log::warn!(
            "Secret Service unavailable ({err}); storing {}/{key} in the local encrypted \
             fallback instead. The fallback file is owner-only (0600), but its encryption \
             key is a build-time constant, so it defends against other local users only -- \
             not against anyone able to read files as this user.",
            self.service_name,
        );
    }

    /// The marker recording that `key` was removed while the keyring could not be used.
    fn tombstone_file(&self, key: &str) -> Result<PathBuf, Error> {
        let mut path = self.fallback_file(key)?;
        path.as_mut_os_string().push(".removed");
        Ok(path)
    }

    /// Records a removal the keyring could not take, so a keyring copy does not come back when
    /// the keyring does (#1805). Empty and owner-only, like the fallback blobs.
    fn write_tombstone(&self, key: &str) -> Result<(), Error> {
        let path = self.tombstone_file(key)?;
        OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map(drop)
            .map_err(|err| {
                Error::Unknown(anyhow!(
                    "cannot write secure-storage removal marker {}: {err}",
                    path.display()
                ))
            })
    }

    fn clear_tombstone(&self, key: &str) {
        if let Ok(path) = self.tombstone_file(key) {
            let _ = std::fs::remove_file(path);
        }
    }

    /// What the fallback holds for `key`. An unreadable blob is reported and treated as absent,
    /// so the keyring copy (if any) is used, as before.
    fn fallback_entry(&self, key: &str) -> FallbackEntry {
        if self.tombstone_file(key).is_ok_and(|path| path.exists()) {
            return FallbackEntry::Removed;
        }
        match self.read_fallback_value(key) {
            Ok(value) => FallbackEntry::Value(value),
            Err(Error::NotFound) => FallbackEntry::Absent,
            Err(err) => {
                log::warn!(
                    "Unreadable secure-storage fallback for {}/{key} ({err:#}); using the keyring",
                    self.service_name
                );
                FallbackEntry::Absent
            }
        }
    }

    /// Writes to the keyring, else to the fallback (`create_dir`: see
    /// [`Self::write_fallback_file`]). Either way a pending removal marker is void.
    fn write_with_fallback(&self, key: &str, value: &str, create_dir: bool) -> Result<(), Error> {
        let result = match self.keyring.write(key, value) {
            Ok(()) => {
                // The keyring holds the value: a fallback copy would shadow it on read.
                let _ = self.delete_fallback_value(key);
                Ok(())
            }
            Err(err) => {
                self.warn_fallback_backend(key, &err);
                self.write_fallback_file(key, value, create_dir)
            }
        };
        if result.is_ok() {
            self.clear_tombstone(key);
        }
        result
    }
}

impl super::SecureStorage for SecureStorage {
    fn write_value(&self, key: &str, value: &str) -> Result<(), Error> {
        self.write_with_fallback(key, value, false)
    }

    fn write_value_with_owner_only_fallback(&self, key: &str, value: &str) -> Result<(), Error> {
        self.write_with_fallback(key, value, true)
    }

    /// The fallback is consulted first (#1805): it only holds a value (or a removal) when the
    /// keyring could not take it, which makes it newer than whatever the keyring holds. When the
    /// keyring is reachable again the fallback's state is moved into it, and the fallback is
    /// deleted only once that succeeded, so a value saved during an outage is never lost to the
    /// keyring's older copy.
    fn read_value(&self, key: &str) -> Result<String, Error> {
        match self.fallback_entry(key) {
            FallbackEntry::Value(value) => {
                if availability::should_contact_keyring() {
                    match self.keyring.write(key, &value) {
                        Ok(()) => {
                            let _ = self.delete_fallback_value(key);
                            log::info!(
                                "Moved {}/{key} from the local fallback into the OS keyring",
                                self.service_name
                            );
                        }
                        Err(err) => log::debug!("{key} stays in the local fallback: {err:#}"),
                    }
                }
                Ok(value)
            }
            FallbackEntry::Removed => {
                if availability::should_contact_keyring() {
                    match self.keyring.delete(key) {
                        Ok(()) | Err(Error::NotFound) => self.clear_tombstone(key),
                        Err(err) => log::debug!("{key} removal stays pending: {err:#}"),
                    }
                }
                Err(Error::NotFound)
            }
            // As before: a keyring that cannot be read, with nothing in the fallback, reads as
            // "not found" (callers that must tell the two apart consult `availability`).
            FallbackEntry::Absent => self.keyring.read(key).map_err(|_| Error::NotFound),
        }
    }

    fn remove_value(&self, key: &str) -> Result<(), Error> {
        let secret_result = self.keyring.delete(key);
        let fs_result = self.delete_fallback_value(key);

        match secret_result {
            Ok(()) => {
                self.clear_tombstone(key);
                Ok(())
            }
            // The keyring answered and had nothing: the fallback decides, as before.
            Err(Error::NotFound) => {
                self.clear_tombstone(key);
                fs_result.map_err(|_| Error::NotFound)
            }
            // The keyring could not be reached: a copy it still holds must stay removed when it
            // comes back (#1805), so the removal is recorded in the fallback.
            Err(secret_err) => match self.write_tombstone(key) {
                Ok(()) => Ok(()),
                Err(_) => fs_result.map_err(|_| secret_err),
            },
        }
    }
}

/// The Secret Service side of [`SecureStorage`].
struct SecretServiceKeyring {
    /// The value of the "service" attribute (the app's secrets namespace).
    service_name: String,

    /// A lazily-initialized reference to the default secret collection as
    /// provided by the installed Secret Service API provider. Only a
    /// successful open is kept; a failed one is recorded process-wide in
    /// [`availability`] instead, so every handle (the app's and each
    /// worker's) learns of it from one failing attempt (#1805).
    collection: OnceCell<Collection>,

    /// Opens the default collection. [`open_default_collection_bounded`] outside tests.
    open_collection: fn() -> Result<Collection, Error>,
}

impl SecretServiceKeyring {
    fn new(service_name: &str, open_collection: fn() -> Result<Collection, Error>) -> Self {
        Self {
            service_name: service_name.to_owned(),
            collection: OnceCell::new(),
            open_collection,
        }
    }

    /// Returns a reference to the default secret collection, lazily
    /// instantiating the underlying service and collection reference,
    /// returning an error if the connection cannot be established or the
    /// collection cannot be opened or unlocked.
    ///
    /// A failure is cached process-wide, not per handle (#1805): once the
    /// keyring is known to be unavailable this returns an error at once,
    /// without touching D-Bus, and every caller uses the fallback file. The
    /// failure is logged once per run. Only an explicit retry
    /// ([`availability::retry`]) or a late answer from the service makes the
    /// next call try again.
    fn collection(&self) -> Result<&secret_service::blocking::Collection<'_>, Error> {
        if !availability::should_contact_keyring() {
            return Err(keyring_unavailable());
        }
        let collection = match self.collection.get() {
            Some(collection) => collection,
            None => match (self.open_collection)() {
                Ok(collection) => self.collection.get_or_init(|| collection),
                Err(err) => {
                    if availability::mark_unavailable() {
                        log::error!(
                            "Failed to acquire default Secret Service collection: {err:#}. \
                             Stored credentials come from the local fallback for the rest of \
                             this run; the keyring is not tried again unless asked to."
                        );
                    } else {
                        log::debug!("Secret Service collection still unavailable: {err:#}");
                    }
                    return Err(keyring_unavailable());
                }
            },
        };
        let collection = collection.borrow_collection();
        // Ensure that the collection is unlocked, otherwise we will be unable
        // add or modify items.
        if let Err(err) = collection.unlock() {
            // A locked collection whose unlock was refused (or cannot be prompted
            // for) is as unusable as a missing one: do not prompt again per lookup.
            if availability::mark_unavailable() {
                log::error!(
                    "Failed to unlock the default Secret Service collection: {err:#}. \
                     Stored credentials come from the local fallback for the rest of this run."
                );
            }
            return Err(err.into());
        }
        availability::mark_available();
        Ok(collection)
    }

    /// Returns the set of attributes which should be used when interacting
    /// with a secret item that is identified by the given key.
    fn attributes_for_key<'a>(&'a self, key: &'a str) -> HashMap<&'static str, &'a str> {
        HashMap::from([
            // Ensure our keys don't conflict with ones stored by another
            // application.
            ("service", self.service_name.as_str()),
            // Specify the key for the secret.
            ("key", key),
        ])
    }

    /// Provides the given function access to a secret item with the given key
    /// in order to read or manipulate the item.
    fn with_item<T>(
        &self,
        key: &str,
        func: impl FnOnce(&Item) -> Result<T, Error>,
    ) -> Result<T, Error> {
        let collection = self.collection()?;
        let items = collection.search_items(self.attributes_for_key(key))?;
        let Some(item) = items.first() else {
            return Err(Error::NotFound);
        };
        func(item)
    }
}

impl Keyring for SecretServiceKeyring {
    fn read(&self, key: &str) -> Result<String, Error> {
        self.with_item(key, |item| {
            let bytes = item.get_secret()?;
            String::from_utf8(bytes).map_err(|err| Error::DecodeError(err.utf8_error()))
        })
    }

    fn write(&self, key: &str, value: &str) -> Result<(), Error> {
        let collection = self.collection()?;
        // Construct a slightly more human-readable label for the secret than
        // using the key alone.
        let label = format!("{}: {key}", self.service_name);
        collection.create_item(
            &label,
            self.attributes_for_key(key),
            value.as_bytes(),
            // replace the existing key, if any
            true,
            "text/plain",
        )?;
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), Error> {
        self.with_item(key, |item| item.delete().map_err(Into::into))
    }
}

impl From<secret_service::Error> for Error {
    fn from(value: secret_service::Error) -> Self {
        // TODO(vorporeal): Check to see if we can return any more specific
        // values.
        Error::Unknown(anyhow!(value))
    }
}

impl From<ring::error::Unspecified> for Error {
    fn from(value: ring::error::Unspecified) -> Self {
        Error::Unknown(anyhow!(value))
    }
}

fn keyring_unavailable() -> Error {
    Error::Unknown(anyhow!("the Secret Service keyring is unavailable"))
}

/// The production collection opener: [`Collection::open_default_collection`] on a helper
/// thread, bounded by [`CONNECT_GIVE_UP`], so a bus that never answers costs that long once
/// instead of the bus's activation timeout. The opened collection is handed back to the caller,
/// so no second, unbounded open follows a successful probe.
fn open_default_collection_bounded() -> Result<Collection, Error> {
    let open = || Collection::open_default_collection().map_err(|err| format!("{err:#}"));
    match probe_with_give_up(open, CONNECT_GIVE_UP) {
        Probe::Answered(Ok(collection)) => Ok(collection),
        Probe::Answered(Err(err)) => Err(Error::Unknown(anyhow!(err))),
        Probe::TimedOut => Err(Error::Unknown(anyhow!(
            "the Secret Service did not answer within {CONNECT_GIVE_UP:?}"
        ))),
        Probe::StillPending => Err(Error::Unknown(anyhow!(
            "the Secret Service has not answered an earlier request yet"
        ))),
    }
}

/// What [`probe_with_give_up`] learned.
#[derive(Debug, PartialEq, Eq)]
enum Probe<T> {
    /// The probe returned in time.
    Answered(Result<T, String>),
    /// It did not; the keyring has been marked unavailable. The probe keeps running on its own
    /// thread, and if it later succeeds the keyring is marked available again.
    TimedOut,
    /// An earlier probe that timed out is still running (the service is still hung): no second
    /// probe thread was started. Its own late answer is what can mark the keyring available.
    StillPending,
}

/// Set while a probe that the caller gave up on is still running (#1805 review): a retry must
/// not pile up one more stuck thread per attempt against a service that is still hung.
static HUNG_PROBE: AtomicBool = AtomicBool::new(false);

/// Runs `probe` on a helper thread and waits at most `give_up` for it. A blocking D-Bus call
/// cannot be cancelled, so a probe that overruns is left to finish on its own; its late answer is
/// not lost: success marks the keyring available (the service appeared; the late result itself
/// is dropped), failure changes nothing. While such a probe is still running, later calls return
/// [`Probe::StillPending`] without starting another.
fn probe_with_give_up<T: Send + 'static>(
    probe: impl FnOnce() -> Result<T, String> + Send + 'static,
    give_up: Duration,
) -> Probe<T> {
    if HUNG_PROBE.load(Ordering::SeqCst) {
        return Probe::StillPending;
    }
    // `true` once the caller stopped waiting. The helper reads it and sends under this lock, and
    // the caller sets it (and marks the keyring unavailable) under the same lock, so a late
    // success can never be overtaken by the caller's "unavailable".
    let gave_up = Arc::new(Mutex::new(false));
    let (tx, rx) = mpsc::sync_channel::<Result<T, String>>(1);
    let helper_gave_up = gave_up.clone();
    let spawned = std::thread::Builder::new()
        .name("secret-service-probe".into())
        .spawn(move || {
            let result = probe();
            let gave_up = helper_gave_up.lock().unwrap_or_else(|e| e.into_inner());
            if *gave_up {
                HUNG_PROBE.store(false, Ordering::SeqCst);
                if result.is_ok() && availability::mark_available() {
                    log::info!("The Secret Service answered late; using the OS keyring again.");
                }
                return;
            }
            let _ = tx.send(result);
        });
    if let Err(err) = spawned {
        return Probe::Answered(Err(format!(
            "could not start the Secret Service probe: {err}"
        )));
    }
    match rx.recv_timeout(give_up) {
        Ok(result) => Probe::Answered(result),
        Err(mpsc::RecvTimeoutError::Disconnected) => {
            Probe::Answered(Err("the Secret Service probe stopped".to_owned()))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            let mut gave_up = gave_up.lock().unwrap_or_else(|e| e.into_inner());
            // It may have answered between the timeout and taking the lock.
            if let Ok(result) = rx.try_recv() {
                return Probe::Answered(result);
            }
            *gave_up = true;
            HUNG_PROBE.store(true, Ordering::SeqCst);
            if availability::mark_unavailable() {
                log::error!(
                    "Failed to acquire default Secret Service collection: the service did not \
                     answer within {give_up:?}. Stored credentials come from the local fallback \
                     for the rest of this run, unless the service answers later."
                );
            }
            Probe::TimedOut
        }
    }
}

/// A helper structure that maintains access to the default collection.
///
/// [`secret_service::SecretService`] is a self-referential struct that leaks
/// its internal reference lifetime, which is why we use [`ouroboros`] here to
/// provide a safe API for interacting with the service and collection.
#[ouroboros::self_referencing]
struct Collection {
    /// An encrypted dbus connection to the Secret Service API provider.
    #[borrows()]
    #[covariant]
    service: SecretService<'this>,

    /// A reference to the default secret collection, which can be used to
    /// add, remove and read secrets.
    #[borrows(service)]
    #[covariant]
    collection: secret_service::blocking::Collection<'this>,
}

impl Collection {
    /// Tries to open the default secret collection via the Secret Service
    /// API.
    fn open_default_collection() -> Result<Self, Error> {
        SecretService::connect(EncryptionType::Plain)
            .and_then(|service| {
                CollectionTryBuilder {
                    service,
                    collection_builder: |service| service.get_default_collection(),
                }
                .try_build()
            })
            .map_err(Into::into)
    }
}

#[cfg(test)]
#[path = "linux_test.rs"]
mod tests;
