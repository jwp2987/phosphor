use super::Error;
use super::SecureStorage;

/// The legacy format (the key compiled into the binary) must stay readable: blobs written before
/// #1932 are still on disk.
#[test]
fn test_encrypt_decrypt_returns_same_value() {
    let storage = SecureStorage::new("darmok");

    let input = String::from("darmok and jalad at tanagra");
    let encrypted = storage.legacy_encrypt(&input).unwrap();
    let output = storage.legacy_decrypt(&encrypted).unwrap();

    assert_eq!(input, output)
}

#[test]
fn test_encrypt_decrypt_works_across_storage_instances() {
    let storage_1 = SecureStorage::new("darmok");
    let storage_2 = SecureStorage::new("jalad");

    let input = String::from("shaka when the walls fell");
    let encrypted = storage_1.legacy_encrypt(&input).unwrap();
    let output = storage_2.legacy_decrypt(&encrypted).unwrap();

    assert_eq!(input, output)
}

#[test]
fn test_decrypt_fails_on_malformed_data() {
    let storage = SecureStorage::new("darmok");

    let bad_datas: [&[u8]; 4] = [&[], &[0; 1], &[0; 11], &[0; 12]];

    for bad_data in bad_datas {
        let result = storage.legacy_decrypt(bad_data);

        assert!(result.is_err());
        let error = result.unwrap_err();
        let Error::Unknown(err) = error else {
            panic!("Expected error variant to be Error::Unknown, but found {error:?}")
        };
        assert_eq!(
            format!("{err}"),
            "Attempting to decrypt too small value for fallback decryption"
        );
    }
}

#[test]
fn default_fallback_does_not_create_missing_directory() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("secure-storage");
    let storage = SecureStorage::new_with_fallback("darmok", fallback_dir.clone());

    assert!(storage.write_fallback_value("key", "value").is_err());
    assert!(!fallback_dir.exists());
}

#[test]
fn fallback_value_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("secure-storage");
    let storage = SecureStorage::new_with_fallback("darmok", fallback_dir.clone());
    storage
        .write_owner_only_fallback_value("key", "value")
        .expect("fallback write");
    let dir_mode = std::fs::metadata(&fallback_dir)
        .expect("directory metadata")
        .permissions()
        .mode()
        & 0o777;
    let file_mode = std::fs::metadata(storage.fallback_file("key").expect("fallback file"))
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dir_mode, 0o700);
    assert_eq!(file_mode, 0o600);
}

/// The *default* fallback path -- the one every real credential takes when the
/// Secret Service is unavailable -- must be owner-only too.
///
/// Before 2026-08-21 this path was a plain `std::fs::write`, i.e. `0o644` under
/// the default umask, while the `0o600` variant covered only a non-secret mode
/// enum. `fallback_value_is_owner_only` above tested the variant nobody used,
/// so it could not fail on the exposure.
#[test]
fn default_fallback_value_is_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("secure-storage");
    std::fs::create_dir_all(&fallback_dir).expect("create fallback dir");
    std::fs::set_permissions(&fallback_dir, std::fs::Permissions::from_mode(0o755))
        .expect("relax directory mode");
    let storage = SecureStorage::new_with_fallback("darmok", fallback_dir.clone());

    storage
        .write_fallback_value("key", "value")
        .expect("fallback write");

    let file_mode = std::fs::metadata(storage.fallback_file("key").expect("fallback file"))
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        file_mode, 0o600,
        "fallback blob must not be group/other readable"
    );

    // The registered fallback directory is `paths::state_dir()`, shared with
    // unrelated state, so the default path must NOT re-mode it as a side effect.
    let dir_mode = std::fs::metadata(&fallback_dir)
        .expect("directory metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        dir_mode, 0o755,
        "default fallback path must not chmod the shared state dir"
    );
}

