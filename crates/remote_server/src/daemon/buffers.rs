//! Server-side text buffers: `OpenBuffer`, `BufferEdit`, `SaveBuffer`, `ResolveConflict`,
//! `CloseBuffer`, and the `BufferUpdatedPush` / `BufferConflictDetected` pushes (Wave H parity,
//! family package P4, family c). The warpui-free equivalent of the app daemon's
//! `ServerModel` + `GlobalBufferModel` server-local mode, with the same wire semantics:
//!
//! * Offsets are 1-indexed CHARACTER offsets (`CharOffset`): `1` is before the first character
//!   and `char_count + 1` after the last. A batch of edits is applied in order, each against the
//!   text left by the one before it, and every offset is clamped to the current end.
//! * Two counters per buffer: `server_version` (bumped when the text changes because the disk
//!   changed) and `client_version` (the last `new_client_version` the client sent). A
//!   `BufferEdit` whose `expected_server_version` is not the current one is dropped silently (no
//!   response, no state change). An accepted edit sets `client_version` and is NOT forwarded to
//!   the other connections: only one client should hold a writable view (a v0 limit of the wire
//!   protocol, kept as it was).
//! * A change on disk is told apart from our own save by the SHA-256 of the file. If the buffer
//!   has no unsaved client edits the change is applied (one replacement edit covering only the
//!   differing middle), `server_version` is bumped and a `BufferUpdatedPush` carrying
//!   `expected_client_version = client_version` goes to every connection. If it has unsaved
//!   edits the change is NOT applied and a `BufferConflictDetected` goes to every connection.
//! * `OpenBuffer { force_reload }` re-reads the disk into a tracked buffer, bumps
//!   `server_version`, resets `client_version` to 0 and tells the OTHER connections with a
//!   `BufferUpdatedPush` that replaces the whole text. `ResolveConflict` replaces the text with
//!   the client's, keeps `server_version`, adopts the client's version and saves, but only if
//!   `acknowledged_server_version` is still current.
//! * Saves write a temp file beside the target and rename it over (mode kept), so a reader never
//!   sees half a file.
//!
//! The directory of every open buffer holds one [`WatchSet`] reference for as long as the buffer
//! is open (non-recursive watches see edits and renames of the files inside). No timers, no
//! debounce: one filesystem event is one comparison.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::jobs::Reply;
use super::server::{ConnId, Pusher};
use super::watch::{FsKind, WatchSet};
use crate::proto::{
    BufferConflictDetected, BufferEdit, BufferUpdatedPush, ErrorCode, FileOperationError,
    OpenBuffer, OpenBufferResponse, ResolveConflict, ResolveConflictResponse,
    ResolveConflictSuccess, SaveBuffer, SaveBufferResponse, SaveBufferSuccess, TextEdit,
    resolve_conflict_response, save_buffer_response, server_message,
};

/// The capability this module serves.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::BUFFERS;

/// The largest file a buffer opens (what a whole-file read over the shell arm returns too).
const MAX_BUFFER_BYTES: u64 = 7 * 1024 * 1024;

/// The first `server_version` of a freshly opened buffer.
const FIRST_SERVER_VERSION: u64 = 1;

type Digest32 = [u8; 32];

struct Buffer {
    text: String,
    server_version: u64,
    client_version: u64,
    /// Hash of the file as last read or written by us: a filesystem event for a file with this
    /// hash is our own write (or a change already handled).
    disk_hash: Digest32,
    /// The client edited the text since it was last loaded or saved.
    dirty: bool,
    conns: BTreeSet<ConnId>,
    /// Whether this buffer holds a watch on its directory.
    watching: bool,
}

/// All open buffers of the daemon, by path.
#[derive(Default)]
pub(super) struct Buffers {
    open: HashMap<PathBuf, Buffer>,
}

fn hash(bytes: &[u8]) -> Digest32 {
    Sha256::digest(bytes).into()
}

/// The file's text and hash, or the message to send back.
fn read_text(path: &Path) -> Result<(String, Digest32), String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| format!("Failed to load buffer {}: {e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!(
            "Failed to load buffer {}: not a regular file",
            path.display()
        ));
    }
    if meta.len() > MAX_BUFFER_BYTES {
        return Err(format!(
            "Failed to load buffer {}: the file is too large to edit remotely",
            path.display()
        ));
    }
    let bytes = std::fs::read(path)
        .map_err(|e| format!("Failed to load buffer {}: {e}", path.display()))?;
    let digest = hash(&bytes);
    let text = String::from_utf8(bytes).map_err(|_| {
        format!(
            "Failed to load buffer {}: the file is not valid UTF-8 text",
            path.display()
        )
    })?;
    Ok((text, digest))
}

