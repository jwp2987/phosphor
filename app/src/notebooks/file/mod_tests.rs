use std::{cell::RefCell, path::Path, rc::Rc, sync::Arc};

use pathfinder_geometry::vector::vec2f;

#[cfg(feature = "local_fs")]
use repo_metadata::RepoMetadataModel;
use repo_metadata::{repositories::DetectedRepositories, watcher::DirectoryWatcher};
use string_offset::CharOffset;
use warp_core::features::FeatureFlag;
use warp_core::ui::appearance::Appearance;
use warp_core::HostId;
use warp_editor::render::model::BlockItem;
#[cfg(feature = "local_fs")]
use warp_files::FileModel;
use warp_util::standardized_path::StandardizedPath;
use warpui::{platform::WindowStyle, App, SingletonEntity, TypedActionView, View};

use crate::code::buffer_location::{BufferLocation, RemotePath};
use crate::pane_group::PaneEvent;
use crate::terminal::keys::TerminalKeybindings;
use crate::{
    auth::{AuthManager, AuthStateProvider},
    cloud_object::model::persistence::ObjectStoreModel,
    notebooks::{
        editor::keys::NotebookKeybindings,
        file::{is_markdown_file, MarkdownDisplayMode},
    },
    search::files::model::FileSearchModel,
    settings_view::keybindings::KeybindingChangedNotifier,
    terminal::model::session::Session,
    test_util::settings::initialize_settings_for_tests,
    workspace::ActiveSession,
    workspaces::user_workspaces::UserWorkspaces,
    GlobalResourceHandles, GlobalResourceHandlesProvider,
};

use crate::notebooks::context_menu::MenuSource;

use super::{FileNotebookAction, FileNotebookEvent, FileNotebookView, FileState, SourceFile};

fn init_app(app: &mut App) {
    initialize_settings_for_tests(app);

    let global_resource_handles = GlobalResourceHandles::mock(app);
    app.add_singleton_model(|_| GlobalResourceHandlesProvider::new(global_resource_handles));
    app.add_singleton_model(|_| Appearance::mock());
    app.add_singleton_model(|_| ActiveSession::default());
    app.add_singleton_model(|_| KeybindingChangedNotifier::new());
    app.add_singleton_model(DirectoryWatcher::new);
    app.add_singleton_model(|_| DetectedRepositories::default());
    #[cfg(feature = "local_fs")]
    app.add_singleton_model(RepoMetadataModel::new);
    app.add_singleton_model(FileSearchModel::new);
    app.add_singleton_model(FileModel::new);
    app.add_singleton_model(NotebookKeybindings::new);
    app.add_singleton_model(TerminalKeybindings::new);
    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.add_singleton_model(AuthManager::new_for_test);
    app.add_singleton_model(|ctx| UserWorkspaces::mock(vec![], ctx));
    #[cfg(feature = "voice_input")]
    app.add_singleton_model(voice_input::VoiceInput::new);
}

#[test]
fn test_load_local() {
    App::test((), |mut app| async move {
        init_app(&mut app);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);
        let session = Arc::new(Session::test());
        handle
            .update(&mut app, |file_notebook, ctx| {
                file_notebook.open_local("../README.md", Some(session), ctx);

                let file_id = file_notebook
                    .file_id
                    .expect("File should be opened and have a file_id");

                let future_handle = FileModel::as_ref(ctx)
                    .get_future_handle(file_id)
                    .expect("Loading future should be present");

                ctx.await_spawned_future(future_handle.future_id())
            })
            .await;

        app.read(|ctx| {
            assert_eq!(&handle.as_ref(ctx).title(), "README.md");
            let location = handle
                .as_ref(ctx)
                .location
                .as_ref()
                .expect("Location should be set");
            assert_eq!(location.breadcrumbs, "..");

            let editor = handle.as_ref(ctx).editor.as_ref(ctx);
            assert!(!editor.is_editable(ctx));
            // We don't want to check the actual README contents, but it should be clearly non-empty.
            assert!(editor.markdown(ctx).len() > 4);

            // Rendering should not panic.
            handle.as_ref(ctx).render(ctx);
        });
    });
}

