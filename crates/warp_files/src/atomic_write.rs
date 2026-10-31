//! Crash-safe replacement of a local file's contents.
//!
//! `async_fs::write` truncates the target and then writes, so a crash, a kill or a full disk
//! part-way through leaves the user's file truncated or half-written. [`write_atomic`] instead
//! writes a temporary file in the same directory, fsyncs it, and renames it over the target:
//! a reader (or a crash) sees the old content or the new content, never a mixture.
//!
//! What is preserved: the permission bits, the owner and group (where the process may set
//! them), and symlinks (the file the link points to is replaced, not the link). What is not:
//! extended attributes and ACLs. Where preserving the file's identity is not possible, the
//! write falls back to the old in-place behaviour rather than silently changing the file:
//!
//! - the directory is not writable (so no temp file can be created) but the file is,
//! - the file has several hard links (a rename would detach the others),
//! - the file is not a regular file (device, FIFO),
//! - the owner or group cannot be restored on the temp file,
//! - on Windows, the rename is refused because another process holds the file open.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use futures::io::AsyncWriteExt;

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Replaces the contents of `path` with `content`, atomically where possible (see the module
/// docs). Creates the file if it does not exist; the parent directory must already exist.
pub async fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    // Edit the file a symlink points to, not the link itself.
    let target = match async_fs::symlink_metadata(path).await {
        Ok(meta) if meta.file_type().is_symlink() => match async_fs::canonicalize(path).await {
            Ok(resolved) => resolved,
            // A dangling link: writing through it creates its target, which a rename would not.
            Err(_) => return async_fs::write(path, content).await,
        },
        _ => path.to_path_buf(),
    };

    let existing = match async_fs::metadata(&target).await {
        Ok(meta) if meta.is_file() => {
            if has_other_hard_links(&meta) {
                return async_fs::write(&target, content).await;
            }
            Some(meta)
        }
        // Not a regular file: leave it to the OS.
        Ok(_) => return async_fs::write(&target, content).await,
        Err(err) if err.kind() == io::ErrorKind::NotFound => None,
        Err(err) => return Err(err),
    };

    let temp = temp_path_for(&target);
    let mut file = match async_fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .await
    {
        Ok(file) => file,
        // The file may be writable in a directory that is not.
        Err(err) if err.kind() == io::ErrorKind::PermissionDenied && existing.is_some() => {
            return async_fs::write(&target, content).await;
        }
        Err(err) => return Err(err),
    };

    match write_temp_and_rename(&mut file, &temp, &target, existing.as_ref(), content).await {
        Ok(true) => Ok(()),
        Ok(false) => {
            drop(file);
            let _ = async_fs::remove_file(&temp).await;
            async_fs::write(&target, content).await
        }
        Err(err) => {
            drop(file);
            let _ = async_fs::remove_file(&temp).await;
            Err(err)
        }
    }
}

/// Returns `Ok(false)` when the temp file could not take over the original's identity and the
/// caller should write in place instead.
async fn write_temp_and_rename(
    file: &mut async_fs::File,
    temp: &Path,
    target: &Path,
    existing: Option<&std::fs::Metadata>,
    content: &[u8],
) -> io::Result<bool> {
    file.write_all(content).await?;
    file.flush().await?;
    if let Some(meta) = existing {
        async_fs::set_permissions(temp, meta.permissions()).await?;
        if !restore_owner(temp, meta).await? {
            return Ok(false);
        }
    }
    // Durable before it becomes visible under the target's name.
    file.sync_all().await?;

    if let Err(err) = async_fs::rename(temp, target).await {
        // Windows refuses to rename over a file another process (or our own not yet
        // closed handle) holds open.
        if cfg!(windows) && err.kind() == io::ErrorKind::PermissionDenied {
            return Ok(false);
        }
        return Err(err);
    }
    sync_parent_dir(target).await;
    Ok(true)
}

fn temp_path_for(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(".{name}.phosphor-{}-{unique}.tmp", std::process::id());
    match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(temp_name),
        _ => PathBuf::from(temp_name),
    }
}

#[cfg(unix)]
fn has_other_hard_links(meta: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}

#[cfg(not(unix))]
fn has_other_hard_links(_meta: &std::fs::Metadata) -> bool {
    false
}

/// Gives `temp` the owner and group of the file it replaces. `Ok(false)` means they differ and
/// could not be set, so replacing the file would change who owns it.
#[cfg(unix)]
async fn restore_owner(temp: &Path, original: &std::fs::Metadata) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let created = async_fs::metadata(temp).await?;
    if created.uid() == original.uid() && created.gid() == original.gid() {
        return Ok(true);
    }
    Ok(std::os::unix::fs::chown(temp, Some(original.uid()), Some(original.gid())).is_ok())
}

#[cfg(not(unix))]
async fn restore_owner(_temp: &Path, _original: &std::fs::Metadata) -> io::Result<bool> {
    Ok(true)
}

/// Best effort: makes the rename itself durable. Failure is not a write failure.
#[cfg(unix)]
async fn sync_parent_dir(target: &Path) {
    let dir = match target.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    if let Ok(dir) = async_fs::File::open(dir).await {
        let _ = dir.sync_all().await;
    }
}

#[cfg(not(unix))]
async fn sync_parent_dir(_target: &Path) {}

#[cfg(test)]
#[path = "atomic_write_tests.rs"]
mod tests;
