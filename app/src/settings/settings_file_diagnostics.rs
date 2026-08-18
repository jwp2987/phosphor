//! Diagnostic for keys in `settings.toml` that correspond to no setting.
//!
//! WHY THIS EXISTS
//! ---------------
//! The settings loader is pull-based: `SettingsManager` walks the settings it
//! knows about and asks the preferences backend for each one's value. Nothing
//! ever walks the other way, so a key in the file that matches no setting is
//! not an error, not a warning, and not a no-op the user can observe — it is
//! simply never looked at. A user who typos `terminal.blinking_curser`, or who
//! carries a `settings.toml` over from Warp containing a setting this fork has
//! removed, gets an editor that says nothing and an application that behaves as
//! if the line were not there. Confirmed absent both here and at the pin
//! (`42effe840`), so this is a fork addition, not a port.
//!
//! WHY IT WARNS AND DOES NOT FAIL
//! ------------------------------
//! Dead keys are the *expected* state for anyone migrating a Warp
//! `settings.toml`: this fork has deliberately removed a large number of
//! upstream settings (see `DECLINED.md`), and every one of them is a key some
//! existing user's file may still carry. Treating that as a hard error would
//! lock a migrating user out of their own configuration over lines that are
//! merely obsolete. So: report, never reject, and never rewrite the file —
//! the user's comments and their not-yet-migrated keys are theirs to keep.
//!
//! KNOWN FALSE POSITIVES
//! ---------------------
//! Settings groups behind `#[cfg]` (e.g. `LinuxAppConfiguration`) are not
//! compiled into other platforms' builds, so their keys are genuinely unknown
//! to a macOS or Windows binary and will be reported there. This is why the
//! message says "no setting in this build" rather than "no such setting", and
//! another reason the diagnostic must not be an error: a single
//! `settings.toml` shared across machines is a legitimate thing to have.

use std::collections::BTreeSet;
use std::sync::Mutex;

use warpui::{AppContext, SingletonEntity};

use super::user_preferences_toml_file_path;

/// The unknown-key set as of the last time it was reported.
///
/// The diagnostic runs on every settings-file hot-reload, and the file is
/// rewritten by the app itself whenever any setting changes — so without this,
/// toggling a checkbox in the settings UI would re-emit the same warning about
/// the same dead keys, forever. Only a *change* in the set is news.
///
/// `None` means "never run"; `Some(vec![])` means "ran, and the file was
/// clean", which is why this is not simply an empty set.
static LAST_REPORTED: Mutex<Option<Vec<String>>> = Mutex::new(None);

/// Logs a warning naming every key in `settings.toml` that corresponds to no
/// registered setting, unless the same set was already reported, and returns
/// that set.
///
/// The return value is what the in-app surface renders, and is deliberately
/// *not* subject to the once-only suppression the log is: the warning is a
/// stream, so repeating it is noise, but a banner is a state, so it has to
/// keep being true for as long as the keys are still in the file.
///
/// Callers must use the returned set rather than
/// [`last_unknown_settings_file_keys`], which retains the previous run's
/// answer across the early returns below and would therefore report keys from
/// a file this call never looked at.
///
/// Safe to call on every load and every hot-reload; that is the intended use.
/// Returns an empty set when the settings file is not the active backend (the
/// feature flag is off, or this is a test using the in-memory store).
pub fn report_unknown_settings_file_keys(ctx: &AppContext) -> Vec<String> {
    if !<settings::PublicPreferences as SingletonEntity>::as_ref(ctx).is_settings_file() {
        return Vec::new();
    }

    let path = user_preferences_toml_file_path();
    let Ok(contents) = std::fs::read_to_string(&path) else {
        // No file yet (the common case before the first write), or unreadable.
        // Neither is this diagnostic's business to report: an unreadable file
        // is already surfaced by the backend's own load error.
        return Vec::new();
    };

    let known: BTreeSet<String> = settings::SettingsManager::as_ref(ctx)
        .public_settings_file_paths()
        .collect();
    let unknown = unknown_settings_file_keys(&contents, &known);

    // Take the lock, compare, and record — all before logging, so two loads
    // racing cannot both decide they are the one to report.
    let already_reported = {
        let mut last = match LAST_REPORTED.lock() {
            Ok(guard) => guard,
            // A poisoned lock only means some other caller panicked while
            // holding it. The contents are still a valid "last reported" set,
            // and losing this diagnostic is not worth propagating a panic.
            Err(poisoned) => poisoned.into_inner(),
        };
        let seen = last.as_deref() == Some(unknown.as_slice());
        if !seen {
            *last = Some(unknown.clone());
        }
        seen
    };

    if already_reported || unknown.is_empty() {
        return unknown;
    }

    log::warn!(
        "{} key(s) in {} match no setting in this build and are being ignored: {}. \
         A typo, or a setting this fork has removed — either way the line has no \
         effect. Settings for a platform this build does not target also appear here.",
        unknown.len(),
        path.display(),
        unknown.join(", "),
    );

    unknown
}