#[test]
fn test_load_jupyter_notebook_renders_cells() {
    App::test((), |mut app| async move {
        init_app(&mut app);
        let _flag = FeatureFlag::JupyterNotebookRendering.override_enabled(true);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("analysis.ipynb");
        std::fs::write(
            &path,
            r##"{
                "nbformat": 4,
                "nbformat_minor": 5,
                "metadata": {"language_info": {"name": "python"}},
                "cells": [
                    {"cell_type": "markdown", "source": ["# Notebook heading"]},
                    {"cell_type": "code", "source": "print('hello')", "outputs": []}
                ]
            }"##,
        )
        .unwrap();

        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);
        let session = Arc::new(Session::test());
        handle
            .update(&mut app, |file_notebook, ctx| {
                file_notebook.open_local(&path, Some(session), ctx);

                let file_id = file_notebook
                    .file_id
                    .expect("File should be opened and have a file_id");

                let future_handle = FileModel::as_ref(ctx)
                    .get_future_handle(file_id)
                    .expect("Loading future should be present");

                ctx.await_spawned_future(future_handle.future_id())
            })
            .await;

        app.read(|ctx| {
            let editor = handle.as_ref(ctx).editor.as_ref(ctx);
            let markdown = editor.markdown(ctx);
            // The notebook is rendered (heading from the markdown cell shows),
            // and the raw JSON is not (no `nbformat` key leaks through).
            assert!(
                markdown.contains("Notebook heading"),
                "expected rendered heading, got: {markdown}"
            );
            assert!(
                !markdown.contains("nbformat"),
                "raw notebook JSON should not be shown, got: {markdown}"
            );

            // The Rendered/Raw toggle is exposed for .ipynb, the same way it is
            // for markdown files (PRODUCT invariant 14).
            assert!(
                handle.as_ref(ctx).shows_markdown_toggle(),
                "rendered notebook should expose the Rendered/Raw toggle"
            );

            // Rendering should not panic.
            handle.as_ref(ctx).render(ctx);
        });
    });
}

#[test]
fn test_malformed_jupyter_notebook_falls_back_to_raw() {
    App::test((), |mut app| async move {
        init_app(&mut app);
        let _flag = FeatureFlag::JupyterNotebookRendering.override_enabled(true);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.ipynb");
        // Invalid notebook JSON that also contains Markdown which must NOT be
        // rendered as Markdown (PRODUCT invariant 11: fall back to raw text).
        std::fs::write(&path, "{ \"nbformat\": 4, broken json # Heading").unwrap();

        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);
        let session = Arc::new(Session::test());
        handle
            .update(&mut app, |file_notebook, ctx| {
                file_notebook.open_local(&path, Some(session), ctx);

                let file_id = file_notebook
                    .file_id
                    .expect("File should be opened and have a file_id");

                let future_handle = FileModel::as_ref(ctx)
                    .get_future_handle(file_id)
                    .expect("Loading future should be present");

                ctx.await_spawned_future(future_handle.future_id())
            })
            .await;

        app.read(|ctx| {
            let editor = handle.as_ref(ctx).editor.as_ref(ctx);
            let markdown = editor.markdown(ctx);
            // The raw contents are shown verbatim (never a blank view), fenced
            // as a code block rather than interpreted as Markdown.
            assert!(
                markdown.contains("broken json"),
                "expected raw contents shown, got: {markdown}"
            );
            assert!(
                markdown.contains("```"),
                "raw fallback should be fenced, got: {markdown}"
            );

            // Rendering should not panic.
            handle.as_ref(ctx).render(ctx);
        });
    });
}

#[test]
fn test_file_notebook_mermaid_blocks_default_to_rendered() {
    App::test((), |mut app| async move {
        init_app(&mut app);
        let _flag = FeatureFlag::MarkdownMermaid.override_enabled(true);
        let _editable_flag = FeatureFlag::EditableMarkdownMermaid.override_enabled(true);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);

        handle.update(&mut app, |file_notebook, ctx| {
            file_notebook.open_static("Test Title", "```mermaid\ngraph TD\nA --> B\n```", ctx);
        });
        let render_state = handle.read(&app, |view, ctx| {
            view.editor
                .as_ref(ctx)
                .model()
                .as_ref(ctx)
                .render_state()
                .clone()
        });
        app.read(|ctx| render_state.as_ref(ctx).layout_complete())
            .await;
        app.read(|ctx| render_state.as_ref(ctx).layout_complete())
            .await;

        handle.read(&app, |view, ctx| {
            let editor = view.editor.as_ref(ctx);
            let model = editor.model().as_ref(ctx);
            let command = model
                .notebook_command_for_block(CharOffset::zero())
                .expect("Mermaid command should exist");
            assert_eq!(
                command.as_ref(ctx).mermaid_display_mode,
                MarkdownDisplayMode::Rendered
            );
            assert!(matches!(
                model
                    .render_state()
                    .as_ref(ctx)
                    .content()
                    .block_at_height(0.)
                    .map(|item| item.item),
                Some(BlockItem::MermaidDiagram { .. })
            ));
        });
    });
}

