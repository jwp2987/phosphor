use async_channel::{unbounded, Receiver};
use warpui::{r#async::block_on, App, ModelHandle};

// lib_tests.rs
use super::*;

const WRITE_TEST_PATH: &str = "test_data/test_write/";

/// This enum is used so that we can pass the event through the async channel.
/// io::Error is not clonable, so we can't clone the FileModelEvent.
#[derive(Debug)]
enum TestFileModelEvent {
    FileLoaded {
        id: FileId,
        content: String,
        _version: ContentVersion,
    },
    FileSaved,
    FailedToLoad(String),
    /// Carries the error's user-facing text so tests can assert on the reason a
    /// save was refused, not merely that one was.
    FailedToSave(String),
}

impl From<&FileModelEvent> for TestFileModelEvent {
    fn from(event: &FileModelEvent) -> Self {
        match event {
            FileModelEvent::FileLoaded {
                id,
                content,
                version,
            } => TestFileModelEvent::FileLoaded {
                id: *id,
                content: content.clone(),
                _version: *version,
            },
            FileModelEvent::FileSaved { .. } => TestFileModelEvent::FileSaved,
            FileModelEvent::FailedToLoad {
                id: _id,
                error: err,
            } => TestFileModelEvent::FailedToLoad(format!("{err:?}")),
            FileModelEvent::FailedToSave { error, .. } => {
                TestFileModelEvent::FailedToSave(error.to_string())
            }
            FileModelEvent::FileUpdated { .. } => {
                // For now, we don't handle file updated events in tests
                // This could be extended to include a FileUpdated variant in TestFileModelEvent if needed
                TestFileModelEvent::FileLoaded {
                    id: event.file_id(),
                    content: String::new(),
                    _version: ContentVersion::new(),
                }
            }
        }
    }
}

/// Setup a Tokio channel that will forward any events from the FileModel to the receiver.
fn setup_event_channel(
    app: &mut App,
    files: &ModelHandle<FileModel>,
) -> Receiver<TestFileModelEvent> {
    let (sender, receiver) = unbounded();
    app.update(|ctx| {
        ctx.subscribe_to_model(files, move |_model, event, _ctx| {
            block_on(sender.send(TestFileModelEvent::from(event)))
                .expect("Could not send the result");
        });
    });
    receiver
}

#[test]
fn test_load() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        // Load the test file.
        files.update(app, |model, ctx| {
            model.open(Path::new("test_data/test_file.rs"), false, ctx);
        });

        // Check that the first event out is the file loaded event.
        let event = receiver.recv().await.expect("Could not receive the result");
        match event {
            TestFileModelEvent::FileLoaded { content, .. } => {
                assert_eq!(content.as_bytes(), TEST_FILE_CONTENT)
            }
            _ => panic!("Failed to load file"),
        }
    });
}

#[test]
fn test_save_uninitialized_file() {
    App::test((), |mut app| async move {
        let app = &mut app;

        let files = app.add_singleton_model(FileModel::new);
        let id = FileId::new();

        // This file has not been initialized with the model.  Make sure trying to save it fails immediately.
        files.update(app, |model, ctx| {
            let result = model.save(
                id,
                "This file doesn't exist".to_string(),
                ContentVersion::new(),
                ctx,
            );
            assert!(result.is_err());

            let e = result.unwrap_err();
            assert!(matches!(e, FileSaveError::NoFilePath(file_id) if file_id == id));
        });
    });
}

#[test]
fn test_save_file() {
    // Create the test write directory if it doesn't exist.
    std::fs::create_dir_all(WRITE_TEST_PATH).unwrap();

    // Write the test file content to a random file in the test write directory.
    let path = PathBuf::from(WRITE_TEST_PATH).join("test_save_file.rs");
    std::fs::write(&path, TEST_FILE_CONTENT).unwrap();

    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        // Open the newly created file.
        let path_clone = path.clone();
        files.update(app, |model, ctx| {
            model.open(&path_clone, false, ctx);
        });

        let file_id = match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FileLoaded { id, .. } => id,
            _ => panic!("Failed to load file"),
        };

        let old_version = files.read(app, |files, _ctx| files.version(file_id));
        let new_version = ContentVersion::new();

        // Save new content to the file.
        files.update(app, |model, ctx| {
            let result = model.save(file_id, "Overwrite content".to_string(), new_version, ctx);
            assert!(result.is_ok());
        });

        // Make sure that the file saved event was emitted.
        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FileSaved => (),
            _ => panic!("Failed to save file"),
        }

        // Make sure the content on disk matches the content we saved.
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "Overwrite content");

        // Make sure the version was updated.
        let model_version = files.read(app, |files, _ctx| files.version(file_id));
        assert_ne!(old_version, model_version);
        assert_eq!(Some(new_version), model_version);
    });
}