/// A blob left at `0o644` by an older build is tightened when it is rewritten.
///
/// `OpenOptions::mode` only applies when the call itself creates the file, and
/// `truncate(true)` preserves the existing mode, so this needs the explicit
/// `set_permissions` on the descriptor.
#[test]
fn rewriting_an_existing_world_readable_fallback_tightens_it() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("secure-storage");
    std::fs::create_dir_all(&fallback_dir).expect("create fallback dir");
    let storage = SecureStorage::new_with_fallback("darmok", fallback_dir);
    let path = storage.fallback_file("key").expect("fallback file");

    // Simulate the pre-hardening writer.
    std::fs::write(&path, b"stale").expect("seed legacy blob");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .expect("seed legacy mode");

    storage
        .write_fallback_value("key", "value")
        .expect("fallback write");

    let mode = std::fs::metadata(&path)
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(
        storage.read_fallback_value("key").expect("read back").value,
        "value"
    );
}

/// Migration for blobs that are never rewritten: reading a `0o644` fallback
/// file tightens it in place. (Re-sealing a legacy blob with the per-install key is a
/// separate step of `read_value`: see `a_legacy_blob_is_resealed_...` below.)
///
/// A long-lived API key may never be rewritten, so hardening only the write
/// path would leave the existing exposure standing indefinitely.
#[test]
fn reading_a_world_readable_fallback_migrates_it_to_owner_only() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("secure-storage");
    std::fs::create_dir_all(&fallback_dir).expect("create fallback dir");
    let storage = SecureStorage::new_with_fallback("darmok", fallback_dir);
    let path = storage.fallback_file("key").expect("fallback file");

    // A blob written by a pre-hardening build: same path, same ciphertext, 0644.
    let encrypted = storage
        .legacy_encrypt("shaka when the walls fell")
        .expect("encrypt");
    std::fs::write(&path, &encrypted).expect("seed legacy blob");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .expect("seed legacy mode");

    // The credential must still be readable -- the migration must not orphan it.
    assert_eq!(
        storage.read_fallback_value("key").expect("read").value,
        "shaka when the walls fell"
    );

    let mode = std::fs::metadata(&path)
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(
        mode, 0o600,
        "read must migrate a legacy world-readable blob"
    );
}

