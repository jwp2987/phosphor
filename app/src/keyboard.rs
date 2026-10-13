use itertools::Itertools;
use serde::{Deserialize, Serialize};
#[cfg(not(test))]
use std::env::var_os;
use vec1::{vec1, Vec1};
use warpui::keymap::Keystroke;
#[cfg(not(test))]
use warpui::keymap::Trigger;
use warpui::AppContext;

use anyhow::Context;

/// Environment variable to disable saving keybindings to file (used in integration tests)
pub const DISABLE_SAVE_ENV_VAR: &str = "WARP_TEST_DISABLE_KEYBINDING_SAVE";
const REMOVED_KEYBINDING_SERIALIZATION: &str = "none";

#[derive(PartialEq, Debug)]
/// A type to encapsulate the valid states of a keybinding
/// provided by a user in their keybindings.yaml file
pub enum UserDefinedKeybinding {
    /// Keybinding we can normalize/parse and will be recognized
    Keystrokes(Vec1<Keystroke>),
    /// User chose to remove the keybinding for an action
    Removed,
}

impl UserDefinedKeybinding {
    pub fn keystroke(value: Keystroke) -> Self {
        UserDefinedKeybinding::Keystrokes(vec1![value])
    }
}

// Ungated alongside `keybinding_file_path`, which is pure path computation and
// is rendered into the bundled `change-keybinding` skill in every build.
const KEYBINDINGS_FILE_NAME: &str = "keybindings.yaml";

/// Load all stored custom keybindings into the UI framework so that they are used
#[cfg(not(test))]
pub fn load_custom_keybindings(app: &mut AppContext) {
    let keybindings = match read_keybindings_file(&keybinding_file_path()) {
        KeybindingsFile::Missing => return,
        KeybindingsFile::Parsed(keybindings) => keybindings,
        KeybindingsFile::Invalid(reason) => {
            // Previously indistinguishable from "no file": nothing told the
            // user their bindings were not in effect, and the next keybinding
            // change rewrote the file without them (#858).
            report_invalid_keybindings_file(&reason, app);
            return;
        }
    };
    for (name, trigger) in keybindings.0 {
        let keybinding_type = UserDefinedKeybinding::try_from(trigger.clone());

        match keybinding_type {
            Ok(UserDefinedKeybinding::Removed) => {
                app.set_custom_trigger(name, Trigger::Empty);
            }
            Ok(UserDefinedKeybinding::Keystrokes(keystrokes)) => {
                app.set_custom_trigger(name, Trigger::Keystrokes(keystrokes.to_vec()));
            }
            Err(e) => {
                log::warn!(
                    "Tried to load an unparsable keybinding of {trigger:?} for action: {name}. error: {e}"
                );
            }
        }
    }
}

/// Write a new custom keybinding to disk
/// using the name of the editable binding and the new keystrokes
/// if keystrokes is UserDefinedKeybinding::Removed
/// we write a special value to disk to save that state
///
/// If the existing file cannot be read or parsed, nothing is written and the
/// error is reported: the change still applies for this session, but the
/// user's file is never replaced by one that has lost their bindings (#858).
#[cfg(not(test))]
pub fn write_custom_keybinding(
    name: String,
    keybinding: UserDefinedKeybinding,
    ctx: &mut AppContext,
) {
    // In tests, we don't want to write the actual keybindings file, since that could clobber the
    // user's current settings, so we no-op
    if var_os(DISABLE_SAVE_ENV_VAR).is_some() {
        return;
    }

    persist_keybindings_change(ctx, move |map| {
        map.0.insert(name, keybinding.into());
    });
}

/// Remove a custom keybinding from disk.
///
/// Like [`write_custom_keybinding`], refuses to touch a file it cannot parse.
#[cfg(not(test))]
pub fn remove_custom_keybinding<N>(name: N, ctx: &mut AppContext)
where
    N: AsRef<str>,
{
    // In tests, we don't want to write the actual keybindings file, since that could clobber the
    // users current settings, so we no-op
    if var_os(DISABLE_SAVE_ENV_VAR).is_some() {
        return;
    }

    let name = name.as_ref().to_owned();
    persist_keybindings_change(ctx, move |map| {
        map.0.remove(&name);
    });
}