#[test]
fn test_load_missing_file() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        // Load a file that doesn't exist.
        files.update(app, |model, ctx| {
            model.open(Path::new("test_data/missing_file.rs"), false, ctx);
        });

        // Check that the first event out is the failed to load event.
        let event = receiver.recv().await.expect("Could not receive the result");
        match event {
            TestFileModelEvent::FailedToLoad(err) => {
                // File not found error strings differ across operating systems.
                #[cfg(not(windows))]
                let os_error_message = "No such file or directory";
                #[cfg(windows)]
                let os_error_message = "The system cannot find the file specified.";

                assert_eq!(
                    err,
                    format!(
                        "IOError(Os {{ code: 2, kind: NotFound, message: \"{os_error_message}\" }})"
                    )
                );
            }
            _ => panic!("Failed to load file"),
        }
    });
}

#[test]
fn test_save_missing_directory() {
    // Create the test write directory if it doesn't exist.
    let directory = PathBuf::from(WRITE_TEST_PATH).join("missing-directory");
    std::fs::create_dir_all(&directory).unwrap();

    // Write the test file content to a random file in the test write directory.
    let path = directory.join("test_save_missing_directory.rs");
    std::fs::write(&path, TEST_FILE_CONTENT).unwrap();

    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        // Save a file to a directory that doesn't exist.
        let file_id = files.update(app, |model, ctx| model.open(&path, false, ctx));

        // Check that the first event out is the successful load.
        let event = receiver.recv().await.expect("Could not receive the result");
        match event {
            TestFileModelEvent::FileLoaded { content, .. } => {
                assert_eq!(content.as_bytes(), TEST_FILE_CONTENT)
            }
            event => panic!("Failed to load file {event:?}"),
        }

        // Delete the directory that the file is in.
        std::fs::remove_dir_all(directory).unwrap();

        // Save new content to the file.
        files.update(app, |model, ctx| {
            let result = model.save(
                file_id,
                "Overwrite content".to_string(),
                ContentVersion::new(),
                ctx,
            );
            assert!(result.is_ok());
        });

        // Now we expect the save to succeed because ensure_parent_directories will create the missing directory
        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FileSaved => {
                // Make sure the content on disk matches the content we saved.
                let content = std::fs::read_to_string(&path).unwrap();
                assert_eq!(content, "Overwrite content");
            }
            event => panic!("Save should have succeeded but got event: {event:?}"),
        }
    });
}

/// A bare relative file name has an empty parent, which platform watchers resolve to the app's
/// own process directory. Watching (or worse, unwatching) that directory is never what the caller
/// asked for, so such files get no individual watcher at all.
#[test]
fn test_watch_path_ignores_empty_parents() {
    assert_eq!(FileModel::watch_path_for(Path::new("README.md")), None);
    assert_eq!(FileModel::watch_path_for(Path::new("")), None);
    assert_eq!(
        FileModel::watch_path_for(Path::new("docs/README.md")),
        Some(PathBuf::from("docs"))
    );

    let directory = std::env::temp_dir().join("app-5243");
    assert_eq!(
        FileModel::watch_path_for(&directory.join("README.md")),
        Some(directory)
    );
}

/// Waits for the read that `FileModel::open` spawned to settle, whichever way it resolves.
async fn await_load(receiver: &Receiver<TestFileModelEvent>) {
    match receiver.recv().await.expect("Could not receive the result") {
        TestFileModelEvent::FileLoaded { .. } | TestFileModelEvent::FailedToLoad(_) => (),
        event => panic!("Expected a load result, got {event:?}"),
    }
}

