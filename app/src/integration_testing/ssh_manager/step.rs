//! SSH manager integration test step helper functions.
//!
//! All steps are created with `TestStep::new` and don't attach the
//! terminal-related default assertions, since SSH manager tests don't depend
//! on the terminal view.

use std::sync::{Arc, Mutex};

use warp_ssh_manager::{SshRepository, SshServerInfo};
use warpui::SingletonEntity;
use warpui::TypedActionView;
use warpui::integration::TestStep;
use warpui::windowing::WindowManager;

use crate::ssh_manager::server_view::SshServerAction;
use crate::workspace::{Workspace, WorkspaceAction};

use super::assertions::ssh_server_view;

/// Opens the SSH manager's left-side panel.
pub fn open_ssh_manager_panel() -> TestStep {
    TestStep::new("Open SSH manager panel").with_action(move |app, _, _| {
        let window_id = app.read(|ctx| {
            WindowManager::as_ref(ctx)
                .active_window()
                .expect("no active window")
        });
        let workspace_view_id = app
            .views_of_type::<Workspace>(window_id)
            .and_then(|views| views.first().map(|view| view.id()))
            .expect("no workspace view");
        log::info!(
            "dispatching ToggleSshManager to workspace view {}",
            workspace_view_id
        );
        app.dispatch_typed_action(
            window_id,
            &[workspace_view_id],
            &WorkspaceAction::ToggleSshManager,
        );
    })
}

/// Creates a test folder via the DB, returning the folder node ID.
pub fn create_folder_via_db(name: &str) -> String {
    let name = name.to_string();
    warp_ssh_manager::with_conn(move |c| {
        let node = SshRepository::create_folder(c, None, &name)
            .unwrap_or_else(|e| panic!("create folder failed: {e:?}"));
        Ok(node.id)
    })
    .expect("create folder via db")
}

/// Creates a test server under the given folder via the DB, returning the node ID.
pub fn create_server_via_db(name: &str, parent_id: Option<&str>) -> String {
    let name = name.to_string();
    let parent = parent_id.map(String::from);
    warp_ssh_manager::with_conn(move |c| {
        let info = SshServerInfo {
            node_id: String::new(),
            host: format!("{name}.example.com"),
            port: 22,
            username: "root".into(),
            auth_type: warp_ssh_manager::AuthType::Password,
            key_path: None,
            credential_id: None,
            startup_command: None,
            notes: None,
            last_connected_at: None,
        };
        let node = SshRepository::create_server(c, parent.as_deref(), &name, &info)
            .unwrap_or_else(|e| panic!("create server failed: {e:?}"));
        Ok(node.id)
    })
    .expect("create server via db")
}

/// Selects the given group in the group dropdown selector.
/// Takes an `Arc<Mutex<Option<String>>>` so the folder ID can be read at
/// runtime, looks up the corresponding index by ID, and then dispatches SelectGroup.
pub fn select_group_by_id(folder_id: Arc<Mutex<Option<String>>>) -> TestStep {
    TestStep::new("Select group by folder id").with_action(move |app, _, _| {
        let window_id = app.read(|ctx| {
            WindowManager::as_ref(ctx)
                .active_window()
                .expect("no active window")
        });
        let view = ssh_server_view(app, window_id);
        let gid = folder_id.lock().unwrap().clone();
        view.update(app, |v, ctx| {
            let index = gid
                .as_ref()
                .and_then(|gid| v.folders().iter().position(|(id, _)| id == gid));
            v.handle_action(&SshServerAction::SelectGroup(index), ctx);
        });
    })
}

/// Saves the server editor's content.
pub fn save_server() -> TestStep {
    TestStep::new("Save server").with_action(move |app, _, _| {
        let window_id = app.read(|ctx| {
            WindowManager::as_ref(ctx)
                .active_window()
                .expect("no active window")
        });
        let view = ssh_server_view(app, window_id);
        view.update(app, |v, ctx| {
            v.handle_action(&SshServerAction::Save, ctx);
        });
    })
}