/// The migration must not disturb a file that is already owner-only, and must
/// not fabricate one that does not exist.
#[test]
fn reading_an_absent_or_already_tight_fallback_is_unchanged() {
    use std::os::unix::fs::PermissionsExt as _;

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("secure-storage");
    std::fs::create_dir_all(&fallback_dir).expect("create fallback dir");
    let storage = SecureStorage::new_with_fallback("darmok", fallback_dir);

    assert!(matches!(
        storage.read_fallback_value("missing"),
        Err(Error::NotFound)
    ));
    assert!(!storage.fallback_file("missing").expect("path").exists());

    storage
        .write_fallback_value("key", "value")
        .expect("fallback write");
    let path = storage.fallback_file("key").expect("fallback file");
    assert_eq!(
        storage.read_fallback_value("key").expect("read").value,
        "value"
    );
    let mode = std::fs::metadata(&path)
        .expect("file metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

/// #861: a fallback directory that is a file is an error naming the path, not
/// a bare "Not a directory (os error 20)", and nothing is overwritten.
#[test]
fn test_fallback_write_error_names_the_path_when_dir_is_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let not_a_dir = dir.path().join("state");
    std::fs::write(&not_a_dir, b"keep me").unwrap();
    let storage = SecureStorage::new_with_fallback("darmok", not_a_dir.clone());

    let error = storage
        .write_owner_only_fallback_value("key", "value")
        .unwrap_err();
    let message = format!("{error:?}");
    assert!(
        message.contains(&not_a_dir.display().to_string()),
        "error should name the path: {message}"
    );
    assert_eq!(std::fs::read(&not_a_dir).unwrap(), b"keep me");
}

/// Counts calls to [`failing_opener`]; read only while holding the availability test guard.
static OPEN_ATTEMPTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A collection opener that fails like a session with no Secret Service.
fn failing_opener() -> Result<super::Collection, Error> {
    OPEN_ATTEMPTS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    Err(Error::Unknown(anyhow::anyhow!(
        "org.freedesktop.secrets was not provided by any .service files"
    )))
}

/// #1805: with no keyring, the first lookup on ANY handle in the process records that, and no
/// later lookup -- on that handle or another (the app's handle and each worker's) -- contacts
/// the keyring again. Every lookup is served by the fallback file, with its owner-only mode.
#[test]
fn an_unavailable_keyring_is_tried_once_per_run_across_handles() {
    use super::super::SecureStorage as _;
    use std::os::unix::fs::PermissionsExt as _;
    use std::sync::atomic::Ordering;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    OPEN_ATTEMPTS.store(0, Ordering::SeqCst);

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&fallback_dir).expect("create fallback dir");
    let ui = SecureStorage::with_opener("darmok", Some(fallback_dir.clone()), failing_opener);
    let worker = SecureStorage::with_opener("darmok", Some(fallback_dir.clone()), failing_opener);

    assert!(matches!(ui.read_value("key"), Err(Error::NotFound)));
    assert!(matches!(ui.read_value("key"), Err(Error::NotFound)));
    assert!(matches!(worker.read_value("key"), Err(Error::NotFound)));
    worker.write_value("key", "value").expect("fallback write");
    assert_eq!(ui.read_value("key").expect("read back"), "value");
    assert_eq!(
        OPEN_ATTEMPTS.load(Ordering::SeqCst),
        1,
        "one failing keyring attempt per run, not one per lookup or per handle"
    );
    assert_eq!(
        super::availability::current(),
        super::availability::Availability::Unavailable
    );

    // The fallback keeps its semantics: owner-only blob, readable, removable.
    let mode = std::fs::metadata(worker.fallback_file("key").expect("path"))
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    worker.remove_value("key").expect("remove from fallback");
    assert!(matches!(ui.read_value("key"), Err(Error::NotFound)));
    assert_eq!(OPEN_ATTEMPTS.load(Ordering::SeqCst), 1);

    // Only an explicit retry contacts the keyring again -- once.
    assert!(super::availability::retry());
    assert!(matches!(ui.read_value("key"), Err(Error::NotFound)));
    assert!(matches!(worker.read_value("key"), Err(Error::NotFound)));
    assert_eq!(OPEN_ATTEMPTS.load(Ordering::SeqCst), 2);

    super::availability::reset_for_test();
}

/// #1805: a Secret Service that never answers costs the give-up once; the probe's late success
/// marks the keyring available again (the service appeared).
#[test]
fn a_probe_that_does_not_answer_is_given_up_on_and_a_late_success_is_kept() {
    use std::time::{Duration, Instant};

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();

    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let started = Instant::now();
    let outcome = super::probe_with_give_up(
        move || {
            let _ = release_rx.recv();
            Ok(())
        },
        Duration::from_millis(100),
    );
    assert_eq!(outcome, super::Probe::TimedOut);
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the caller waits for the give-up only"
    );
    assert_eq!(
        super::availability::current(),
        super::availability::Availability::Unavailable
    );

    // While that probe is still stuck, a retry does not start another thread against the
    // same hung service.
    let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ran_in_probe = ran.clone();
    let second = super::probe_with_give_up(
        move || {
            ran_in_probe.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        },
        Duration::from_millis(100),
    );
    assert_eq!(second, super::Probe::StillPending);
    assert!(!ran.load(std::sync::atomic::Ordering::SeqCst));

    release_tx.send(()).expect("release the probe");
    let deadline = Instant::now() + Duration::from_secs(10);
    while super::availability::current() != super::availability::Availability::Available {
        assert!(Instant::now() < deadline, "the late success was lost");
        std::thread::sleep(Duration::from_millis(5));
    }
    super::availability::reset_for_test();
}