/// Registration and unregistration must use the exact same path, whichever entry point registered
/// the watcher. `register_file_path` used to watch the file itself while `unsubscribe` unwatched
/// it under a different derivation, so teardown could remove a watch that was never added and
/// leave the real one behind - the same asymmetry class as the crash this fixes.
#[test]
fn test_registration_and_unregistration_use_the_same_path() {
    App::test((), |mut app| async move {
        let app = &mut app;
        app.add_singleton_model(|_| DetectedRepositories::default());
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("watched.md");
        std::fs::write(&path, "# watched").expect("write file");

        // `register_file_path` registers up front...
        let registered_id =
            files.update(app, |model, ctx| model.register_file_path(&path, true, ctx));

        // ...and `open` registers once the read succeeds. Both must land on the same directory.
        let opened_id = files.update(app, |model, ctx| model.open(&path, true, ctx));
        await_load(&receiver).await;

        files.read(app, |model, _| {
            let stored_path = model.file_path(opened_id).expect("stored path");
            let watch_path = FileModel::watch_path_for(&stored_path).expect("watch path");
            assert_eq!(Some(watch_path.as_path()), stored_path.parent());
            assert_eq!(
                model.registered_watch_path(registered_id),
                Some(watch_path.as_path())
            );
            assert_eq!(
                model.registered_watch_path(opened_id),
                Some(watch_path.as_path())
            );
        });

        // Unsubscribing releases exactly what was registered, and nothing is left tracked.
        files.update(app, |model, ctx| {
            model.unsubscribe(registered_id, ctx);
            model.unsubscribe(opened_id, ctx);
        });
        files.read(app, |model, _| {
            assert_eq!(model.registered_watch_path(registered_id), None);
            assert_eq!(model.registered_watch_path(opened_id), None);
            assert_eq!(model.file_path(registered_id), None);
            assert_eq!(model.file_path(opened_id), None);
        });
    });
}

/// A file whose read fails never gets a watcher, so teardown must not try to remove one. This is
/// the exact shape of the crash: an unresolved relative path failed to load, and unsubscribing it
/// handed an empty directory to the platform watcher.
#[test]
fn test_a_failed_open_registers_no_watcher() {
    App::test((), |mut app| async move {
        let app = &mut app;
        app.add_singleton_model(|_| DetectedRepositories::default());
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let file_id = files.update(app, |model, ctx| {
            model.open(Path::new("app-5243-does-not-exist.md"), true, ctx)
        });
        await_load(&receiver).await;

        files.read(app, |model, _| {
            assert_eq!(model.registered_watch_path(file_id), None);
        });

        files.update(app, |model, ctx| model.unsubscribe(file_id, ctx));
        assert_eq!(files.read(app, |model, _| model.file_path(file_id)), None);
    });
}

static TEST_FILE_CONTENT: &[u8] = include_bytes!("../test_data/test_file.rs");

// ── Guarded writes (`save_if_unchanged`) ──────────────────────────
//
// These cover the accept path of an AI diff: content derived from a snapshot
// read earlier, written over a file that anything else may have touched since.
// The plain `save` above has no such guard by design, so nothing here should be
// read as a claim about it.

/// The text a diff was computed against.
const GUARD_BASE: &str = "fn main() {\n    println!(\"one\");\n}\n";
/// The text the user is accepting.
const GUARD_ACCEPTED: &str = "fn main() {\n    println!(\"two\");\n}\n";
/// What somebody else wrote to the file in the meantime.
const GUARD_EXTERNAL: &str = "fn main() {\n    println!(\"mine, actually\");\n}\n";

/// Registers `path` with `FileModel` exactly the way `InlineDiffView` does:
/// by path, without loading it and without subscribing to updates.
fn register_for_guarded_write(
    app: &mut App,
    files: &ModelHandle<FileModel>,
    path: &Path,
) -> FileId {
    files.update(app, |model, ctx| model.register_file_path(path, false, ctx))
}

/// Accepting an edit on a file nobody else touched still writes it. The guard
/// must not have turned the ordinary case into a refusal.
#[test]
fn guarded_write_saves_when_the_file_is_untouched() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("untouched.rs");
        std::fs::write(&path, GUARD_BASE).expect("write file");

        let file_id = register_for_guarded_write(app, &files, &path);
        let version = ContentVersion::new();
        files.update(app, |model, ctx| {
            model
                .save_if_unchanged(
                    file_id,
                    GUARD_ACCEPTED.to_owned(),
                    ExpectedDiskState::Content(GUARD_BASE.to_owned()),
                    version,
                    ctx,
                )
                .expect("save should dispatch");
        });

        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FileSaved => (),
            event => panic!("Expected the save to succeed, got {event:?}"),
        }

        assert_eq!(std::fs::read_to_string(&path).unwrap(), GUARD_ACCEPTED);
        assert_eq!(
            files.read(app, |model, _| model.version(file_id)),
            Some(version),
            "a successful guarded write records the version, like an unguarded one"
        );
    });
}

