//! SFTP integration test helper functions
//!
//! Provides helpers for getting the SFTP browser view, creating a mock
//! backend, and opening/injecting the panel.
//! author: logic
//! date: 2026-05-30

use std::path::PathBuf;
use std::sync::Arc;

use warpui::{App, ViewHandle, WindowId};

use crate::sftp_manager::browser::SftpBrowserView;
use crate::sftp_manager::sftp_backend::{InMemorySftpBackend, SftpBackend};

// Re-exported for integration tests to use via warp::integration_testing::sftp
pub use crate::sftp_manager::browser::SftpBrowserAction;
pub use crate::sftp_manager::types::{ConnectionState, Dialog};

/// Gets the SFTP browser view handle
///
/// Looks up the SftpBrowserView instance in the given window.
/// author: logic
/// date: 2026-05-30
pub fn sftp_browser_view(app: &App, window_id: WindowId) -> ViewHandle<SftpBrowserView> {
    let views: Vec<ViewHandle<SftpBrowserView>> = app
        .views_of_type(window_id)
        .expect("should have views for window");
    views
        .into_iter()
        .next()
        .expect("should have at least one SFTP browser view")
}

/// Creates a temp directory with a preset file structure and a mock backend
///
/// `files` is a list of (relative path, content); the required parent directories are created automatically.
/// author: logic
/// date: 2026-05-30
pub fn create_mock_backend(
    files: &[(&str, &[u8])],
) -> (tempfile::TempDir, Arc<dyn SftpBackend>) {
    let temp_dir = tempfile::tempdir().expect("failed to create temp directory");
    for (path, content) in files {
        let full_path = temp_dir.path().join(path);
        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).expect("failed to create subdirectory");
        }
        std::fs::write(&full_path, content).expect("failed to write test file");
    }
    let backend = Arc::new(InMemorySftpBackend::new(temp_dir.path().to_path_buf()))
        as Arc<dyn SftpBackend>;
    (temp_dir, backend)
}

/// Opens the SFTP panel and injects a mock backend
///
/// Returns (window_id, temp_dir); temp_dir must be kept alive for the duration of the test.
/// author: logic
/// date: 2026-05-30
pub fn open_sftp_pane_with_mock(
    app: &mut App,
    files: &[(&str, &[u8])],
) -> (WindowId, tempfile::TempDir) {
    let window_id = app.read(|ctx| {
        ctx.windows()
            .active_window()
            .expect("should have an active window")
    });

    let workspace = super::view_getters::workspace_view(app, window_id);
    app.update(|ctx| {
        workspace.update(ctx, |ws, ctx| {
            ws.open_sftp_pane("__mock_sftp_test__".to_string(), ctx);
        });
    });

    let (temp_dir, backend) = create_mock_backend(files);
    let view = sftp_browser_view(app, window_id);
    view.update(app, |v, ctx| {
        v.inject_mock_backend(backend, PathBuf::from("/"), ctx);
    });

    (window_id, temp_dir)
}
