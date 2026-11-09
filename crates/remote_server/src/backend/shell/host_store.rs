//! The codebase index's store on the HOST, reached over the shell (#1881).
//!
//! The owner's design: the host holds the flat-file store and the vectors. With the helper that
//! is `~/.phosphor/index/<hash>/`; without it the shell arm uses the very same directory and the
//! very same file formats ([`crate::index::store`]), so installing or removing the helper never
//! changes where the vectors are and never makes anything be embedded twice.
//!
//! Nothing can run on the host without the helper, so reads and searches happen app-side over a
//! LOCAL MIRROR of the two files they need (`meta.json` and one `vectors-*.bin`): a mirror file
//! is downloaded when the host's copy differs from the one mirrored (size and mtime, one `stat`
//! round trip), and the existing [`IndexStore`] code runs over it (brute-force cosine search,
//! known hashes, compaction). Writes go to the host first (atomic replace for `meta.json`, an
//! append of the new records for vectors, both from stdin), then the mirror follows. The API key
//! is never involved: only hashes, vectors and file text cross the channel.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::super::{BackendError, ShellBackend};
use super::index::ShellIndex;
use super::write::op_failed;
use crate::index::store::{IndexStore, StoredScan};
use crate::shellcmd::index_store::{
    FileStat, append_command, drop_other_vectors_command, list_vectors_command, parse_resolve,
    parse_stat, put_command, resolve_command, stat_command,
};
use crate::shellcmd::read::{parse_range_section, posix_range_section, pwsh_range_section};
use crate::shellcmd::{ParsedSection, Script, StderrMode};

/// One repository's store: where it is on the host and its mirror.
#[derive(Clone, Debug)]
pub(super) struct HostStore {
    /// The store directory on the host (absolute).
    pub dir: String,
    /// The mirror: an ordinary local store directory that follows the host's files.
    pub local: IndexStore,
}

/// What the app remembers about the store of one repository path.
#[derive(Clone, Debug)]
pub(super) struct HostLoc {
    pub dir: String,
}

#[derive(Debug, Default)]
pub(super) struct Mirrors {
    locs: Mutex<std::collections::HashMap<String, HostLoc>>,
    /// The host stat each mirrored file matched when it was last synced, by `<dir>/<name>`.
    stamps: Mutex<std::collections::HashMap<String, FileStat>>,
}

impl Mirrors {
    fn stamp(&self, key: &str) -> Option<FileStat> {
        self.stamps
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .copied()
    }

    fn set_stamp(&self, key: &str, stat: Option<FileStat>) {
        let mut stamps = self.stamps.lock().unwrap_or_else(|e| e.into_inner());
        match stat {
            Some(stat) => {
                stamps.insert(key.to_string(), stat);
            }
            None => {
                stamps.remove(key);
            }
        }
    }
}

fn stamp_key(dir: &str, name: &str) -> String {
    format!("{dir}/{name}")
}

impl ShellBackend {
    fn is_pwsh(&self) -> bool {
        self.shell_kind().is_powershell()
    }

    /// Runs one section and returns it.
    async fn run_section(
        &self,
        command: String,
        stdin: Vec<u8>,
        stderr: StderrMode,
    ) -> Result<ParsedSection, BackendError> {
        let script = Script::new(Self::new_nonce())?.section("s", command, stderr)?;
        let frame = self.run_script_stdin(&script, stdin).await?;
        frame
            .section("s")
            .cloned()
            .ok_or_else(|| op_failed("the host returned no answer"))
    }

    /// Finds `repo_path`'s store on the host. `Ok(None)`: `repo_path` is not a directory there.
    pub(super) async fn host_store(
        &self,
        index: &Arc<ShellIndex>,
        repo_path: &str,
    ) -> Result<Option<HostStore>, BackendError> {
        let local = index.store(repo_path);
        let cached = index
            .mirrors
            .locs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(repo_path)
            .cloned();
        if let Some(loc) = cached {
            return Ok(Some(HostStore {
                dir: loc.dir,
                local,
            }));
        }
        let section = self
            .run_section(
                resolve_command(self.is_pwsh(), repo_path),
                Vec::new(),
                StderrMode::Discard,
            )
            .await?;
        if section.exit != 0 {
            return Ok(None);
        }
        let Some((canonical, root)) = parse_resolve(&section.text()) else {
            return Err(op_failed("the host did not say where its index store is"));
        };
        let sep = if root.contains('\\') && !root.contains('/') {
            '\\'
        } else {
            '/'
        };
        let dir = format!(
            "{}{sep}{}",
            root.trim_end_matches(['/', '\\']),
            IndexStore::dir_name(&canonical)
        );
        index
            .mirrors
            .locs
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(repo_path.to_string(), HostLoc { dir: dir.clone() });
        Ok(Some(HostStore { dir, local }))
    }