/// The defect this guard exists for: the user edits the file elsewhere while
/// the diff sits on screen, then accepts. Their edit must survive, and they must
/// be told the accept did not land and why.
#[test]
fn guarded_write_refuses_after_an_external_modification() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("clobbered.rs");
        std::fs::write(&path, GUARD_BASE).expect("write file");

        let file_id = register_for_guarded_write(app, &files, &path);

        // Somebody else edits the file after the diff was proposed.
        std::fs::write(&path, GUARD_EXTERNAL).expect("external write");

        let version = ContentVersion::new();
        files.update(app, |model, ctx| {
            model
                .save_if_unchanged(
                    file_id,
                    GUARD_ACCEPTED.to_owned(),
                    ExpectedDiskState::Content(GUARD_BASE.to_owned()),
                    version,
                    ctx,
                )
                .expect("save should dispatch");
        });

        // The user-visible signal fires: this is the event `InlineDiffView`
        // forwards as `InlineDiffViewEvent::FailedToSave`, which raises the
        // error toast and marks the diff as not saved.
        let message = match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FailedToSave(message) => message,
            event => panic!("Expected the save to be refused, got {event:?}"),
        };
        assert!(
            message.contains("changed on disk"),
            "the refusal must say why, got: {message}"
        );
        assert!(
            message.contains("not overwritten"),
            "the refusal must say the write did not happen, got: {message}"
        );

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            GUARD_EXTERNAL,
            "the external edit must still be on disk"
        );
        assert_eq!(
            files.read(app, |model, _| model.version(file_id)),
            None,
            "a refused write must not record the version it never wrote"
        );
    });
}

/// A file deleted out from under the diff is a change too, not an invitation to
/// recreate it from a stale snapshot.
#[test]
fn guarded_write_refuses_when_the_file_was_deleted() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("deleted.rs");
        std::fs::write(&path, GUARD_BASE).expect("write file");

        let file_id = register_for_guarded_write(app, &files, &path);
        std::fs::remove_file(&path).expect("external delete");

        files.update(app, |model, ctx| {
            model
                .save_if_unchanged(
                    file_id,
                    GUARD_ACCEPTED.to_owned(),
                    ExpectedDiskState::Content(GUARD_BASE.to_owned()),
                    ContentVersion::new(),
                    ctx,
                )
                .expect("save should dispatch");
        });

        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FailedToSave(message) => {
                assert!(
                    message.contains("was deleted"),
                    "the refusal must say why, got: {message}"
                );
            }
            event => panic!("Expected the save to be refused, got {event:?}"),
        }
        assert!(!path.exists(), "the file must not have been recreated");
    });
}

/// The diff base is stored LF-normalised whatever the file's real line endings
/// are, and the write emits the editor's own endings regardless. A CRLF file
/// that nobody touched must therefore still accept.
#[test]
fn guarded_write_ignores_line_ending_differences() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("crlf.rs");
        std::fs::write(&path, GUARD_BASE.replace('\n', "\r\n")).expect("write file");

        let file_id = register_for_guarded_write(app, &files, &path);
        files.update(app, |model, ctx| {
            model
                .save_if_unchanged(
                    file_id,
                    GUARD_ACCEPTED.to_owned(),
                    ExpectedDiskState::Content(GUARD_BASE.to_owned()),
                    ContentVersion::new(),
                    ctx,
                )
                .expect("save should dispatch");
        });

        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FileSaved => (),
            event => panic!("A CRLF file nobody touched must still accept, got {event:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), GUARD_ACCEPTED);
    });
}

/// Creating a file that is still absent works. `ExpectedDiskState::Absent` must
/// not have made file creation impossible.
#[test]
fn guarded_write_creates_a_file_that_is_still_absent() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("nested").join("created.rs");

        let file_id = register_for_guarded_write(app, &files, &path);
        files.update(app, |model, ctx| {
            model
                .save_if_unchanged(
                    file_id,
                    GUARD_ACCEPTED.to_owned(),
                    ExpectedDiskState::Absent,
                    ContentVersion::new(),
                    ctx,
                )
                .expect("save should dispatch");
        });

        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FileSaved => (),
            event => panic!("Expected the file to be created, got {event:?}"),
        }
        assert_eq!(std::fs::read_to_string(&path).unwrap(), GUARD_ACCEPTED);
    });
}