/// A probe that answers in time reports its result and leaves the state to the caller.
#[test]
fn a_probe_that_answers_in_time_reports_its_result() {
    use std::time::Duration;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    assert_eq!(
        super::probe_with_give_up(
            || Err::<(), String>("no such name".to_owned()),
            Duration::from_secs(5)
        ),
        super::Probe::Answered(Err("no such name".to_owned()))
    );
    assert_eq!(
        super::probe_with_give_up(|| Ok(()), Duration::from_secs(5)),
        super::Probe::Answered(Ok(()))
    );
    assert_eq!(
        super::availability::current(),
        super::availability::Availability::Unknown
    );
}

/// A keyring stand-in that consults the process-wide availability like the real one, and can
/// be taken down and brought back.
struct FakeKeyring {
    data: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>>,
    up: std::rc::Rc<std::cell::Cell<bool>>,
}

impl FakeKeyring {
    fn check(&self) -> Result<(), Error> {
        if !super::availability::should_contact_keyring() {
            return Err(Error::Unknown(anyhow::anyhow!("unavailable")));
        }
        if !self.up.get() {
            super::availability::mark_unavailable();
            return Err(Error::Unknown(anyhow::anyhow!("no Secret Service")));
        }
        super::availability::mark_available();
        Ok(())
    }
}

impl super::Keyring for FakeKeyring {
    fn read(&self, key: &str) -> Result<String, Error> {
        self.check()?;
        self.data.borrow().get(key).cloned().ok_or(Error::NotFound)
    }
    fn write(&self, key: &str, value: &str) -> Result<(), Error> {
        self.check()?;
        self.data
            .borrow_mut()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), Error> {
        self.check()?;
        self.data
            .borrow_mut()
            .remove(key)
            .map(drop)
            .ok_or(Error::NotFound)
    }
}

type FakeHandles = (
    SecureStorage,
    std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>>,
    std::rc::Rc<std::cell::Cell<bool>>,
    tempfile::TempDir,
);

/// A storage over a [`FakeKeyring`] that already holds `key = K0`, with a fallback directory.
fn storage_with_fake_keyring() -> FakeHandles {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let fallback_dir = temp_dir.path().join("state");
    std::fs::create_dir_all(&fallback_dir).expect("create fallback dir");
    let data = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::from([
        ("key".to_owned(), "K0".to_owned()),
    ])));
    let up = std::rc::Rc::new(std::cell::Cell::new(true));
    let storage = SecureStorage::with_keyring(
        "darmok",
        Some(fallback_dir),
        Box::new(FakeKeyring {
            data: data.clone(),
            up: up.clone(),
        }),
    );
    (storage, data, up, temp_dir)
}

/// #1805 review: a value saved during an outage survives the keyring's return. The read returns
/// it (not the keyring's older K0), moves it into the keyring, and only then deletes the fallback.
#[test]
fn a_value_saved_during_an_outage_wins_over_the_keyring_copy_on_recovery() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let (storage, data, up, _dir) = storage_with_fake_keyring();

    up.set(false);
    storage.write_value("key", "K1").expect("outage write");
    assert_eq!(storage.read_value("key").expect("outage read"), "K1");
    let fallback = storage.fallback_file("key").expect("path");
    assert!(fallback.exists(), "the outage write went to the fallback");
    assert_eq!(data.borrow()["key"], "K0");

    // The keyring comes back (an explicit retry here; a late answer works the same way).
    up.set(true);
    super::availability::retry();
    assert_eq!(storage.read_value("key").expect("recovery read"), "K1");
    assert_eq!(
        data.borrow()["key"],
        "K1",
        "the keyring now holds the newer value"
    );
    assert!(
        !fallback.exists(),
        "the fallback is gone once the keyring took it"
    );
    assert_eq!(storage.read_value("key").expect("later read"), "K1");
    super::availability::reset_for_test();
}

/// If the keyring is back but refuses the migration write, the fallback is kept (and still read).
#[test]
fn the_fallback_is_kept_until_the_keyring_has_taken_it() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let (storage, data, up, _dir) = storage_with_fake_keyring();

    up.set(false);
    storage.write_value("key", "K1").expect("outage write");
    super::availability::retry();
    // Still down: the migration write fails.
    assert_eq!(storage.read_value("key").expect("read"), "K1");
    assert!(storage.fallback_file("key").expect("path").exists());
    assert_eq!(data.borrow()["key"], "K0");
    super::availability::reset_for_test();
}

