use crate::keyboard::{PersistedTrigger, UserDefinedKeybinding, REMOVED_KEYBINDING_SERIALIZATION};
use anyhow::{Ok, Result};
use vec1::vec1;

use warpui::keymap::Keystroke;

#[test]
fn test_short_user_defined_keybinding_to_persisted_trigger() {
    let keystroke = Keystroke::parse("ctrl-p").unwrap();
    let keybinding = UserDefinedKeybinding::Keystrokes(vec1![keystroke]);
    let persisted_trigger: PersistedTrigger = keybinding.into();

    assert_eq!(persisted_trigger, PersistedTrigger("ctrl-p".to_string()));
}

#[test]
fn test_long_user_defined_keybinding_to_persisted_trigger() {
    let keystroke = Keystroke::parse("ctrl-p").unwrap();
    let other_keystroke = Keystroke::parse("1").unwrap();

    let keybinding = UserDefinedKeybinding::Keystrokes(vec1![keystroke, other_keystroke]);
    let persisted_trigger: PersistedTrigger = keybinding.into();

    assert_eq!(persisted_trigger, PersistedTrigger("ctrl-p 1".to_string()));
}

#[test]
fn test_short_persisted_trigger_to_user_defined_keybinding() -> Result<()> {
    let persisted_trigger = PersistedTrigger("ctrl-x".to_string());
    let keybinding = UserDefinedKeybinding::try_from(persisted_trigger)?;

    let correct_keybinding =
        UserDefinedKeybinding::Keystrokes(vec1![Keystroke::parse("ctrl-x").unwrap()]);

    assert_eq!(keybinding, correct_keybinding);
    Ok(())
}

#[test]
fn test_long_persisted_trigger_to_user_defined_keybinding() -> Result<()> {
    let persisted_trigger = PersistedTrigger("ctrl-x 8".to_string());
    let keybinding = UserDefinedKeybinding::try_from(persisted_trigger)?;

    let correct_keybinding = UserDefinedKeybinding::Keystrokes(vec1![
        Keystroke::parse("ctrl-x").unwrap(),
        Keystroke::parse("8").unwrap()
    ]);

    assert_eq!(keybinding, correct_keybinding);
    Ok(())
}

#[test]
fn test_persisted_trigger_to_removed_user_keybinding() -> Result<()> {
    let persisted_trigger = PersistedTrigger(REMOVED_KEYBINDING_SERIALIZATION.to_string());
    let keybinding = UserDefinedKeybinding::try_from(persisted_trigger)?;

    assert_eq!(keybinding, UserDefinedKeybinding::Removed);
    Ok(())
}

#[test]
fn test_removed_user_keybinding_to_persisted_trigger() {
    let keybinding = UserDefinedKeybinding::Removed;
    let persisted_trigger: PersistedTrigger = keybinding.into();

    assert_eq!(
        persisted_trigger,
        PersistedTrigger(REMOVED_KEYBINDING_SERIALIZATION.to_string())
    );
}

#[test]
fn test_unparsable_persisted_trigger() {
    let persisted_trigger = PersistedTrigger("".to_string());
    let keybinding = UserDefinedKeybinding::try_from(persisted_trigger);

    assert!(keybinding.is_err());
}

mod keybindings_file {
    //! #858: a keybindings.yaml that fails to parse was treated as missing,
    //! and the next keybinding change rewrote it with only that change.

    use crate::keyboard::{
        CustomKeybindings, KeybindingsFile, KeybindingsWriteError, PersistedTrigger,
        current_keybindings_file_error, read_keybindings_file, update_keybindings_file,
    };

    // The broken file from the torture run (`logs/kb-garbage.yaml`): four
    // valid bindings and one broken line.
    const BROKEN: &str = r#"---
"workspace:new_tab": ctrl-shift-y
"editor_view:select_all": ctrl-alt-a
"terminal:clear": ctrl-alt-l
"workspace:close_tab": ctrl-alt-w
"editor:delete_all_left": [this is: broken
"#;

    fn insert(map: &mut CustomKeybindings, name: &str, trigger: &str) {
        map.0
            .insert(name.to_owned(), PersistedTrigger(trigger.to_owned()));
    }

    #[test]
    fn broken_file_is_invalid_not_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");
        std::fs::write(&path, BROKEN).unwrap();

        assert!(matches!(
            read_keybindings_file(&path),
            KeybindingsFile::Invalid(_)
        ));
    }

