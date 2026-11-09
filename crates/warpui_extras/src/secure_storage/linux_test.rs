use super::Error;
use super::SecureStorage;

#[test]
fn test_encrypt_decrypt_returns_same_value() {
    let storage = SecureStorage::new("darmok");

    let input = String::from("darmok and jalad at tanagra");
    let encrypted = storage.fallback_encrypt(&input).unwrap();
    let output = storage.fallback_decrypt(&encrypted).unwrap();

    assert_eq!(input, output)
}

#[test]
fn test_encrypt_decrypt_works_across_storage_instances() {
    let storage_1 = SecureStorage::new("darmok");
    let storage_2 = SecureStorage::new("jalad");

    let input = String::from("shaka when the walls fell");
    let encrypted = storage_1.fallback_encrypt(&input).unwrap();
    let output = storage_2.fallback_decrypt(&encrypted).unwrap();

    assert_eq!(input, output)
}

#[test]
fn test_decrypt_fails_on_malformed_data() {
    let storage = SecureStorage::new("darmok");

    let bad_datas: [&[u8]; 4] = [&[], &[0; 1], &[0; 11], &[0; 12]];

    for bad_data in bad_datas {
        let result = storage.fallback_decrypt(bad_data);

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
        storage.read_fallback_value("key").expect("read back"),
        "value"
    );
}

/// Migration for blobs that are never rewritten: reading a `0o644` fallback
/// file tightens it in place, without moving it or re-encrypting it.
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
        .fallback_encrypt("shaka when the walls fell")
        .expect("encrypt");
    std::fs::write(&path, &encrypted).expect("seed legacy blob");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
        .expect("seed legacy mode");

    // The credential must still be readable -- the migration must not orphan it.
    assert_eq!(
        storage.read_fallback_value("key").expect("read"),
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
    assert_eq!(storage.read_fallback_value("key").expect("read"), "value");
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
        super::probe_with_give_up(|| Err("no such name".to_owned()), Duration::from_secs(5)),
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