/// #1805 review: a removal made during an outage is not undone by the keyring's copy on recovery.
#[test]
fn a_removal_made_during_an_outage_stays_removed_on_recovery() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let (storage, data, up, _dir) = storage_with_fake_keyring();

    up.set(false);
    storage
        .remove_value("key")
        .expect("outage removal is recorded");
    assert!(matches!(storage.read_value("key"), Err(Error::NotFound)));

    up.set(true);
    super::availability::retry();
    assert!(
        matches!(storage.read_value("key"), Err(Error::NotFound)),
        "the keyring's K0 must not come back"
    );
    assert!(
        data.borrow().get("key").is_none(),
        "the removal reached the keyring"
    );
    assert!(
        !storage.tombstone_file("key").expect("path").exists(),
        "the marker is gone once the keyring took the removal"
    );

    // A later write is an ordinary keyring write again.
    storage.write_value("key", "K2").expect("write");
    assert_eq!(storage.read_value("key").expect("read"), "K2");
    assert_eq!(data.borrow()["key"], "K2");
    super::availability::reset_for_test();
}

// ---- #1932: the fallback key comes from a per-install secret, never from the binary ----

/// A keyring that answers every call, but will not store `refused`.
struct PickyKeyring {
    data: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>>,
    refused: &'static str,
}

impl super::Keyring for PickyKeyring {
    fn read(&self, key: &str) -> Result<String, Error> {
        super::availability::mark_available();
        self.data.borrow().get(key).cloned().ok_or(Error::NotFound)
    }
    fn write(&self, key: &str, value: &str) -> Result<(), Error> {
        super::availability::mark_available();
        if key == self.refused {
            return Err(Error::Unknown(anyhow::anyhow!(
                "the keyring refused the item"
            )));
        }
        self.data
            .borrow_mut()
            .insert(key.to_owned(), value.to_owned());
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), Error> {
        super::availability::mark_available();
        self.data
            .borrow_mut()
            .remove(key)
            .map(drop)
            .ok_or(Error::NotFound)
    }
}

/// A storage whose keyring is down, over an existing fallback directory.
fn storage_without_keyring(fallback_dir: &std::path::Path) -> SecureStorage {
    SecureStorage::with_keyring(
        "darmok",
        Some(fallback_dir.to_owned()),
        Box::new(FakeKeyring {
            data: Default::default(),
            up: std::rc::Rc::new(std::cell::Cell::new(false)),
        }),
    )
}

fn mode_of(path: &std::path::Path) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

/// The names in `dir`, sorted: proves no temporary file is left behind.
fn names_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// Key derivation: the same secret, source and service give the same key; changing any one of
/// them gives a key that does not open the other's blob.
#[test]
fn the_fallback_key_depends_on_the_secret_the_source_and_the_service() {
    use super::{KeySource, derive_key, open_sealed, seal_value};

    let secret = [7u8; super::INSTALL_SECRET_LEN];
    let other_secret = [8u8; super::INSTALL_SECRET_LEN];
    let key = derive_key("darmok", KeySource::KeyFile, &secret).expect("key");
    let sealed = seal_value(&key, b"aad", "temba, his arms wide").expect("seal");

    let same = derive_key("darmok", KeySource::KeyFile, &secret).expect("key");
    assert_eq!(
        open_sealed(&same, b"aad", &sealed).expect("same inputs, same key"),
        "temba, his arms wide"
    );
    for (what, different) in [
        (
            "secret",
            derive_key("darmok", KeySource::KeyFile, &other_secret),
        ),
        ("source", derive_key("darmok", KeySource::Keyring, &secret)),
        ("service", derive_key("jalad", KeySource::KeyFile, &secret)),
    ] {
        let different = different.expect("key");
        assert!(
            open_sealed(&different, b"aad", &sealed).is_err(),
            "a different {what} must give a different key"
        );
    }
    assert!(
        open_sealed(&same, b"other aad", &sealed).is_err(),
        "the associated data is authenticated"
    );
}