    /// Stats `names` in the host store directory.
    pub(super) async fn stat_host_files(
        &self,
        dir: &str,
        names: &[&str],
    ) -> Result<std::collections::HashMap<String, FileStat>, BackendError> {
        let section = self
            .run_section(
                stat_command(self.is_pwsh(), dir, names),
                Vec::new(),
                StderrMode::Discard,
            )
            .await?;
        Ok(parse_stat(&section.text()))
    }

    /// Brings the mirror of `name` up to the host's copy. `Ok(None)`: the host has no such file
    /// (and the mirror has none either now).
    pub(super) async fn sync_host_file(
        &self,
        index: &Arc<ShellIndex>,
        store: &HostStore,
        name: &str,
    ) -> Result<Option<FileStat>, BackendError> {
        let key = stamp_key(&store.dir, name);
        let stats = self.stat_host_files(&store.dir, &[name]).await?;
        let Some(stat) = stats.get(name).copied() else {
            store.local.remove_file(name);
            index.mirrors.set_stamp(&key, None);
            return Ok(None);
        };
        if index.mirrors.stamp(&key) == Some(stat) && store.local.dir().join(name).exists() {
            return Ok(Some(stat));
        }
        let sep = if store.dir.contains('\\') && !store.dir.contains('/') {
            '\\'
        } else {
            '/'
        };
        let bytes = self
            .read_host_bytes(&format!("{}{sep}{name}", store.dir))
            .await?;
        store
            .local
            .put_file(name, &bytes)
            .map_err(|e| op_failed(format!("{e:#}")))?;
        // A file that changed between the stat and the read does not match its stamp; it is
        // fetched again next time.
        index
            .mirrors
            .set_stamp(&key, (bytes.len() as u64 == stat.size).then_some(stat));
        Ok(Some(stat))
    }

    /// The whole file at `path` on the host (the chunked reader, any size).
    async fn read_host_bytes(&self, path: &str) -> Result<Vec<u8>, BackendError> {
        self.read_file_bytes_shell(path.to_string()).await
    }

