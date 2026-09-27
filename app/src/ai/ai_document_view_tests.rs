#[cfg(feature = "local_fs")]
mod link_targets {
    use super::super::document_link_target;
    use crate::util::file::external_editor::EditorSettings;
    use crate::util::file::external_editor::settings::{
        EditorChoice, OpenCodePanelsFileEditor, OpenConversationLayoutPreference, OpenFileEditor,
        OpenFileLayout, PreferMarkdownViewer, PreferTabbedEditorView,
    };
    use crate::util::openable_file_type::FileTarget;
    use settings::Setting as _;
    use std::path::Path;

    fn settings(editor_choice: EditorChoice) -> EditorSettings {
        EditorSettings {
            open_file_editor: OpenFileEditor::new(Some(editor_choice)),
            open_code_panels_file_editor: OpenCodePanelsFileEditor::new(Some(editor_choice)),
            open_file_layout: OpenFileLayout::new(None),
            prefer_markdown_viewer: PreferMarkdownViewer::new(Some(false)),
            prefer_tabbed_editor_view: PreferTabbedEditorView::new(None),
            open_conversation_layout_preference: OpenConversationLayoutPreference::new(None),
        }
    }

    /// #681: a model-written document linking an app bundle, installer or executable reveals it
    /// instead of launching it, whatever the user's editor choice.
    #[test]
    fn launchable_link_is_revealed() {
        for choice in [EditorChoice::Zap, EditorChoice::SystemDefault] {
            let settings = settings(choice);
            for path in [
                "/tmp/Evil.app",
                "/tmp/setup.pkg",
                "/tmp/setup.exe",
                "/tmp/report.xlsm",
                "/tmp/run.command",
            ] {
                let target = document_link_target(Path::new(path), &settings);
                assert!(
                    !target.is_system_handler(),
                    "{path} under {choice:?} resolved to {target:?}"
                );
            }
            assert_eq!(
                document_link_target(Path::new("/tmp/Evil.app"), &settings),
                FileTarget::RevealInFileManager
            );
        }
    }

    /// Raster images keep the system viewer, and ordinary binaries keep `SystemGeneric`.
    #[test]
    fn ordinary_links_are_unchanged() {
        let settings = settings(EditorChoice::Zap);
        assert_eq!(
            document_link_target(Path::new("/tmp/photo.png"), &settings),
            FileTarget::SystemGeneric
        );
        assert_eq!(
            document_link_target(Path::new("/tmp/paper.pdf"), &settings),
            FileTarget::SystemGeneric
        );
    }

    /// The raster shortcut is guarded: a `.png` that is really an executable is revealed.
    #[test]
    #[cfg(unix)]
    fn executable_disguised_as_raster_image_is_revealed() {
        use std::os::unix::fs::PermissionsExt;

        let dir = tempfile::tempdir().unwrap();
        let disguised = dir.path().join("photo.png");
        std::fs::write(&disguised, b"#!/bin/sh\necho pwned\n").unwrap();
        std::fs::set_permissions(&disguised, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert_eq!(
            document_link_target(&disguised, &settings(EditorChoice::Zap)),
            FileTarget::RevealInFileManager
        );
    }
}