#[test]
fn the_install_secret_round_trips_through_its_keyring_encoding() {
    use super::{decode_install_secret, encode_hex};

    let mut secret = [0u8; super::INSTALL_SECRET_LEN];
    for (index, byte) in secret.iter_mut().enumerate() {
        *byte = (index * 9) as u8;
    }
    secret[31] = 0xff;
    let encoded = encode_hex(&secret);
    assert_eq!(encoded.len(), 64);
    assert_eq!(decode_install_secret(&encoded), Some(secret));
    assert_eq!(decode_install_secret(&encoded[..62]), None);
    assert_eq!(decode_install_secret(&encoded.replace('f', "g")), None);
    assert_eq!(decode_install_secret(""), None);
}

/// No keyring: a write creates the owner-only key file and seals with its key. The blob says so
/// in its header, the key compiled into the binary does not open it, and another handle on the
/// same directory (the worker's, a second app's) reads it.
#[test]
fn without_a_keyring_a_value_is_sealed_with_the_key_file_never_the_built_in_key() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let storage = storage_without_keyring(temp_dir.path());

    storage
        .write_value("key", "sokath, his eyes uncovered")
        .expect("write");

    let key_file = storage.key_file().expect("key file path");
    assert_eq!(
        std::fs::read(&key_file).expect("key file").len(),
        super::INSTALL_SECRET_LEN
    );
    assert_eq!(mode_of(&key_file), 0o600);
    let blob = std::fs::read(storage.fallback_file("key").expect("path")).expect("blob");
    assert!(blob.starts_with(b"PSF1F"), "header: magic and key-file tag");
    assert!(
        storage.legacy_decrypt(&blob).is_err(),
        "the key compiled into the binary must not open a new blob"
    );
    assert!(
        !blob
            .windows(b"sokath".len())
            .any(|window| window == b"sokath"),
        "the value is not stored in the clear"
    );
    assert_eq!(
        names_in(temp_dir.path()),
        ["darmok-key", "darmok-secure-storage.key"]
    );

    let other = storage_without_keyring(temp_dir.path());
    assert_eq!(
        other.read_value("key").expect("read"),
        "sokath, his eyes uncovered"
    );
    super::availability::reset_for_test();
}

/// Migration: a legacy blob read with no keyring is re-sealed with the key file's key in place.
/// The value is identical, before and after, and for a handle that never saw the legacy blob.
#[test]
fn a_legacy_blob_is_resealed_with_the_key_file_on_read() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let storage = storage_without_keyring(temp_dir.path());
    let path = storage.fallback_file("key").expect("path");
    let legacy = storage
        .legacy_encrypt("mirab, his sails unfurled")
        .expect("legacy blob");
    std::fs::write(&path, &legacy).expect("seed legacy blob");

    assert_eq!(
        storage.read_value("key").expect("first read"),
        "mirab, his sails unfurled"
    );

    let blob = std::fs::read(&path).expect("blob");
    assert!(blob.starts_with(b"PSF1F"), "the blob was re-sealed");
    assert!(storage.legacy_decrypt(&blob).is_err());
    assert_eq!(mode_of(&path), 0o600);
    assert_eq!(
        names_in(temp_dir.path()),
        ["darmok-key", "darmok-secure-storage.key"],
        "no temporary file is left"
    );
    let reread = storage.read_fallback_value("key").expect("second read");
    assert_eq!(reread.value, "mirab, his sails unfurled");
    assert!(!reread.legacy);
    assert_eq!(
        storage_without_keyring(temp_dir.path())
            .read_value("key")
            .expect("another handle"),
        "mirab, his sails unfurled"
    );
    super::availability::reset_for_test();
}