#[test]
fn test_load_before_session() {
    // There might not be a session if:
    // * Restoring a file notebook, since terminal panes won't have bootstrapped yet
    // * Only notebooks are open
    App::test((), |mut app| async move {
        init_app(&mut app);
        let (window_id, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);

        // Open a file we know exists to verify that the view can render.
        handle
            .update(&mut app, |file_notebook, ctx| {
                file_notebook.open_local("../README.md", None, ctx);
                match &file_notebook.file_state {
                    FileState::Loading(source) => {
                        assert_eq!(source.local_path(), Some(Path::new("../README.md")))
                    }
                    other => panic!("Expected FileState::Loading, got {other:?}"),
                }

                let file_id = file_notebook
                    .file_id
                    .expect("File should be opened and have a file_id");

                let future_handle = FileModel::as_ref(ctx)
                    .get_future_handle(file_id)
                    .expect("Loading future should be present");

                ctx.await_spawned_future(future_handle.future_id())
            })
            .await;

        handle.read(&app, |view, _| {
            let expected_path = dunce::canonicalize("../README.md").expect("Path exists");

            assert_eq!(view.title(), expected_path.display().to_string());
            assert!(view.location.is_none());

            match &view.file_state {
                FileState::Loaded(source) => {
                    assert_eq!(source.local_path(), Some(expected_path.as_path()));
                }
                other => panic!("Expected FileState::Loaded, got {other:?}"),
            };
        });

        // Once a local session is available, the view should use it.
        let session = Arc::new(Session::test());
        ActiveSession::handle(&app).update(&mut app, |active_session, ctx| {
            active_session.set_session_for_test(window_id, session.clone(), Some("."), None, ctx);
        });

        handle.read(&app, |view, _| {
            assert_eq!(&view.title(), "README.md");
            // The location should be set, but the exact breadcrumbs depend on where the repo
            // is located.
            assert!(view.location.is_some());
        });
    });
}

#[test]
fn test_load_static() {
    App::test((), |mut app| async move {
        init_app(&mut app);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);

        handle.update(&mut app, |file_notebook, ctx| {
            file_notebook.open_static("Test Title", "Test Content", ctx);
            assert!(file_notebook.file_id.is_none());

            assert!(matches!(file_notebook.file_state, FileState::Loaded(_)));
            assert_eq!(file_notebook.title(), "Test Title");
            assert!(file_notebook.location.is_none());

            let editor = file_notebook.editor.as_ref(ctx);
            assert!(!editor.is_editable(ctx));
            // We don't want to check the actual README contents, but it should be clearly non-empty.
            assert!(editor.markdown(ctx).len() > 4);

            // Rendering should not panic.
            file_notebook.render(ctx);
        });
    });
}

/// APP-5243: retrying and then discarding a failed open must not panic, and each attempt must
/// release the file state it opened rather than stacking it on the shared [`FileModel`].
///
/// Fork drift from the pin: none in the assertions. The pin's copy opens a local `use
/// warpui::TypedActionView;` inside the test body; this module already imports that trait at the
/// top, so the inner import is dropped.
#[cfg(feature = "local_fs")]
#[test]
fn test_reload_and_discard_after_failed_open() {
    /// Opens the notebook's current file and waits for the read to settle.
    async fn await_open(
        app: &mut warpui::App,
        handle: &warpui::ViewHandle<FileNotebookView>,
        open: impl FnOnce(&mut FileNotebookView, &mut warpui::ViewContext<FileNotebookView>),
    ) -> warp_util::file::FileId {
        let (file_id, future) = handle.update(app, |file_notebook, ctx| {
            open(file_notebook, ctx);
            let file_id = file_notebook.file_id.expect("File should have a file_id");
            let future_handle = FileModel::as_ref(ctx)
                .get_future_handle(file_id)
                .expect("Loading future should be present");
            (file_id, ctx.await_spawned_future(future_handle.future_id()))
        });
        future.await;
        file_id
    }

    App::test((), |mut app| async move {
        init_app(&mut app);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);

        // A path that cannot be read, mirroring the `Could not read ...` treatment the reporter hit.
        let first_id = await_open(&mut app, &handle, |view, ctx| {
            view.open_local("app-5243-does-not-exist.md", None, ctx)
        })
        .await;

        handle.read(&app, |view, _| {
            assert!(
                matches!(view.file_state, FileState::Error(_)),
                "expected an error state, got {:?}",
                view.file_state
            );
        });

        // "Try again" in the error treatment.
        let second_id = await_open(&mut app, &handle, |view, ctx| {
            view.handle_action(&FileNotebookAction::ReloadFile, ctx)
        })
        .await;

        assert_ne!(first_id, second_id, "reload should open a fresh file id");
        app.read(|ctx| {
            assert!(
                FileModel::as_ref(ctx).file_path(first_id).is_none(),
                "reload should release the previous file id"
            );
        });
        handle.read(&app, |view, _| {
            assert!(
                matches!(view.file_state, FileState::Error(_)),
                "expected an error state after reload, got {:?}",
                view.file_state
            );
        });

        // Discarding the pane for good. Releasing is idempotent, so every teardown path can run it.
        handle.update(&mut app, |file_notebook, ctx| {
            file_notebook.release_file_model(ctx);
            file_notebook.release_file_model(ctx);
            assert!(file_notebook.file_id.is_none());
        });
        app.read(|ctx| {
            assert!(
                FileModel::as_ref(ctx).file_path(second_id).is_none(),
                "discarding the pane should release the open file id"
            );
        });
    });
}