#[cfg(not(test))]
fn persist_keybindings_change(ctx: &mut AppContext, change: impl FnOnce(&mut CustomKeybindings)) {
    cfg_if::cfg_if! {
        if #[cfg(feature = "local_fs")] {
            match update_keybindings_file(&keybinding_file_path(), change) {
                Ok(()) => {
                    // The file parsed, so it has been fixed since an earlier
                    // failure: windows opened from now on need no warning.
                    use warpui::SingletonEntity as _;
                    crate::user_config::WarpConfig::handle(ctx).update(ctx, |config, _| {
                        config.clear_keybindings_file_error();
                    });
                }
                Err(KeybindingsWriteError::ExistingFileInvalid(reason)) => {
                    report_invalid_keybindings_file(&reason, ctx);
                }
                Err(KeybindingsWriteError::Io(e)) => {
                    log::warn!("Unable to store custom keybindings: {e:#}");
                }
            }
        } else {
            let _ = (ctx, change);
            log::warn!("TODO(wasm): need to implement keybindings support");
        }
    }
}

/// Logs and surfaces (as a toast in every workspace) that `keybindings.yaml`
/// could not be used.
#[cfg(not(test))]
fn report_invalid_keybindings_file(reason: &str, ctx: &mut AppContext) {
    use warpui::SingletonEntity as _;

    log::warn!(
        "Custom keybindings in {} are not in effect and the file will not be modified until it is fixed: {reason}",
        keybinding_file_path().display()
    );
    crate::user_config::WarpConfig::handle(ctx).update(ctx, |config, ctx| {
        config.set_keybindings_file_error(reason.to_owned(), ctx);
    });
}

/// Ungated, matching the pinned oracle (`02b53fcd8`): only the *disk-touching*
/// helpers below are replaced with no-ops under `cfg(test)`. This one is pure
/// path computation, and the bundled `change-keybinding` skill renders it into
/// its content in every build, including test builds.
pub fn keybinding_file_path() -> std::path::PathBuf {
    warp_core::paths::config_local_dir().join(KEYBINDINGS_FILE_NAME)
}

/// The state of a keybindings file on disk.
// `Invalid`'s reason is read only by the non-test reporting path.
#[cfg_attr(test, allow(dead_code))]
enum KeybindingsFile {
    /// There is no file: nothing is customized yet.
    Missing,
    Parsed(CustomKeybindings),
    /// The file exists but could not be read or parsed. Holds a description
    /// of why. Must never be treated like [`Self::Missing`]: writing a fresh
    /// map over it would destroy every binding the user had (#858).
    Invalid(String),
}

#[derive(Debug)]
// The payloads are read only by the non-test, `local_fs` reporting path.
#[cfg_attr(any(test, not(feature = "local_fs")), allow(dead_code))]
enum KeybindingsWriteError {
    /// The existing file is unreadable or unparsable, so it was left alone.
    ExistingFileInvalid(String),
    Io(anyhow::Error),
}

/// Reads the keybindings file at `path`.
fn read_keybindings_file(path: &std::path::Path) -> KeybindingsFile {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return KeybindingsFile::Missing,
        Err(e) => return KeybindingsFile::Invalid(format!("could not be read: {e}")),
    };
    // An empty (or comment-only) file means "no customizations". serde_yaml
    // does not deserialize an empty document into a map, so without this an
    // empty file would be reported as broken (before #858 it was silently
    // treated as missing, which was right for this case only).
    if contents.lines().all(|line| {
        line.trim().is_empty() || line.trim_start().starts_with('#') || line.trim() == "---"
    }) {
        return KeybindingsFile::Parsed(CustomKeybindings::default());
    }
    match serde_yaml::from_str(&contents) {
        Ok(map) => KeybindingsFile::Parsed(map),
        Err(e) => KeybindingsFile::Invalid(format!("could not be parsed: {e}")),
    }
}