/// Migration must never cost the value: when the key file cannot be made (here its name is taken
/// by a directory), the legacy blob stays byte for byte and still reads.
#[test]
fn a_legacy_blob_survives_a_failed_reseal() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let storage = storage_without_keyring(temp_dir.path());
    let path = storage.fallback_file("key").expect("path");
    let legacy = storage.legacy_encrypt("K-legacy").expect("legacy blob");
    std::fs::write(&path, &legacy).expect("seed legacy blob");
    std::fs::create_dir(storage.key_file().expect("key file path")).expect("block the key file");

    assert_eq!(storage.read_value("key").expect("read"), "K-legacy");
    assert_eq!(std::fs::read(&path).expect("blob"), legacy);
    assert_eq!(storage.read_value("key").expect("read again"), "K-legacy");
    super::availability::reset_for_test();
}

/// A legacy blob the keyring takes is moved there (as any fallback value is) and not re-sealed:
/// no key file appears.
#[test]
fn a_legacy_blob_moves_into_a_working_keyring_without_a_key_file() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let (storage, data, _up, dir) = storage_with_fake_keyring();
    let path = storage.fallback_file("key").expect("path");
    std::fs::write(&path, storage.legacy_encrypt("K-legacy").expect("blob")).expect("seed");

    assert_eq!(storage.read_value("key").expect("read"), "K-legacy");
    assert_eq!(data.borrow()["key"], "K-legacy");
    assert!(names_in(&dir.path().join("state")).is_empty());
    super::availability::reset_for_test();
}

/// A blob whose key file is gone is an error that is not "not found", the blob is untouched, and
/// no new key file takes the old one's place (it could not open the blob, and would hide the
/// loss). The same for a key file of the wrong length, which is never overwritten.
#[test]
fn a_lost_or_damaged_key_file_is_reported_and_never_replaced_on_read() {
    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let writer = storage_without_keyring(temp_dir.path());
    writer.write_fallback_value("key", "value").expect("write");
    let key_file = writer.key_file().expect("key file path");
    let blob_path = writer.fallback_file("key").expect("path");
    let blob = std::fs::read(&blob_path).expect("blob");
    let secret = std::fs::read(&key_file).expect("key file");

    std::fs::remove_file(&key_file).expect("lose the key file");
    let reader = storage_without_keyring(temp_dir.path());
    let error = reader.read_fallback_value("key").err().expect("unreadable");
    assert!(
        matches!(error, Error::Unknown(_)),
        "a lost key is an error, not NotFound: {error:?}"
    );
    assert!(!key_file.exists(), "a read must not create a new key file");
    assert_eq!(std::fs::read(&blob_path).expect("blob"), blob);

    std::fs::write(&key_file, &secret[..16]).expect("truncated key file");
    let reader = storage_without_keyring(temp_dir.path());
    assert!(matches!(
        reader.read_fallback_value("key"),
        Err(Error::Unknown(_))
    ));
    assert!(
        reader.write_fallback_value("other", "value").is_err(),
        "a write is refused rather than sealed with a replaced or built-in key"
    );
    assert_eq!(std::fs::read(&key_file).expect("key file"), &secret[..16]);
    assert!(!reader.fallback_file("other").expect("path").exists());

    // With the key file back, the blob reads again.
    std::fs::write(&key_file, &secret).expect("restore the key file");
    assert_eq!(
        storage_without_keyring(temp_dir.path())
            .read_fallback_value("key")
            .expect("read")
            .value,
        "value"
    );
    super::availability::reset_for_test();
}

/// A blob with a flipped byte fails authentication: an error, in the header as in the body.
#[test]
fn a_tampered_blob_is_an_error() {
    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let storage = storage_without_keyring(temp_dir.path());
    storage.write_fallback_value("key", "value").expect("write");
    let path = storage.fallback_file("key").expect("path");
    let blob = std::fs::read(&path).expect("blob");

    for index in [4, blob.len() - 1] {
        let mut tampered = blob.clone();
        tampered[index] ^= 0x01;
        std::fs::write(&path, &tampered).expect("tamper");
        assert!(
            matches!(storage.read_fallback_value("key"), Err(Error::Unknown(_))),
            "byte {index}"
        );
    }
    super::availability::reset_for_test();
}