#[test]
fn test_markdown_file_detection() {
    assert!(is_markdown_file("README.md"));
    assert!(is_markdown_file("DATABASE.MD"));
    assert!(is_markdown_file("notes.markdown"));
    assert!(is_markdown_file("README"));
    assert!(is_markdown_file("license"));
    assert!(is_markdown_file("CHANGELOG"));
    assert!(is_markdown_file("ReadMe"));

    assert!(!is_markdown_file("README.txt"));
    assert!(!is_markdown_file("main.rs"));
    assert!(!is_markdown_file("notes"));
}

#[test]
fn test_file_notebook_mermaid_context_menu_does_not_show_copy_image() {
    App::test((), |mut app| async move {
        init_app(&mut app);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);

        handle.update(&mut app, |file_notebook, ctx| {
            file_notebook.open_static("Test Title", "```mermaid\ngraph TD\nA --> B\n```", ctx);

            let source = MenuSource::RichTextEditor {
                parent_offset: vec2f(0., 0.),
                editor: file_notebook.editor.clone(),
            };
            file_notebook.context_menu.show_context_menu(source, ctx);

            let item_names = file_notebook.context_menu.item_names(ctx);
            assert!(!item_names.contains(&"Copy image"));
        });
    });
}

/// Subscribes to `handle`'s `FileNotebookEvent`s and returns the collector
/// they're pushed into, filtered to the `Pane` events -- the ones
/// `ToggleMarkdownDisplayMode(Raw)` / `OpenAsCode` emit to ask the pane group
/// to swap in a `CodePane`.
fn collect_pane_events(
    app: &mut App,
    handle: &warpui::ViewHandle<FileNotebookView>,
) -> Rc<RefCell<Vec<PaneEvent>>> {
    let pane_events = Rc::new(RefCell::new(Vec::new()));
    let collector = pane_events.clone();
    app.update(|ctx| {
        ctx.subscribe_to_view(handle, move |_, event: &FileNotebookEvent, _ctx| {
            if let FileNotebookEvent::Pane(pane_event) = event {
                collector.borrow_mut().push(pane_event.clone());
            }
        });
    });
    pane_events
}

#[test]
fn test_toggle_raw_mode_local_notebook_emits_replace_with_code_pane() {
    // Regression coverage for widening `PaneEvent::ReplaceWithCodePane`'s
    // `path` field from a plain local `PathBuf` to `BufferLocation`: local
    // notebooks must keep behaving exactly as before, emitting
    // `BufferLocation::Local(path)`.
    App::test((), |mut app| async move {
        init_app(&mut app);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);
        let session = Arc::new(Session::test());
        handle
            .update(&mut app, |file_notebook, ctx| {
                file_notebook.open_local("../README.md", Some(session), ctx);

                let file_id = file_notebook
                    .file_id
                    .expect("File should be opened and have a file_id");

                let future_handle = FileModel::as_ref(ctx)
                    .get_future_handle(file_id)
                    .expect("Loading future should be present");

                ctx.await_spawned_future(future_handle.future_id())
            })
            .await;

        let expected_path = dunce::canonicalize("../README.md").expect("Path exists");
        let pane_events = collect_pane_events(&mut app, &handle);

        handle.update(&mut app, |view, ctx| {
            view.handle_action(
                &FileNotebookAction::ToggleMarkdownDisplayMode(MarkdownDisplayMode::Raw),
                ctx,
            );
        });

        assert_eq!(
            pane_events.borrow().as_slice(),
            [PaneEvent::ReplaceWithCodePane {
                path: BufferLocation::Local(expected_path),
                source: None,
            }]
        );
    });
}