    /// Replaces `dir/name` on the host with `bytes` (atomically) and mirrors them.
    pub(super) async fn put_host_file(
        &self,
        index: &Arc<ShellIndex>,
        store: &HostStore,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<(), BackendError> {
        let section = self
            .run_section(
                put_command(self.is_pwsh(), &store.dir, name, &Self::new_nonce()),
                bytes.clone(),
                StderrMode::Merge,
            )
            .await?;
        if section.exit != 0 {
            return Err(op_failed(format!(
                "Failed to store {name} on the host (exit {}): {}",
                section.exit,
                section.text().trim()
            )));
        }
        store
            .local
            .put_file(name, &bytes)
            .map_err(|e| op_failed(format!("{e:#}")))?;
        let key = stamp_key(&store.dir, name);
        let stats = self.stat_host_files(&store.dir, &[name]).await?;
        index.mirrors.set_stamp(&key, stats.get(name).copied());
        Ok(())
    }

    /// Appends `records` to `dir/name` on the host, first cutting it back to `whole_len`.
    /// `Ok(false)`: the file is not what the caller saw (missing or shorter); rewrite it.
    pub(super) async fn append_host_file(
        &self,
        store: &HostStore,
        name: &str,
        whole_len: u64,
        records: Vec<u8>,
    ) -> Result<bool, BackendError> {
        use crate::shellcmd::index_store::{EXIT_MISSING, EXIT_SHORT};
        let section = self
            .run_section(
                append_command(self.is_pwsh(), &store.dir, name, whole_len),
                records,
                StderrMode::Merge,
            )
            .await?;
        match section.exit {
            0 => Ok(true),
            EXIT_MISSING | EXIT_SHORT => Ok(false),
            n => Err(op_failed(format!(
                "Failed to append to {name} on the host (exit {n}): {}",
                section.text().trim()
            ))),
        }
    }

    /// Reads the start of a host file (a vector file's header): up to `len` bytes.
    pub(super) async fn read_host_head(
        &self,
        path: &str,
        len: u64,
    ) -> Result<Vec<u8>, BackendError> {
        let pwsh = self.is_pwsh();
        let tool = if pwsh {
            None
        } else {
            self.base64_tool().await?
        };
        if !pwsh && tool.is_none() {
            return Err(op_failed("the host has no base64, openssl or od"));
        }
        let command = if pwsh {
            pwsh_range_section(path, 0, len)
        } else {
            posix_range_section(path, 0, len, tool.map_or("base64", |t| t.encoder_filter()))
        };
        let section = self
            .run_section(command, Vec::new(), StderrMode::Discard)
            .await?;
        parse_range_section(&section, tool).map_err(op_failed)
    }

    /// Deletes the host's vector files of every other embedding space.
    pub(super) async fn drop_other_host_vectors(
        &self,
        store: &HostStore,
        keep: &str,
    ) -> Result<(), BackendError> {
        self.run_section(
            drop_other_vectors_command(self.is_pwsh(), &store.dir, keep),
            Vec::new(),
            StderrMode::Discard,
        )
        .await
        .map(|_| ())
    }

    /// Rewrites each host vector file that holds much more than `live` needs (what the helper's
    /// scan does after every scan), keeping only live hashes. Costs one `ls`-sized call when
    /// nothing is wasted: the header and size of each file are all that is read.
    pub(super) async fn compact_host_vectors(
        &self,
        index: &Arc<ShellIndex>,
        store: &HostStore,
        live: &HashSet<String>,
    ) -> Result<(), BackendError> {
        let section = self
            .run_section(
                list_vectors_command(self.is_pwsh(), &store.dir),
                Vec::new(),
                StderrMode::Discard,
            )
            .await?;
        let files = parse_stat(&section.text());
        for (name, stat) in files {
            let sep = if store.dir.contains('\\') && !store.dir.contains('/') {
                '\\'
            } else {
                '/'
            };
            let head = self
                .read_host_head(
                    &format!("{}{sep}{name}", store.dir),
                    IndexStore::MAX_HEADER_BYTES.min(stat.size),
                )
                .await?;
            let Some((_key, _dims, header_len, record_size)) =
                IndexStore::parse_vector_header(&head)
            else {
                continue;
            };
            let records = stat.size.saturating_sub(header_len) / record_size;
            if records <= (live.len() as u64) * 3 / 2 + 64 {
                continue;
            }
            // Wasteful: fetch it, compact the mirror copy alone, and put it back.
            self.sync_host_file(index, store, &name).await?;
            drop_other_local_vectors(&store.local, &name);
            store
                .local
                .compact_wasteful(live)
                .map_err(|e| op_failed(format!("{e:#}")))?;
            let compacted = std::fs::read(store.local.dir().join(&name))
                .map_err(|e| op_failed(format!("{e}")))?;
            if compacted.len() as u64 != stat.size {
                self.put_host_file(index, store, &name, compacted).await?;
            }
        }
        Ok(())
    }

    /// `StoredScan` of the host store's `meta.json`, mirrored. `None`: no scan stored.
    pub(super) async fn load_host_scan(
        &self,
        index: &Arc<ShellIndex>,
        store: &HostStore,
    ) -> Result<Option<StoredScan>, BackendError> {
        if self
            .sync_host_file(index, store, IndexStore::META_FILE)
            .await?
            .is_none()
        {
            return Ok(None);
        }
        Ok(store.local.load_scan())
    }

    /// Forgets that `name` is mirrored (it is fetched again when next needed).
    pub(super) fn forget_mirror(&self, index: &Arc<ShellIndex>, store: &HostStore, name: &str) {
        index.mirrors.set_stamp(&stamp_key(&store.dir, name), None);
        store.local.remove_file(name);
    }

    /// Records that the mirror of `name` now matches the host's file (after the same bytes were
    /// appended to both).
    pub(super) async fn restamp(
        &self,
        index: &Arc<ShellIndex>,
        store: &HostStore,
        name: &str,
    ) -> Result<(), BackendError> {
        let stats = self.stat_host_files(&store.dir, &[name]).await?;
        index
            .mirrors
            .set_stamp(&stamp_key(&store.dir, name), stats.get(name).copied());
        Ok(())
    }

    /// Whether the mirror of `name` matches the host's current file (stat only, no download).
    pub(super) async fn mirror_is_current(
        &self,
        index: &Arc<ShellIndex>,
        store: &HostStore,
        name: &str,
    ) -> Result<(bool, Option<FileStat>), BackendError> {
        let stats = self.stat_host_files(&store.dir, &[name]).await?;
        let stat = stats.get(name).copied();
        let current = stat.is_some()
            && index.mirrors.stamp(&stamp_key(&store.dir, name)) == stat
            && store.local.dir().join(name).exists();
        Ok((current, stat))
    }
}

/// Removes every local `vectors-*.bin` but `keep` (so compaction touches only the file just
/// synced).
fn drop_other_local_vectors(local: &IndexStore, keep: &str) {
    let Ok(entries) = std::fs::read_dir(local.dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("vectors-") && name.ends_with(".bin") && name != keep {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}