/// ...but a file that appeared in the meantime belongs to whoever created it.
#[test]
fn guarded_write_refuses_when_a_created_file_already_appeared() {
    App::test((), |mut app| async move {
        let app = &mut app;
        let files = app.add_singleton_model(FileModel::new);
        let receiver = setup_event_channel(app, &files);

        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("raced.rs");

        let file_id = register_for_guarded_write(app, &files, &path);
        std::fs::write(&path, GUARD_EXTERNAL).expect("external create");

        files.update(app, |model, ctx| {
            model
                .save_if_unchanged(
                    file_id,
                    GUARD_ACCEPTED.to_owned(),
                    ExpectedDiskState::Absent,
                    ContentVersion::new(),
                    ctx,
                )
                .expect("save should dispatch");
        });

        match receiver.recv().await.expect("Could not receive the result") {
            TestFileModelEvent::FailedToSave(message) => {
                assert!(
                    message.contains("created by something else"),
                    "the refusal must say why, got: {message}"
                );
            }
            event => panic!("Expected the save to be refused, got {event:?}"),
        }
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            GUARD_EXTERNAL,
            "the file somebody else created must be untouched"
        );
    });
}

/// A file we could not read is a file whose contents we cannot vouch for.
/// "Could not read it" must never collapse into "nothing there, safe to
/// overwrite" — for either expectation.
#[test]
fn an_unreadable_file_is_never_treated_as_safe_to_overwrite() {
    let path = Path::new("/does/not/matter.rs");
    let denied = || io::Error::new(io::ErrorKind::PermissionDenied, "permission denied");

    assert!(matches!(
        check_pre_image(
            path,
            &ExpectedDiskState::Content(GUARD_BASE.to_owned()),
            Err(denied()),
        ),
        PreWriteVerdict::Refuse(_)
    ));
    assert!(matches!(
        check_pre_image(path, &ExpectedDiskState::Absent, Err(denied())),
        PreWriteVerdict::Refuse(_),
    ));

    // Only an unambiguous "it is not there" clears an `Absent` expectation.
    assert_eq!(
        check_pre_image(
            path,
            &ExpectedDiskState::Absent,
            Err(io::Error::from(io::ErrorKind::NotFound)),
        ),
        PreWriteVerdict::Proceed
    );
}

/// The remote guard refuses on a mismatch and on a read it could not complete.
/// Its one documented weaker case — an `Absent` check whose read failed —
/// proceeds, because the wire read cannot tell "missing" from "failed"; pinning
/// that here so the exception cannot quietly widen.
#[test]
fn the_remote_guard_refuses_on_mismatch_and_on_unverifiable_content() {
    let expected = ExpectedDiskState::Content(GUARD_BASE.to_owned());

    assert_eq!(
        remote_pre_image_refusal::<String>("/tmp/a.rs", &expected, Ok(GUARD_BASE.into())),
        None
    );
    assert!(
        remote_pre_image_refusal::<String>("/tmp/a.rs", &expected, Ok(GUARD_EXTERNAL.into()))
            .is_some_and(|message| message.contains("changed on the remote host"))
    );
    assert!(
        remote_pre_image_refusal("/tmp/a.rs", &expected, Err("connection reset")).is_some(),
        "an unreadable remote file must not be overwritten"
    );
    assert!(
        remote_pre_image_refusal::<String>("/tmp/a.rs", &expected, Ok(vec![0xff, 0xfe])).is_some(),
        "a remote file we cannot decode must not be overwritten"
    );

    assert!(
        remote_pre_image_refusal::<String>("/tmp/a.rs", &ExpectedDiskState::Absent, Ok(Vec::new()))
            .is_none()
    );
    assert!(
        remote_pre_image_refusal::<String>(
            "/tmp/a.rs",
            &ExpectedDiskState::Absent,
            Ok(GUARD_EXTERNAL.into()),
        )
        .is_some()
    );
    assert!(
        remote_pre_image_refusal("/tmp/a.rs", &ExpectedDiskState::Absent, Err("no route")).is_none(),
        "documented exception: remote creation cannot be blocked by an unreadable probe"
    );
}

/// Line-ending normalisation must not swallow a real content change.
#[test]
fn normalisation_only_touches_line_endings() {
    assert_eq!(normalize_to_lf("a\r\nb\rc\nd"), "a\nb\nc\nd");
    assert_eq!(normalize_to_lf("no carriage returns"), "no carriage returns");
    assert_ne!(normalize_to_lf("a\r\nb"), normalize_to_lf("a\r\nB"));
}
