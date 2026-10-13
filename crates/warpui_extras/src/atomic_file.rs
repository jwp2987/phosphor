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
//!
//! # Where a rename is impossible
//!
//! A writable file in a directory the user cannot create files in, or a file
//! that is itself a mount point (single-file bind mounts in containers), cannot
//! be replaced by rename. Only in those cases -- see `should_write_in_place` --
//! the write falls back to the old truncate-and-write, which is no worse than
//! before. A full disk or an I/O error never falls back.
//!
//! # Leftovers
//!
//! A crash or kill between creating the temp file and renaming it leaves a
//! `.{name}.tmp-*` file next to the target. Every write sweeps that target's
//! temp files older than an hour.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

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
/// the file. Exposed to tests so a write that fails part-way (as on a full
/// disk) can be simulated. `fill` may run twice: once into the temporary file
/// and, only on the in-place fallback, once more into the target.
fn write_with(path: &Path, fill: impl Fn(&mut File) -> io::Result<()>) -> io::Result<()> {
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
    // The probe above returned early unless an existing target is writable.
    let target_writable = existing_permissions.is_some();

    sweep_stale_temp_files(&dir, &target);

    let (temp_path, mut temp_file) = match create_temp_file(&dir, &target) {
        Ok(created) => created,
        Err(err) => return write_in_place_or_fail(err, &target, target_writable, &fill),
    };
    let prepared = prepare_temp_file(&mut temp_file, existing_permissions, &fill);
    drop(temp_file);
    if let Err(err) = prepared {
        // Never falls back: this is where a full disk or an I/O error shows
        // up, and writing in place is exactly what truncated the file.
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }
    if let Err(err) = fs::rename(&temp_path, &target) {
        let _ = fs::remove_file(&temp_path);
        return write_in_place_or_fail(err, &target, target_writable, &fill);
    }

    // Make the rename itself durable. Best effort: some filesystems refuse to
    // fsync a directory, and the data is already safely in place either way.
    #[cfg(unix)]
    if let Ok(dir_handle) = File::open(&dir) {
        let _ = dir_handle.sync_all();
    }
    Ok(())
}

fn prepare_temp_file(
    temp_file: &mut File,
    permissions: Option<fs::Permissions>,
    fill: &impl Fn(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(permissions) = permissions {
        temp_file.set_permissions(permissions)?;
    }
    fill(temp_file)?;
    temp_file.sync_all()
}

/// Whether a failure to create the temporary file, or to rename it over the
/// target, should be retried as a plain in-place write.
///
/// Only for errors that mean "this location cannot be replaced by rename",
/// never "the write itself cannot succeed":
/// - `PermissionDenied` (EACCES/EPERM): a writable file in a directory the
///   user cannot create files in.
/// - `ResourceBusy` (EBUSY): the target is a mount point -- a single-file
///   bind mount, as container setups use for config files.
/// - `CrossesDevices` (EXDEV): same cause, reported by some kernels.
///
/// And only when the target already exists and is writable; otherwise an
/// in-place write would fail too. `StorageFull`, I/O errors, quotas and
/// everything else return the original error: an in-place write is what
/// truncated the file on a full disk (#859).
fn should_write_in_place(err: &io::Error, target_writable: bool) -> bool {
    target_writable
        && matches!(
            err.kind(),
            io::ErrorKind::PermissionDenied
                | io::ErrorKind::ResourceBusy
                | io::ErrorKind::CrossesDevices
        )
}

/// The fallback: the pre-#859 truncate-and-write, used only where an atomic
/// replace is impossible (see [`should_write_in_place`]).
fn write_in_place_or_fail(
    err: io::Error,
    target: &Path,
    target_writable: bool,
    fill: &impl Fn(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    if !should_write_in_place(&err, target_writable) {
        return Err(err);
    }
    log::warn!(
        "Cannot atomically replace {} ({err}); writing it in place instead",
        target.display()
    );
    let mut file = OpenOptions::new().write(true).truncate(true).open(target)?;
    fill(&mut file)?;
    file.sync_all()
}

/// Temp files older than this are leftovers of a crash (or a kill between
/// creating and renaming) and are removed on the next write to the same file.
const STALE_TEMP_AGE: Duration = Duration::from_secs(60 * 60);

/// Whether `name` is a temp file [`create_temp_file`] makes for `file_name`.
fn is_temp_file_for(name: &str, file_name: &str) -> bool {
    name.strip_prefix('.')
        .and_then(|rest| rest.strip_prefix(file_name))
        .is_some_and(|rest| rest.starts_with(".tmp-"))
}

/// Best-effort removal of this file's temp files left behind by a crashed
/// write. Only names this module creates for this target, and only ones older
/// than [`STALE_TEMP_AGE`], so a concurrent writer's in-flight file is safe.
fn sweep_stale_temp_files(dir: &Path, target: &Path) {
    let Some(file_name) = target.file_name().and_then(|name| name.to_str()) else {
        return;
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name
            .to_str()
            .is_some_and(|name| is_temp_file_for(name, file_name))
        {
            continue;
        }
        let stale = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_TEMP_AGE);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
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
