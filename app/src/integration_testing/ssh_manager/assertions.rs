//! SSH manager integration test assertion helper functions.

use std::sync::{Arc, Mutex};

use warp_ssh_manager::SshRepository;
use warpui::{async_assert, integration::AssertionCallback, App, ViewHandle, WindowId};

use crate::integration_testing::view_getters::workspace_view;
use crate::ssh_manager::server_view::SshServerView;

/// Gets the unique SshServerView view handle in the window.
pub fn ssh_server_view(app: &App, window_id: WindowId) -> ViewHandle<SshServerView> {
    let mut views = app
        .views_of_type::<SshServerView>(window_id)
        .expect("should be views for window");
    assert_eq!(views.len(), 1, "expected exactly one SshServerView");
    views.remove(0)
}

/// Asserts that the SSH manager's left panel is open.
pub fn assert_ssh_manager_panel_open() -> AssertionCallback {
    Box::new(move |app, window_id| {
        let workspace = workspace_view(app, window_id);
        workspace.read(app, |workspace, ctx| {
            async_assert!(
                workspace.is_left_panel_open(ctx),
                "Expected left panel to be open, but it was closed"
            )
        })
    })
}

/// Asserts that the server editor view is visible.
pub fn assert_server_view_visible() -> AssertionCallback {
    Box::new(move |app, window_id| {
        let views = app.views_of_type::<SshServerView>(window_id);
        let count = views.map(|v| v.len()).unwrap_or(0);
        async_assert!(
            count > 0,
            "Expected SshServerView to be visible, but found {count}"
        )
    })
}

/// Asserts that the server editor's current current_group_id equals the expected value.
pub fn assert_server_group_id(expected: Option<String>) -> AssertionCallback {
    Box::new(move |app, window_id| {
        let view = ssh_server_view(app, window_id);
        let actual = view.read(app, |v, _| v.current_group_id().clone());
        async_assert!(
            actual == expected,
            "Expected current_group_id {:?}, but got {:?}",
            expected,
            actual
        )
    })
}

/// Asserts that the given node's parent_id in the DB equals the expected value (node_id is read from the Arc at runtime).
pub fn assert_db_node_parent_id(
    node_id: Arc<Mutex<Option<String>>>,
    expected: Option<String>,
) -> AssertionCallback {
    Box::new(move |_app, _window_id| {
        let nid = node_id
            .lock()
            .unwrap()
            .clone()
            .expect("node id should exist");
        let actual: Option<String> = warp_ssh_manager::with_conn(|c| {
            let nodes = SshRepository::list_nodes(c)?;
            Ok(nodes
                .into_iter()
                .find(|n| n.id == nid)
                .and_then(|n| n.parent_id))
        })
        .expect("db query");
        async_assert!(
            actual == expected,
            "Expected node parent_id {:?}, but got {:?}",
            expected,
            actual
        )
    })
}