/// Applies `change` to the keybindings file at `path` and writes it back.
///
/// Refuses (without touching the file) when the existing file is unreadable
/// or unparsable.
#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
fn update_keybindings_file(
    path: &std::path::Path,
    change: impl FnOnce(&mut CustomKeybindings),
) -> Result<(), KeybindingsWriteError> {
    let mut map = match read_keybindings_file(path) {
        KeybindingsFile::Missing => CustomKeybindings::default(),
        KeybindingsFile::Parsed(map) => map,
        KeybindingsFile::Invalid(reason) => {
            return Err(KeybindingsWriteError::ExistingFileInvalid(reason));
        }
    };
    change(&mut map);
    let serialized = serde_yaml::to_string(&map)
        .context("Unable to serialize custom keybindings")
        .map_err(KeybindingsWriteError::Io)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Unable to create {}", parent.display()))
            .map_err(KeybindingsWriteError::Io)?;
    }
    std::fs::write(path, serialized)
        .with_context(|| format!("Unable to write {}", path.display()))
        .map_err(KeybindingsWriteError::Io)
}

// For tests, we don't want to read or write from the filesystem.
//
// Unit tests are run with #[cfg(test)] enabled, so we can define custom no-op implementations
#[cfg(test)]
pub fn load_custom_keybindings(_: &mut AppContext) {}
#[cfg(test)]
pub fn write_custom_keybinding(_: String, _: UserDefinedKeybinding, _: &mut AppContext) {}
#[cfg(test)]
pub fn remove_custom_keybinding<N>(_: N, _: &mut AppContext)
where
    N: AsRef<str>,
{
}

/// Struct that represents the full custom keybindings file for (de-)serialization
///
/// The file format is a top-level YAML map of (Editable Binding Name) -> Keybinding
/// Since many of the editable bindings have a `:` character in their name, the name will need to
/// be quoted in most cases.
/// The format of the keybinding is the normalized version that we use internally, with multiple
/// keystrokes separated by whitespace, if necessary.
///
/// For example:
/// ---
/// "editor:delete_all_left": cmd-shift-A
/// "editor:delete_all_right": cmd-shift-D escape
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
struct CustomKeybindings(std::collections::HashMap<String, PersistedTrigger>);

/// The normalized version of a keystroke or series of keystrokes that is written into the
/// keybindings file. If there are multiple keystrokes, each is separated by a space
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
struct PersistedTrigger(String);

impl From<UserDefinedKeybinding> for PersistedTrigger {
    fn from(keybinding: UserDefinedKeybinding) -> Self {
        match keybinding {
            UserDefinedKeybinding::Keystrokes(keystrokes) => {
                PersistedTrigger(keystrokes.iter().map(Keystroke::normalized).join(" "))
            }
            UserDefinedKeybinding::Removed => {
                PersistedTrigger(REMOVED_KEYBINDING_SERIALIZATION.to_string())
            }
        }
    }
}

impl TryFrom<PersistedTrigger> for UserDefinedKeybinding {
    type Error = anyhow::Error;
    fn try_from(trigger: PersistedTrigger) -> anyhow::Result<Self> {
        if trigger.0 == REMOVED_KEYBINDING_SERIALIZATION {
            return Ok(UserDefinedKeybinding::Removed);
        }

        let mut keystrokes: Vec<Keystroke> = Vec::new();

        for keystroke in trigger.0.split_whitespace() {
            let parsed_keystroke: Keystroke = Keystroke::parse(keystroke).context(format!(
                "Failed to parse keystroke \"{}\" in trigger \"{}\"",
                keystroke, trigger.0,
            ))?;
            keystrokes.push(parsed_keystroke);
        }

        let parsed_keystrokes: Vec1<Keystroke> = Vec1::try_from(keystrokes).context(format!(
            "No valid keystrokes were found in trigger: {}",
            trigger.0
        ))?;

        Ok(UserDefinedKeybinding::Keystrokes(parsed_keystrokes))
    }
}

#[cfg(test)]
#[path = "keyboard_test.rs"]
mod tests;