/// Returns the last unknown-key set computed by
/// [`report_unknown_settings_file_keys`], newest first call onwards.
///
/// A read-only view of the suppression state, for callers that want the last
/// answer without triggering a fresh read of the file. The in-app surface
/// (`super::SettingsFileError::UnknownKeys`) does *not* use this -- it takes
/// the set straight off `report_unknown_settings_file_keys`, because this
/// function cannot distinguish "the file is clean" from "the last call bailed
/// before reading the file".
pub fn last_unknown_settings_file_keys() -> Vec<String> {
    let guard = match LAST_REPORTED.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    (*guard).clone().unwrap_or_default()
}

/// Returns the dotted paths present in `file_contents` that are not in
/// `known_paths`, in a stable order.
///
/// `known_paths` holds full `toml_path`s, e.g. `terminal.input.autosuggestions.enabled`.
/// The walk descends only into tables that are a proper prefix of some known
/// path, and stops the moment it reaches a known path itself: a setting whose
/// value is a table (`max_table_depth`) owns everything underneath it, and the
/// shape of that value is that setting's own deserializer's business, not this
/// function's. Reporting its inner keys would turn every structured setting
/// into a wall of false positives.
///
/// A file that does not parse yields no findings — that case is already
/// reported as [`super::SettingsFileError::FileParseFailed`], and guessing at
/// keys in a file we could not read would only double up on the same error.
pub fn unknown_settings_file_keys(
    file_contents: &str,
    known_paths: &BTreeSet<String>,
) -> Vec<String> {
    let Ok(root) = toml::from_str::<toml::Table>(file_contents) else {
        return Vec::new();
    };

    // Every proper ancestor of a known path is a legitimate section heading:
    // `terminal.input.autosuggestions.enabled` makes `terminal`,
    // `terminal.input` and `terminal.input.autosuggestions` all real.
    let mut known_sections: BTreeSet<&str> = BTreeSet::new();
    for path in known_paths {
        for (idx, ch) in path.char_indices() {
            if ch == '.' {
                known_sections.insert(&path[..idx]);
            }
        }
    }

    let mut unknown = Vec::new();
    collect_unknown(&root, "", known_paths, &known_sections, &mut unknown);
    unknown.sort();
    unknown
}

fn collect_unknown(
    table: &toml::Table,
    prefix: &str,
    known_paths: &BTreeSet<String>,
    known_sections: &BTreeSet<&str>,
    unknown: &mut Vec<String>,
) {
    for (key, value) in table {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };

        if known_paths.contains(&path) {
            continue;
        }

        match value.as_table() {
            // A real section: recurse, so the report names the exact dead leaf
            // rather than blaming the whole `[terminal]` table for one typo.
            Some(child) if known_sections.contains(path.as_str()) => {
                collect_unknown(child, &path, known_paths, known_sections, unknown);
            }
            // Anything else is a dead end: an unknown leaf, an unknown table,
            // or a scalar written where a known section should be (`terminal = 3`),
            // which is equally inert and equally worth telling the user about.
            _ => unknown.push(path),
        }
    }
}

#[cfg(test)]
#[path = "settings_file_diagnostics_tests.rs"]
mod tests;
