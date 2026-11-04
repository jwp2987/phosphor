//! The symbol outline is built lazily: entering a repo (cwd change) must not start a
//! parse, and the first reader request for that repo must. The build itself needs a
//! detected `Repository`, so these pin the demand bookkeeping that gates it.

use std::path::{Path, PathBuf};

use repo_metadata::DirectoryWatcher;
use repo_metadata::repositories::DetectedRepositories;
use warpui::{App, SingletonEntity};

use super::RepoOutlines;
use crate::test_util::settings::initialize_settings_for_tests;

fn setup(app: &mut App) {
    initialize_settings_for_tests(app);
    app.add_singleton_model(DirectoryWatcher::new);
    app.add_singleton_model(|_| DetectedRepositories::default());
    app.add_singleton_model(RepoOutlines::new_for_test);
}

/// A freshly created model (what exists when the cwd enters a repo) has queued and
/// started nothing and has no demand for any repo. On the old code the
/// `DetectedGitRepo` subscription called `index_repo` unconditionally.
#[test]
fn no_build_state_without_a_request() {
    App::test((), |mut app| async move {
        setup(&mut app);
        app.read(|ctx| {
            let me = RepoOutlines::as_ref(ctx);
            assert!(me.outlines.is_empty());
            assert!(me.outline_queue.is_empty());
            assert!(me.active_outline_task.is_none());
            assert!(!me.is_requested(Path::new("/repo")));
        });
    })
}

/// The first reader request (the @ menu) records demand for the repo containing the
/// path, so a later `DetectedGitRepo` for that repo builds; other repos stay lazy.
#[test]
fn first_request_marks_only_its_repo_as_wanted() {
    App::test((), |mut app| async move {
        setup(&mut app);
        app.update(|ctx| {
            RepoOutlines::handle(ctx).update(ctx, |me, ctx| {
                me.handle_outline_request(PathBuf::from("/repo/src"), ctx);
            });
        });
        app.read(|ctx| {
            let me = RepoOutlines::as_ref(ctx);
            assert!(me.is_requested(Path::new("/repo")));
            assert!(!me.is_requested(Path::new("/other")));
        });
    })
}

/// #1276: a failed outline build is retried by a later request, but only after the backoff;
/// before, `Failed` was terminal.
#[test]
fn failed_outline_retries_only_after_the_backoff() {
    use std::time::Duration;

    use instant::Instant;

    use super::retry_backoff_elapsed;

    let backoff = Duration::from_secs(60);
    // Just failed: inside the window, so repeated tool calls do not rebuild.
    assert!(!retry_backoff_elapsed(Some(Instant::now()), backoff));
    // Failed long ago: the next request rebuilds.
    let long_ago = Instant::now().checked_sub(Duration::from_secs(120));
    assert!(retry_backoff_elapsed(long_ago, backoff));
    // Unknown failure time retries.
    assert!(retry_backoff_elapsed(None, backoff));
}

/// #1276: every status mutation changes the version that symbol caches key on.
#[test]
fn outline_mutations_bump_the_version() {
    App::test((), |mut app| async move {
        setup(&mut app);
        let v0 = app.read(|ctx| RepoOutlines::as_ref(ctx).version());
        app.update(|ctx| {
            RepoOutlines::handle(ctx).update(ctx, |me, _| me.bump_version());
        });
        let v1 = app.read(|ctx| RepoOutlines::as_ref(ctx).version());
        assert_ne!(v0, v1);
        // Unchanged model: the version is stable between reads.
        assert_eq!(v1, app.read(|ctx| RepoOutlines::as_ref(ctx).version()));
    })
}