    #[test]
    fn update_refuses_to_overwrite_a_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");
        std::fs::write(&path, BROKEN).unwrap();

        let result = update_keybindings_file(&path, |map| {
            insert(map, "editor:move_to_line_start", "ctrl-a");
        });

        assert!(matches!(
            result,
            Err(KeybindingsWriteError::ExistingFileInvalid(_))
        ));
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            BROKEN,
            "the user's file must be left byte-for-byte intact"
        );
    }

    #[test]
    fn missing_file_is_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("keybindings.yaml");

        assert!(matches!(
            read_keybindings_file(&path),
            KeybindingsFile::Missing
        ));
        update_keybindings_file(&path, |map| insert(map, "terminal:clear", "ctrl-alt-l")).unwrap();

        let KeybindingsFile::Parsed(map) = read_keybindings_file(&path) else {
            panic!("the written file must parse");
        };
        assert_eq!(
            map.0.get("terminal:clear"),
            Some(&PersistedTrigger("ctrl-alt-l".to_owned()))
        );
    }

    #[test]
    fn valid_file_keeps_existing_bindings_on_update() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");
        std::fs::write(&path, "\"workspace:new_tab\": ctrl-shift-y\n").unwrap();

        update_keybindings_file(&path, |map| insert(map, "terminal:clear", "ctrl-alt-l")).unwrap();

        let KeybindingsFile::Parsed(map) = read_keybindings_file(&path) else {
            panic!("the written file must parse");
        };
        assert_eq!(map.0.len(), 2);
        assert_eq!(
            map.0.get("workspace:new_tab"),
            Some(&PersistedTrigger("ctrl-shift-y".to_owned()))
        );
    }

    #[test]
    fn empty_or_comment_only_file_means_no_customizations() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");
        for contents in [
            "",
            "\n",
            "---\n",
            "---\n...\n",
            "...\n",
            "# my bindings\n\n",
        ] {
            std::fs::write(&path, contents).unwrap();
            match read_keybindings_file(&path) {
                KeybindingsFile::Parsed(map) => assert!(map.0.is_empty()),
                _ => panic!("{contents:?} must read as an empty map, not as broken"),
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_file_is_invalid_not_missing() {
        // A directory where the file should be: open succeeds on some
        // platforms, read fails with something other than NotFound.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");
        std::fs::create_dir(&path).unwrap();

        assert!(matches!(
            read_keybindings_file(&path),
            KeybindingsFile::Invalid(_)
        ));
        assert!(matches!(
            update_keybindings_file(&path, |_| {}),
            Err(KeybindingsWriteError::ExistingFileInvalid(_))
        ));
    }

    #[test]
    fn leading_byte_order_mark_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");

        std::fs::write(&path, "\u{feff}\"terminal:clear\": ctrl-alt-l\n").unwrap();
        let KeybindingsFile::Parsed(map) = read_keybindings_file(&path) else {
            panic!("a BOM before valid YAML must not make it unparsable");
        };
        assert_eq!(
            map.0.get("terminal:clear"),
            Some(&PersistedTrigger("ctrl-alt-l".to_owned()))
        );

        for contents in ["\u{feff}", "\u{feff}# only a comment\n"] {
            std::fs::write(&path, contents).unwrap();
            match read_keybindings_file(&path) {
                KeybindingsFile::Parsed(map) => assert!(map.0.is_empty()),
                _ => panic!("{contents:?} must read as an empty map"),
            }
        }
    }

    /// A new window re-checks a recorded error, so a file fixed by hand stops
    /// producing the toast (there is no watcher on keybindings.yaml).
    #[test]
    fn recheck_reports_only_a_currently_broken_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("keybindings.yaml");

        std::fs::write(&path, BROKEN).unwrap();
        assert!(current_keybindings_file_error(&path).is_some());

        std::fs::write(&path, "\"terminal:clear\": ctrl-alt-l\n").unwrap();
        assert_eq!(current_keybindings_file_error(&path), None, "fixed by hand");

        std::fs::remove_file(&path).unwrap();
        assert_eq!(current_keybindings_file_error(&path), None, "deleted");
    }
}