/// Follows a chain of symlinks at `path` by hand (at most 40 hops), so a write goes THROUGH a
/// link instead of replacing it; a dangling link resolves to its missing target. The shell arm
/// does the same with `readlink` (#1879).
pub(super) fn resolve_symlinks(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    for _ in 0..40 {
        let is_link = std::fs::symlink_metadata(&current)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false);
        if !is_link {
            break;
        }
        let Ok(link) = std::fs::read_link(&current) else {
            break;
        };
        current = if link.is_absolute() {
            link
        } else {
            current
                .parent()
                .map_or_else(|| link.clone(), |parent| parent.join(&link))
        };
    }
    current
}

/// Writes `text` over `path` through a temp file beside it, keeping the target's mode.
fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    write_bytes_atomically(path, text.as_bytes())
}

/// Writes `bytes` over `path` through a hidden temp file (`.<name>.phosphor-tmp-*`) beside it
/// and a rename, so a reader never sees half a file and a failure leaves the old one (#1867).
/// A symlink at `path` is written through. The target's mode is kept, and its owner and group as
/// far as the user may set them. Like the shell arm, this REPLACES the inode: hard links to the
/// old file keep the old bytes and ACLs/xattrs are not carried over (#1876). A directory at
/// `path` is an error and nothing is created.
pub(super) fn write_bytes_atomically(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = resolve_symlinks(path);
    let path = path.as_path();
    let existing = std::fs::metadata(path).ok();
    if existing.as_ref().is_some_and(|m| m.is_dir()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::IsADirectory,
            "the path is a directory",
        ));
    }
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = {
        let tmp_name = format!(
            ".{name}.phosphor-tmp-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        );
        match dir {
            Some(dir) => dir.join(tmp_name),
            None => PathBuf::from(tmp_name),
        }
    };
    let result = (|| {
        std::fs::write(&tmp, bytes)?;
        if let Some(meta) = &existing {
            std::fs::set_permissions(&tmp, meta.permissions())?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                // Best effort: `chown` needs privilege, `chgrp` a group the user is in.
                if std::os::unix::fs::chown(&tmp, Some(meta.uid()), Some(meta.gid())).is_err() {
                    let _ = std::os::unix::fs::chown(&tmp, None, Some(meta.gid()));
                }
            }
        }
        std::fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// Byte index of the character at 0-based position `chars` (the text length for one past the
/// end).
fn byte_index(text: &str, chars: usize) -> usize {
    text.char_indices()
        .nth(chars)
        .map_or(text.len(), |(index, _)| index)
}

/// Applies `edits` in order. Offsets are 1-indexed character offsets, clamped to the end of the
/// text as it is when that edit runs (an end before its start becomes an insert there).
fn apply_edits(text: &mut String, edits: &[TextEdit]) {
    for edit in edits {
        let count = text.chars().count();
        let zero_based = |offset: u64| {
            usize::try_from(offset)
                .unwrap_or(usize::MAX)
                .saturating_sub(1)
        };
        let start = zero_based(edit.start_offset).min(count);
        let end = zero_based(edit.end_offset).min(count).max(start);
        let (from, to) = (byte_index(text, start), byte_index(text, end));
        text.replace_range(from..to, &edit.text);
    }
}

/// The one edit that turns `old` into `new`: the differing middle, between the longest common
/// prefix and suffix (in characters). `None` when they are equal.
fn diff_edit(old: &str, new: &str) -> Option<TextEdit> {
    if old == new {
        return None;
    }
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .count();
    let old_rest = old.chars().count() - prefix;
    let new_rest = new.chars().count() - prefix;
    let suffix = old
        .chars()
        .rev()
        .zip(new.chars().rev())
        .take_while(|(a, b)| a == b)
        .count()
        .min(old_rest)
        .min(new_rest);
    let old_end = prefix + old_rest - suffix;
    let new_end = prefix + new_rest - suffix;
    let (from, to) = (byte_index(new, prefix), byte_index(new, new_end));
    Some(TextEdit {
        start_offset: prefix as u64 + 1,
        end_offset: old_end as u64 + 1,
        text: new[from..to].to_owned(),
    })
}

fn error_response(reply: &Reply, message: String) {
    reply.error(ErrorCode::Internal, message);
}

fn lossy(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl Buffers {
    /// `OpenBuffer`: reads the file (or re-reads it for `force_reload`), starts following it and
    /// answers with the text and `server_version`.
    pub(super) fn open(
        &mut self,
        conn: ConnId,
        msg: OpenBuffer,
        reply: &Reply,
        watches: &mut WatchSet,
        push: &Pusher<'_>,
    ) {
        log::info!(
            "Handling OpenBuffer force_reload={} (conn={conn})",
            msg.force_reload
        );
        let path = PathBuf::from(&msg.path);
        if let Some(buffer) = self.open.get_mut(&path) {
            if msg.force_reload {
                let (text, digest) = match read_text(&path) {
                    Ok(read) => read,
                    Err(message) => return error_response(reply, message),
                };
                let old_chars = buffer.text.chars().count() as u64;
                let expected_client_version = buffer.client_version;
                buffer.text = text;
                buffer.disk_hash = digest;
                buffer.dirty = false;
                buffer.client_version = 0;
                buffer.server_version += 1;
                let update = BufferUpdatedPush {
                    path: msg.path.clone(),
                    new_server_version: buffer.server_version,
                    expected_client_version,
                    edits: vec![TextEdit {
                        start_offset: 1,
                        end_offset: old_chars + 1,
                        text: buffer.text.clone(),
                    }],
                };
                for other in buffer.conns.iter().filter(|c| **c != conn) {
                    push.send(
                        *other,
                        server_message::Message::BufferUpdated(update.clone()),
                    );
                }
            }
            buffer.conns.insert(conn);
            reply.send(server_message::Message::OpenBufferResponse(
                OpenBufferResponse {
                    content: buffer.text.clone(),
                    server_version: buffer.server_version,
                },
            ));
            return;
        }
        let (text, digest) = match read_text(&path) {
            Ok(read) => read,
            Err(message) => return error_response(reply, message),
        };
        let watching = match path.parent() {
            Some(dir) => match watches.acquire(dir) {
                Ok(()) => true,
                Err(e) => {
                    // No live conflict detection for this buffer; edits and saves still work.
                    log::warn!("buffer is not watched: {e}");
                    false
                }
            },
            None => false,
        };
        let response = OpenBufferResponse {
            content: text.clone(),
            server_version: FIRST_SERVER_VERSION,
        };
        self.open.insert(
            path,
            Buffer {
                text,
                server_version: FIRST_SERVER_VERSION,
                client_version: 0,
                disk_hash: digest,
                dirty: false,
                conns: BTreeSet::from([conn]),
                watching,
            },
        );
        reply.send(server_message::Message::OpenBufferResponse(response));
    }

    /// `BufferEdit` (a notification): applies the edits when `expected_server_version` is the
    /// current one, drops them otherwise.
    pub(super) fn edit(&mut self, msg: BufferEdit) {
        let Some(buffer) = self.open.get_mut(Path::new(&msg.path)) else {
            log::warn!("BufferEdit for an unknown buffer");
            return;
        };
        if msg.expected_server_version != buffer.server_version {
            log::info!(
                "Rejected client edit: expected S={}, actual S={}",
                msg.expected_server_version,
                buffer.server_version
            );
            return;
        }
        buffer.client_version = msg.new_client_version;
        apply_edits(&mut buffer.text, &msg.edits);
        buffer.dirty = true;
    }

    /// `SaveBuffer`: writes the buffer's text to its file.
    pub(super) fn save(&mut self, msg: SaveBuffer, reply: &Reply) {
        log::info!("Handling SaveBuffer");
        let result = match self.open.get_mut(Path::new(&msg.path)) {
            None => Err(format!("Buffer not open: {}", msg.path)),
            Some(buffer) => save_buffer(Path::new(&msg.path), buffer)
                .map_err(|e| format!("Failed to save: {e}")),
        };
        reply.send(server_message::Message::SaveBufferResponse(
            SaveBufferResponse {
                result: Some(match result {
                    Ok(()) => save_buffer_response::Result::Success(SaveBufferSuccess {}),
                    Err(message) => {
                        save_buffer_response::Result::Error(FileOperationError { message })
                    }
                }),
            },
        ));
    }

    /// `ResolveConflict`: "keep mine". Replaces the text with the client's, adopts its version
    /// and saves. Refused if the server version moved on since the client saw the conflict.
    pub(super) fn resolve(&mut self, msg: ResolveConflict, reply: &Reply) {
        log::info!("Handling ResolveConflict");
        let result = match self.open.get_mut(Path::new(&msg.path)) {
            None => Err(format!("Buffer not open: {}", msg.path)),
            Some(buffer) if buffer.server_version != msg.acknowledged_server_version => {
                Err("Failed to resolve conflict: Stale conflict resolution".to_owned())
            }
            Some(buffer) => {
                buffer.text = msg.client_content;
                buffer.client_version = msg.current_client_version;
                save_buffer(Path::new(&msg.path), buffer)
                    .map_err(|e| format!("Failed to resolve conflict: {e}"))
            }
        };
        reply.send(server_message::Message::ResolveConflictResponse(
            ResolveConflictResponse {
                result: Some(match result {
                    Ok(()) => resolve_conflict_response::Result::Success(ResolveConflictSuccess {}),
                    Err(message) => {
                        resolve_conflict_response::Result::Error(FileOperationError { message })
                    }
                }),
            },
        ));
    }

    /// `CloseBuffer` (a notification): the connection stops following; the last one frees it.
    pub(super) fn close(&mut self, conn: ConnId, path: &str, watches: &mut WatchSet) {
        log::info!("Handling CloseBuffer (conn={conn})");
        let path = PathBuf::from(path);
        if let Some(buffer) = self.open.get_mut(&path) {
            buffer.conns.remove(&conn);
            if buffer.conns.is_empty() {
                self.drop_buffer(&path, watches);
            }
        }
    }

    /// A connection went away: it follows nothing any more.
    pub(super) fn conn_gone(&mut self, conn: ConnId, watches: &mut WatchSet) {
        let orphaned: Vec<PathBuf> = self
            .open
            .iter_mut()
            .filter_map(|(path, buffer)| {
                buffer.conns.remove(&conn);
                buffer.conns.is_empty().then(|| path.clone())
            })
            .collect();
        for path in orphaned {
            self.drop_buffer(&path, watches);
        }
    }

    fn drop_buffer(&mut self, path: &Path, watches: &mut WatchSet) {
        if let Some(buffer) = self.open.remove(path)
            && buffer.watching
            && let Some(dir) = path.parent()
        {
            watches.release(dir);
        }
    }

    /// Whether any buffer is open (for tests).
    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.open.is_empty()
    }

    /// The watcher saw a change: compares every affected buffer's file with what we last saw.
    pub(super) fn on_fs_change(&mut self, paths: &[PathBuf], kind: FsKind, push: &Pusher<'_>) {
        for (path, buffer) in &mut self.open {
            if kind != FsKind::Rescan && !paths.iter().any(|p| p == path) {
                continue;
            }
            // A removed or unreadable file is left alone: the next save recreates it.
            let Ok((text, digest)) = read_text(path) else {
                continue;
            };
            if digest == buffer.disk_hash {
                continue;
            }
            buffer.disk_hash = digest;
            if buffer.dirty {
                for conn in &buffer.conns {
                    push.send(
                        *conn,
                        server_message::Message::BufferConflictDetected(BufferConflictDetected {
                            path: lossy(path),
                        }),
                    );
                }
                continue;
            }
            let Some(edit) = diff_edit(&buffer.text, &text) else {
                continue;
            };
            buffer.text = text;
            buffer.server_version += 1;
            let update = BufferUpdatedPush {
                path: lossy(path),
                new_server_version: buffer.server_version,
                expected_client_version: buffer.client_version,
                edits: vec![edit],
            };
            for conn in &buffer.conns {
                push.send(
                    *conn,
                    server_message::Message::BufferUpdated(update.clone()),
                );
            }
        }
    }
}

fn save_buffer(path: &Path, buffer: &mut Buffer) -> std::io::Result<()> {
    write_atomically(path, &buffer.text)?;
    buffer.disk_hash = hash(buffer.text.as_bytes());
    buffer.dirty = false;
    Ok(())
}

#[cfg(test)]
#[path = "buffers_tests.rs"]
mod tests;
