//! The filesystem watcher (`notify`, no debouncer) feeding `Event::FsChange` (Wave H parity,
//! family package P3). Reserved by P0; no requests of its own. Refcounted watch set: roots,
//! buffers, diff repos and skill dirs. On inotify cap, ENOSPC or NFS the repo is marked
//! "unwatched" and falls back to the shell arm's refresh events (design ruling R5).
