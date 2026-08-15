use ::local_control::{ActionKind, ErrorCode};

#[cfg(feature = "local_fs")]
use super::resolve_against_working_directory;
use super::validate_staged_input_text;

#[test]
fn staged_input_rejects_line_breaks_and_control_sequences() {
    assert!(validate_staged_input_text(ActionKind::InputInsert, "safe staged text").is_ok());

    for text in ["line\nbreak", "line\rbreak", "tab\tbreak", "\u{1b}[31m"] {
        let error = validate_staged_input_text(ActionKind::InputInsert, text).err();
        assert!(error.is_some_and(|error| error.code == ErrorCode::InvalidParams));
    }
}

#[cfg(feature = "local_fs")]
#[test]
fn file_open_resolves_relative_paths_against_the_session_working_directory() {
    use std::path::{Path, PathBuf};

    let temp_dir = tempfile::tempdir().expect("temp dir");
    let working_directory = dunce::canonicalize(temp_dir.path()).expect("canonical temp dir");
    let nested = working_directory.join("docs");
    std::fs::create_dir(&nested).expect("nested dir");
    std::fs::write(working_directory.join("README.md"), "# hi").expect("readme");
    std::fs::write(nested.join("guide.md"), "# guide").expect("guide");

    assert_eq!(
        resolve_against_working_directory(Path::new("README.md"), &working_directory),
        working_directory.join("README.md")
    );
    assert_eq!(
        resolve_against_working_directory(Path::new("./README.md"), &working_directory),
        working_directory.join("README.md")
    );
    assert_eq!(
        resolve_against_working_directory(Path::new("docs/guide.md"), &working_directory),
        nested.join("guide.md")
    );
    assert_eq!(
        resolve_against_working_directory(Path::new("../README.md"), &nested),
        working_directory.join("README.md")
    );

    // Paths that do not exist still resolve against the session directory, so a genuine
    // failure reports the session-relative file rather than a process-relative one.
    assert_eq!(
        resolve_against_working_directory(Path::new("missing.md"), &working_directory),
        working_directory.join("missing.md")
    );

    let absolute = PathBuf::from(if cfg!(windows) {
        r"C:\tmp\absolute.md"
    } else {
        "/tmp/absolute.md"
    });
    assert_eq!(
        resolve_against_working_directory(&absolute, &working_directory),
        absolute
    );
}

/// Adapted from the pinned oracle's `unavailable_surface_open_returns_structured_error`
/// (02b53fcd8). The pin exercises `ensure_surface_available` against the
/// feature-flag-gated Agent Management surface; this fork's equivalent surface
/// is unconditionally unavailable (see `metadata_tests.rs`), so this instead
/// exercises the same `ensure_surface_available` structured-error contract
/// through `handle`'s dispatch of `SurfaceAgentManagementOpen`, which is
/// unreachable through the normal is_implemented() gate and returns
/// `UnsupportedAction` directly.
#[test]
fn agent_management_open_action_is_rejected_as_unsupported() {
    warpui::App::test((), |mut app| async move {
        let bridge = app.add_singleton_model(crate::local_control::LocalControlBridge::new);
        let error = bridge
            .update(&mut app, |_bridge, ctx| {
                super::handle(
                    &None,
                    ActionKind::SurfaceAgentManagementOpen,
                    &serde_json::json!({}),
                    &::local_control::protocol::TargetSelector::default(),
                    ctx,
                )
            })
            .expect_err("agent management open is not implemented");
        assert_eq!(error.code, ErrorCode::UnsupportedAction);
        assert!(error.message.contains("surface.agent_management.open"));
    });
}