#[test]
fn test_toggle_raw_mode_remote_notebook_replaces_pane_with_remote_code_pane() {
    // Regression test for the gap this closes: Raw mode used to be a no-op
    // for remote notebooks because `open_as_code` /
    // `ToggleMarkdownDisplayMode(Raw)` gated on `local_path()`, which is
    // always `None` for `SourceFile::Remote`. `ReplaceWithCodePane`'s `path`
    // field now carries a `BufferLocation`, so Raw mode works for remote
    // notebooks too: it targets the same `RemotePath` the notebook was
    // opened with, via `CodeSource::RemoteFileTree` -- the same source the
    // existing remote file-tree code-editing flow already uses to fetch and
    // display remote file content.
    App::test((), |mut app| async move {
        init_app(&mut app);
        let (_, handle) = app.add_window(WindowStyle::NotStealFocus, FileNotebookView::new);

        let remote_path = RemotePath::new(
            HostId::new("test-host".to_string()),
            StandardizedPath::try_new("/home/user/notes/README.md").unwrap(),
        );

        // Simulate a remote notebook that has finished loading, bypassing the
        // `ReadFileContextRequest` RPC that `open_remote` would issue over a
        // live SSH connection -- this test only needs to exercise the
        // pane-replacement plumbing that Raw mode drives, not the network
        // fetch (which `RemoteServerManager`'s own tests cover).
        handle.update(&mut app, |view, _ctx| {
            view.file_state = FileState::Loaded(SourceFile::Remote {
                remote_path: remote_path.clone(),
            });
        });

        let pane_events = collect_pane_events(&mut app, &handle);

        handle.update(&mut app, |view, ctx| {
            view.handle_action(
                &FileNotebookAction::ToggleMarkdownDisplayMode(MarkdownDisplayMode::Raw),
                ctx,
            );
        });

        assert_eq!(
            pane_events.borrow().as_slice(),
            [PaneEvent::ReplaceWithCodePane {
                path: BufferLocation::Remote(remote_path),
                source: None,
            }],
            "Raw mode must replace the pane with a CodePane targeting the remote \
             file, not silently no-op"
        );
    });
}

/// #644: the pane header's "Open in editor" must never resolve a markdown file back to the
/// markdown viewer it was invoked from. `resolve_file_target` forwards
/// `prefer_markdown_viewer` (default `true`), which returned `FileTarget::MarkdownViewer`;
/// that routed to `open_file_notebook`, which focused the already-focused pane and returned,
/// so the menu item did nothing at all.
///
/// Non-vacuous: flip `open_in_editor_target`'s hard-coded `false` back to `true`, or restore
/// the `resolve_file_target` call it replaced, and the first assertion sees `MarkdownViewer`.
#[test]
#[cfg(feature = "local_fs")]
fn test_open_in_editor_never_resolves_back_to_the_markdown_viewer() {
    use crate::util::file::external_editor::settings::EditorChoice;
    use crate::util::openable_file_type::{EditorLayout, FileTarget};

    for choice in [
        EditorChoice::Zap,
        EditorChoice::SystemDefault,
        EditorChoice::EnvEditor,
    ] {
        let target = super::open_in_editor_target(
            Path::new("/tmp/notes.md"),
            choice,
            EditorLayout::SplitPane,
        );
        assert!(
            !matches!(target, FileTarget::MarkdownViewer(_)),
            "\"Open in editor\" resolved a markdown file back to the markdown viewer for \
             {choice:?}, which is the pane it was invoked from -- the menu item is a no-op"
        );
    }

    // Suppressing the markdown precedence must not override the user's editor choice:
    // Phosphor's own editor still resolves to the in-app code editor, and the default
    // still hands off to the system.
    assert!(matches!(
        super::open_in_editor_target(
            Path::new("/tmp/notes.md"),
            EditorChoice::Zap,
            EditorLayout::SplitPane,
        ),
        FileTarget::CodeEditor(_)
    ));
    assert!(matches!(
        super::open_in_editor_target(
            Path::new("/tmp/notes.md"),
            EditorChoice::SystemDefault,
            EditorLayout::SplitPane,
        ),
        FileTarget::SystemDefault
    ));
}