/// Handles that create the key file at the same moment all end up with one file and one secret.
#[test]
fn concurrent_key_file_creation_settles_on_one_secret() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let path = temp_dir.path().join("darmok-secure-storage.key");
    let start = std::sync::Arc::new(std::sync::Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let path = path.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                super::create_key_file(&path).expect("create or read the winner's")
            })
        })
        .collect();
    let secrets: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().expect("thread"))
        .collect();

    let on_disk = std::fs::read(&path).expect("key file");
    for secret in &secrets {
        assert_eq!(secret.as_slice(), on_disk.as_slice());
    }
    assert_eq!(mode_of(&path), 0o600);
    assert_eq!(names_in(temp_dir.path()), ["darmok-secure-storage.key"]);
}

/// A keyring that answers but will not take one value: that value goes to the fallback sealed
/// with the per-install secret kept IN the keyring. No key file is made, and the blob cannot be
/// read without the keyring.
#[test]
fn with_a_keyring_the_fallback_key_comes_from_the_secret_in_the_keyring() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let data: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<String, String>>> =
        Default::default();
    let picky = || {
        SecureStorage::with_keyring(
            "darmok",
            Some(temp_dir.path().to_owned()),
            Box::new(PickyKeyring {
                data: data.clone(),
                refused: "key",
            }),
        )
    };
    let storage = picky();

    storage
        .write_value("key", "uzani, his army")
        .expect("write");

    let secret = data.borrow()[super::KEYRING_INSTALL_SECRET_KEY].clone();
    assert!(super::decode_install_secret(&secret).is_some());
    let blob = std::fs::read(storage.fallback_file("key").expect("path")).expect("blob");
    assert!(blob.starts_with(b"PSF1K"), "header: magic and keyring tag");
    assert_eq!(names_in(temp_dir.path()), ["darmok-key"], "no key file");
    assert_eq!(storage.read_value("key").expect("read"), "uzani, his army");
    assert_eq!(
        picky().read_value("key").expect("a later run"),
        "uzani, his army"
    );
    assert_eq!(
        data.borrow()[super::KEYRING_INSTALL_SECRET_KEY],
        secret,
        "the secret is created once"
    );

    // Without the keyring the blob is unreadable, reported as an error and left in place.
    super::availability::reset_for_test();
    let offline = storage_without_keyring(temp_dir.path());
    assert!(matches!(offline.read_value("key"), Err(Error::NotFound)));
    assert!(matches!(
        offline.read_fallback_value("key"),
        Err(Error::Unknown(_))
    ));
    assert_eq!(
        std::fs::read(offline.fallback_file("key").expect("path")).expect("blob"),
        blob
    );
    super::availability::reset_for_test();
}

/// A keyring secret that is not a secret (edited, truncated) is never overwritten: blobs sealed
/// with the real one may still exist. The write uses the key file instead.
#[test]
fn a_malformed_keyring_secret_is_not_replaced() {
    use super::super::SecureStorage as _;

    let _guard = super::availability::test_guard();
    super::availability::reset_for_test();
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let data = std::rc::Rc::new(std::cell::RefCell::new(std::collections::HashMap::from([
        (
            super::KEYRING_INSTALL_SECRET_KEY.to_owned(),
            "not hex".to_owned(),
        ),
    ])));
    let storage = SecureStorage::with_keyring(
        "darmok",
        Some(temp_dir.path().to_owned()),
        Box::new(PickyKeyring {
            data: data.clone(),
            refused: "key",
        }),
    );

    storage.write_value("key", "value").expect("write");

    assert_eq!(data.borrow()[super::KEYRING_INSTALL_SECRET_KEY], "not hex");
    let blob = std::fs::read(storage.fallback_file("key").expect("path")).expect("blob");
    assert!(blob.starts_with(b"PSF1F"));
    assert_eq!(storage.read_value("key").expect("read"), "value");
    super::availability::reset_for_test();
}
