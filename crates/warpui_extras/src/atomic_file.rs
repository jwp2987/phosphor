//! Crash- and disk-full-safe replacement of a user's file.
//!
//! `std::fs::write` truncates the target and then writes into it, so any
//! failure part-way -- a full disk, a quota, a crash, a kill -- leaves the
//! user's file truncated. For `settings.toml` that meant a file cut off
//! mid-line, which then failed to parse, so the next launch started on
//! defaults (#859).
//!
//! [`write`] instead writes a temporary file in the same directory, flushes it
//! to disk, and renames it over the target. A rename within one filesystem is
//! atomic: a reader (or the next launch) sees either the old file or the new
//! one, never a mixture. If anything fails before the rename, the temporary
//! file is removed and the original is untouched.
//!
//! # Symlinks
//!
//! If the target is a symlink (dotfile managers such as stow, chezmoi and
//! home-manager keep config files that way), the write goes **through** the
//! link: the file the link resolves to is replaced, and the link itself is
//! left in place. Renaming over the link instead would silently turn a managed
//! file into an unmanaged copy. Resolution follows the chain hop by hop, so a
//! dangling link creates its target, just as `std::fs::write` would.
//!
//! # Metadata
//!
//! A target the user cannot write to is refused with the same error
//! `std::fs::write` gives, even though the rename would be allowed by the
//! directory's permissions. The replacement keeps the original file's
//! permission bits. Ownership,
//! extended attributes and ACLs are not carried over (changing ownership needs
//! privileges the app does not have); the new file is owned by the writing
//! user, which for a file in the user's own config directory is what it
//! already was. Hard links to the original are not updated -- they keep the
//! old contents, as with any editor that saves by rename.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Symlink hops followed before giving up, matching the kernel's `MAXSYMLINKS`.
const MAX_SYMLINK_HOPS: usize = 40;

/// Atomically replaces the contents of `path` with `contents`.
///
/// Creates missing parent directories. On error the original file (if any)
/// is unchanged and no temporary file is left behind.
pub fn write(path: impl AsRef<Path>, contents: impl AsRef<[u8]>) -> io::Result<()> {
    let contents = contents.as_ref();
    write_with(path.as_ref(), |file| file.write_all(contents))
}

/// Like [`write`], but the contents are produced by `fill`, which writes into
/// the temporary file. Exposed to tests so a write that fails part-way (as on
/// a full disk) can be simulated.
fn write_with(path: &Path, fill: impl FnOnce(&mut File) -> io::Result<()>) -> io::Result<()> {
    let target = resolve_symlinks(path)?;
    let dir = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.to_path_buf(),
        _ => PathBuf::from("."),
    };
    fs::create_dir_all(&dir)?;

    // Permissions to carry over. `None` for a new file, which then gets the
    // same default mode `std::fs::write` would give it (0o666 & !umask).
    let existing_permissions = match fs::metadata(&target) {
        Ok(metadata) => Some(metadata.permissions()),
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };
    if existing_permissions.is_some() {
        // A rename only needs write access to the directory, so without this
        // a file the user made read-only (to stop the app changing it) would
        // be replaced anyway. Refuse exactly when `std::fs::write` would have:
        // opening for writing, without truncating, modifies nothing.
        OpenOptions::new().write(true).open(&target)?;
    }

    let (temp_path, mut temp_file) = create_temp_file(&dir, &target)?;
    let result = (|| {
        if let Some(permissions) = existing_permissions {
            temp_file.set_permissions(permissions)?;
        }
        fill(&mut temp_file)?;
        temp_file.sync_all()?;
        drop(temp_file);
        fs::rename(&temp_path, &target)
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    // Make the rename itself durable. Best effort: some filesystems refuse to
    // fsync a directory, and the data is already safely in place either way.
    #[cfg(unix)]
    if let Ok(dir_handle) = File::open(&dir) {
        let _ = dir_handle.sync_all();
    }
    Ok(())
}

/// Follows `path` through any chain of symlinks to the path that will
/// actually hold the data. Relative link targets resolve against the link's
/// own directory. A dangling link resolves to the missing target.
fn resolve_symlinks(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_SYMLINK_HOPS {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let link_target = fs::read_link(&current)?;
                current = match current.parent() {
                    Some(parent) if link_target.is_relative() => parent.join(link_target),
                    _ => link_target,
                };
            }
            Ok(_) => return Ok(current),
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(current),
            Err(err) => return Err(err),
        }
    }
    Err(io::Error::other(format!(
        "too many levels of symbolic links: {}",
        path.display()
    )))
}

/// Creates a new, uniquely named file next to `target`.
fn create_temp_file(dir: &Path, target: &Path) -> io::Result<(PathBuf, File)> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let file_name = target
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_owned());
    loop {
        let temp_path = dir.join(format!(
            ".{file_name}.tmp-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
        {
            Ok(file) => return Ok((temp_path, file)),
            // A stale temp file from a crashed process with a recycled pid.
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
#[path = "atomic_file_tests.rs"]
mod tests;
