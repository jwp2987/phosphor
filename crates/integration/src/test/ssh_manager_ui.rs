//! Integration tests for the SSH manager's group dropdown selection feature.
//!
//! Uses `TestStep::new` to create steps, without attaching terminal-related default assertions.

use std::sync::{Arc, Mutex};

use warp::integration_testing::ssh_manager::{
    assert_db_node_parent_id, assert_server_group_id, assert_server_view_visible,
    assert_ssh_manager_panel_open, create_folder_via_db, create_server_via_db,
    open_ssh_manager_panel, save_server, select_group_by_id, ssh_server_view,
};
use warp::workspace::Workspace;
use warpui::{
    async_assert, integration::TestStep,
    windowing::WindowManager, SingletonEntity,
};

use crate::Builder;

/// Shared test data IDs.
struct TestIds {
    folder_a: Arc<Mutex<Option<String>>>,
    folder_b: Arc<Mutex<Option<String>>>,
    server: Arc<Mutex<Option<String>>>,
}

impl TestIds {
    fn new() -> Self {
        Self {
            folder_a: Arc::new(Mutex::new(None)),
            folder_b: Arc::new(Mutex::new(None)),
            server: Arc::new(Mutex::new(None)),
        }
    }
}

/// Tests the server group dropdown selection feature:
/// creates two folders and one server, switches the group via the dropdown
/// selector and saves, then verifies the node moved correctly in the DB.
pub fn test_ssh_server_group_dropdown() -> Builder {
    let ids = TestIds::new();

    let mut builder = crate::test::new_builder();

    // Step 0: wait for the workspace view to be ready
    builder = builder.with_step(
        TestStep::new("Wait for workspace to be ready")
            .set_timeout(std::time::Duration::from_secs(30))
            .add_named_assertion(
                "workspace view exists",
                Box::new(|app: &mut warpui::App, _window_id| {
                    let window_id = app.read(|ctx| {
                        WindowManager::as_ref(ctx)
                            .active_window()
                            .expect("no active window")
                    });
                    let views = app.views_of_type::<Workspace>(window_id);
                    async_assert!(
                        views.is_some_and(|v: Vec<_>| !v.is_empty()),
                        "Expected workspace view to exist"
                    )
                }),
            ),
    );

    // Step 1: create test data (folder A, folder B, server in folder A)
    {
        let fa = ids.folder_a.clone();
        let fb = ids.folder_b.clone();
        let sid = ids.server.clone();
        builder = builder.with_step(
            TestStep::new("Create test folders and server via DB").with_action(move |_app, _, _| {
                let a_id = create_folder_via_db("GroupA");
                let b_id = create_folder_via_db("GroupB");
                let s_id = create_server_via_db("TestServer", Some(&a_id));
                *fa.lock().unwrap() = Some(a_id);
                *fb.lock().unwrap() = Some(b_id);
                *sid.lock().unwrap() = Some(s_id);
            }),
        );
    }

    // Step 2: open the SSH manager panel (with retries)
    builder = builder.with_step(
        open_ssh_manager_panel()
            .set_timeout(std::time::Duration::from_secs(30))
            .set_retries(3)
            .add_named_assertion(
                "SSH manager panel is open",
                assert_ssh_manager_panel_open(),
            ),
    );

    // Step 3: open the server editor
    {
        let sid = ids.server.clone();
        builder = builder.with_step(
            TestStep::new("Open server editor for test server").with_action(
                move |app, _, _| {
                    let node_id = sid.lock().unwrap().clone().expect("server id should exist");
                    let window_id = app.read(|ctx| {
                        WindowManager::as_ref(ctx)
                            .active_window()
                            .expect("no active window")
                    });
                    let workspace = app
                        .views_of_type::<Workspace>(window_id)
                        .and_then(|views| views.first().cloned())
                        .expect("no workspace view");
                    workspace.update(app, |ws, ctx| {
                        ws.open_ssh_server(node_id, ctx);
                    });
                },
            ),
        );
    }

    // Step 4: wait for the editor to be visible, assert the initial group is GroupA
    {
        let fa = ids.folder_a.clone();
        builder = builder.with_step(
            TestStep::new("Verify server editor visible and group is GroupA")
                .set_timeout(std::time::Duration::from_secs(15))
                .add_named_assertion("server view visible", assert_server_view_visible())
                .add_named_assertion(
                    "current_group_id equals GroupA",
                    Box::new(move |app: &mut warpui::App, window_id| {
                        let expected = fa.lock().unwrap().clone();
                        let view = ssh_server_view(app, window_id);
                        let actual = view.read(app, |v, _| v.current_group_id().clone());
                        async_assert!(
                            actual == expected,
                            "Expected current_group_id {:?}, but got {:?}",
                            expected,
                            actual
                        )
                    }),
                ),
        );
    }

    // Step 5: select GroupB by ID
    {
        let fb = ids.folder_b.clone();
        builder = builder.with_step(select_group_by_id(fb));
    }

    // Step 6: assert the group switched to GroupB
    {
        let fb = ids.folder_b.clone();
        builder = builder.with_step(
            TestStep::new("Verify group changed to GroupB")
                .set_timeout(std::time::Duration::from_secs(10))
                .add_named_assertion(
                    "current_group_id equals GroupB",
                    Box::new(move |app: &mut warpui::App, window_id| {
                        let expected = fb.lock().unwrap().clone();
                        let view = ssh_server_view(app, window_id);
                        let actual = view.read(app, |v, _| v.current_group_id().clone());
                        async_assert!(
                            actual == expected,
                            "Expected current_group_id {:?}, but got {:?}",
                            expected,
                            actual
                        )
                    }),
                ),
        );
    }

    // Step 7: select Root (None)
    {
        let none_id: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        builder = builder.with_step(select_group_by_id(none_id));
    }

    // Step 8: assert the group switched to Root
    builder = builder.with_step(
        TestStep::new("Verify group changed to Root")
            .set_timeout(std::time::Duration::from_secs(10))
            .add_named_assertion(
                "current_group_id is None",
                assert_server_group_id(None),
            ),
    );

    // Step 9: save
    builder = builder.with_step(save_server());

    // Step 10: assert the node has moved to Root in the DB
    {
        let sid = ids.server.clone();
        builder = builder.with_step(
            TestStep::new("Verify server moved to root in DB")
                .set_timeout(std::time::Duration::from_secs(10))
                .add_named_assertion(
                    "node parent_id is None in DB",
                    assert_db_node_parent_id(sid, None),
                ),
        );
    }

    builder
}
