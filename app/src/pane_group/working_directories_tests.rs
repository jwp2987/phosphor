#![cfg(feature = "local_fs")]

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use repo_metadata::repositories::DetectedRepositories;
use warpui::{App, EntityId};

use crate::pane_group::WorkingDirectoriesModel;

#[test]
fn refresh_working_directories_collapses_subroots_to_nearest_repo_root() {
    App::test((), |mut app| async move {
        let detected_repos_handle = app.add_singleton_model(|_| DetectedRepositories::default());

        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let repo_root = temp_dir.path().join("repo");
        let repo_a = repo_root.join("a");
        let repo_b = repo_root.join("b");
        fs::create_dir_all(&repo_a).expect("create repo/a");
        fs::create_dir_all(&repo_b).expect("create repo/b");

        // Use dunce::canonicalize to match the behavior of warp_util::standardized_path::StandardizedPath and normalize_cwd,
        // which strip the Windows extended-length path prefix (\\?\) for consistent comparison.
        let canonical_repo_root = dunce::canonicalize(&repo_root).expect("canonical repo root");

        // Seed DetectedRepositories so get_root_for_path resolves to this repo.
        detected_repos_handle.update(&mut app, |repos, _ctx| {
            let canonical =
                warp_util::standardized_path::StandardizedPath::from_local_canonicalized(
                    canonical_repo_root.as_path(),
                )
                .expect("canonicalized path");
            repos.insert_test_repo_root(canonical);
        });

        let pane_group_id = EntityId::new();
        let terminal_1 = EntityId::new();
        let terminal_2 = EntityId::new();

        let working_directories_handle = app.add_model(|_| WorkingDirectoriesModel::new());
        let roots: Vec<PathBuf> = working_directories_handle.update(&mut app, |model, ctx| {
            model.refresh_working_directories_for_pane_group(
                pane_group_id,
                vec![
                    (terminal_1, repo_a.to_string_lossy().to_string()),
                    (terminal_2, repo_b.to_string_lossy().to_string()),
                ],
                vec![],
                Some(terminal_1),
                ctx,
            );

            model
                .most_recent_directories_for_pane_group(pane_group_id)
                .expect("pane group exists")
                .map(|dir| dir.path.to_local_path().expect("local").to_path_buf())
                .collect()
        });

        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0], canonical_repo_root);
    });
}

#[test]
fn refresh_working_directories_preserves_non_repo_paths_and_dedupes() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| DetectedRepositories::default());

        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let dir_1 = temp_dir.path().join("dir-1");
        let dir_2 = temp_dir.path().join("dir-2");
        fs::create_dir_all(&dir_1).expect("create dir-1");
        fs::create_dir_all(&dir_2).expect("create dir-2");

        // Use dunce::canonicalize to match the behavior of normalize_cwd,
        // which strips the Windows extended-length path prefix (\\?\) for consistent comparison.
        let canonical_1 = dunce::canonicalize(&dir_1).expect("canonical dir-1");
        let canonical_2 = dunce::canonicalize(&dir_2).expect("canonical dir-2");

        let pane_group_id = EntityId::new();
        let terminal_1 = EntityId::new();
        let terminal_2 = EntityId::new();
        let terminal_3 = EntityId::new();

        let working_directories_handle = app.add_model(|_| WorkingDirectoriesModel::new());
        let roots: HashSet<PathBuf> = working_directories_handle.update(&mut app, |model, ctx| {
            model.refresh_working_directories_for_pane_group(
                pane_group_id,
                vec![
                    (terminal_1, dir_1.to_string_lossy().to_string()),
                    (terminal_2, dir_2.to_string_lossy().to_string()),
                    // Duplicate root should be deduped.
                    (terminal_3, dir_1.to_string_lossy().to_string()),
                ],
                vec![],
                Some(terminal_1),
                ctx,
            );

            model
                .most_recent_directories_for_pane_group(pane_group_id)
                .expect("pane group exists")
                .map(|dir| dir.path.to_local_path().expect("local").to_path_buf())
                .collect()
        });

        assert_eq!(
            roots,
            HashSet::from_iter([canonical_1, canonical_2]),
            "should preserve non-repo roots and dedupe exact paths"
        );
    });
}

#[test]
fn register_remote_repo_persists_across_local_refresh() {
    use warp_util::host_id::HostId;
    use warp_util::local_or_remote_path::LocalOrRemotePath;
    use warp_util::remote_path::RemotePath;
    use warp_util::standardized_path::StandardizedPath;

    App::test((), |mut app| async move {
        app.add_singleton_model(|_| DetectedRepositories::default());

        let pane_group_id = EntityId::new();
        let remote_terminal = EntityId::new();
        let local_terminal = EntityId::new();

        let host = HostId::new("host-1".to_string());
        let remote_root = LocalOrRemotePath::Remote(RemotePath::new(
            host,
            StandardizedPath::from_local_absolute_unchecked(std::path::Path::new("/remote/proj")),
        ));

        let temp_dir = tempfile::TempDir::new().expect("temp dir");
        let local_cwd = temp_dir.path().join("localdir");
        fs::create_dir_all(&local_cwd).expect("create local dir");

        let working_directories_handle = app.add_model(|_| WorkingDirectoriesModel::new());

        // Register a remote repo, then run a local-only refresh; the remote
        // root must survive the refresh (it is not rederivable from local CWDs).
        let repos: Vec<LocalOrRemotePath> = working_directories_handle.update(&mut app, |model, ctx| {
            model.register_remote_repo(pane_group_id, remote_root.clone(), remote_terminal, ctx);

            model.refresh_working_directories_for_pane_group(
                pane_group_id,
                vec![(local_terminal, local_cwd.to_string_lossy().to_string())],
                vec![],
                Some(local_terminal),
                ctx,
            );

            model
                .most_recent_repositories_for_pane_group(pane_group_id)
                .expect("pane group exists")
                .collect()
        });

        assert!(
            repos.contains(&remote_root),
            "remote repo root should persist across a local-only refresh, got {repos:?}"
        );
        // The remote root's terminal mapping is also preserved.
        working_directories_handle.read(&app, |model, _| {
            assert_eq!(
                model.get_terminal_id_for_root_path(pane_group_id, &remote_root),
                Some(remote_terminal)
            );
        });
    });
}
