//! Codebase indexing on the shell arm (family `CodebaseChunks`): the same four requests the
//! helper serves, answered by running ordinary commands on the host over the out-of-band channel.
//! The store (scan table and vectors) lives ON THE HOST, in the helper's own directory and
//! formats (see [`super::host_store`]), so switching the helper on or off never re-embeds.
//!
//! * `ScanCodebase`: commands list and hash the files a page of [`SCAN_PAGE_FILES`] at a time
//!   ([`shellcmd::index`]), so no repository size hits the output cap or the timeout; only files
//!   whose hash is new are read (base64, in batches that split themselves when the output cap
//!   is hit) and chunked locally with the shared chunker. The text of those new files stays in a
//!   bounded in-memory cache so the `GetChunks` that follows does not read them a second time.
//! * `GetChunks`: from that cache, else read again from the host and checked against the hash.
//! * `StoreVectors`: appended to the host's vector file. `QueryVectors`: brute-force cosine
//!   search over the app-side mirror of the host's files.
//!
//! PowerShell hosts get PowerShell listing, hashing and store commands.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use super::super::executor::{ExecRequest, MAX_EXEC_OUTPUT_BYTES};
use super::super::{BackendError, Family, ShellBackend};
use super::host_store::Mirrors;
use crate::index::proto::{
    MAX_CHUNKS_PER_REQUEST, chunks_error, query_error, scan_error, scan_to_proto, store_error,
};
use crate::index::store::IndexStore;
use crate::index::types::{
    ChunkEntry, ManifestEntry, Scan, chunk_file, looks_binary, root_hash, sha256_hex,
};
use crate::proto::{
    CodebaseChunkText, GetChunks, GetChunksResponse, GetChunksSuccess, QueryVectors,
    QueryVectorsResponse, ScanCodebaseResponse, StoreVectors, StoreVectorsResponse,
    get_chunks_response, scan_codebase_response,
};
use crate::shellcmd::base64::Base64Tool;
use crate::shellcmd::chunk::MAX_FILE_BYTES;
use crate::shellcmd::index::{
    EXIT_NO_DIRECTORY, EXIT_NO_HASH_TOOL, SCAN_PAGE_FILES, parse_scan_output, parse_total,
    read_file_command, read_file_command_pwsh, scan_page_command, scan_page_command_pwsh,
};
use crate::shellcmd::{Frame, Script, StderrMode};

/// Files read per command at first; a batch that goes over the output cap is halved.
const READ_BATCH_FILES: usize = 32;
/// Text kept between a scan and the `GetChunks` that follows it.
const TEXT_CACHE_CAP_BYTES: usize = 64 * 1024 * 1024;

/// One host's app-side index state: where the local mirrors of its stores live, what is
/// mirrored, and the scan-to-chunks text cache. Shared by every backend the router builds for the
/// host.
#[derive(Debug)]
pub struct ShellIndex {
    root: PathBuf,
    /// Stable identity of the host (for an ssh session `user@host`), so the store survives
    /// reconnects.
    identity: String,
    text: Mutex<TextCache>,
    /// Where each repository's store is on the host, and which host files the mirror matches.
    pub(super) mirrors: Mirrors,
}

#[derive(Debug, Default)]
struct TextCache {
    bytes: usize,
    by_hash: HashMap<String, String>,
}

impl ShellIndex {
    pub fn new(root: PathBuf, identity: String) -> Self {
        Self {
            root,
            identity,
            text: Mutex::default(),
            mirrors: Mirrors::default(),
        }
    }

    /// The local mirror of `repo_path`'s store on the host.
    pub fn store(&self, repo_path: &str) -> IndexStore {
        IndexStore::open(&self.root, &format!("{}\n{repo_path}", self.identity))
    }

    fn lock_text(&self) -> std::sync::MutexGuard<'_, TextCache> {
        self.text.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn clear_text(&self) {
        *self.lock_text() = TextCache::default();
    }

    /// Remembers `text` for `hash` unless the cache is full.
    fn stash(&self, hash: &str, text: &str) {
        let mut cache = self.lock_text();
        if cache.bytes + text.len() > TEXT_CACHE_CAP_BYTES {
            return;
        }
        if cache
            .by_hash
            .insert(hash.to_string(), text.to_string())
            .is_none()
        {
            cache.bytes += text.len();
        }
    }

    fn take(&self, hash: &str) -> Option<String> {
        let mut cache = self.lock_text();
        let text = cache.by_hash.remove(hash)?;
        cache.bytes = cache.bytes.saturating_sub(text.len());
        Some(text)
    }
}

impl ShellBackend {
    fn index_handle(&self, what: &'static str) -> Result<Arc<ShellIndex>, BackendError> {
        self.executor_for(what)?;
        self.index.clone().ok_or_else(|| {
            BackendError::Unavailable(format!("{what}: no index storage is configured"))
        })
    }

    pub(in crate::backend) async fn scan_codebase_shell(
        &self,
        repo_path: String,
    ) -> Result<ScanCodebaseResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::CodebaseChunks);
        let index = self.index_handle("scan_codebase")?;
        let executor = self.executor_for("scan_codebase")?;
        let pwsh = executor.shell_kind().is_powershell();
        let tool = if pwsh {
            None
        } else {
            match self.base64_tool().await? {
                Some(tool) => Some(tool),
                None => {
                    return Ok(scan_error(
                        "The host has no base64, openssl or od, so files cannot be read in shell mode.",
                    ));
                }
            }
        };
        let Some(store) = self.host_store(&index, &repo_path).await? else {
            return Ok(scan_error(format!(
                "{repo_path} is not a directory on the host"
            )));
        };

        // List and hash one page at a time: page 0 also reports how many files there are.
        let mut listed = Vec::new();
        let mut page = 0usize;
        let mut total: Option<usize> = None;
        loop {
            let command = if pwsh {
                scan_page_command_pwsh(&repo_path, page, SCAN_PAGE_FILES, page == 0)
            } else {
                scan_page_command(&repo_path, page, SCAN_PAGE_FILES, page == 0)
            };
            let script =
                Script::new(Self::new_nonce())?.section("scan", command, StderrMode::Discard)?;
            let frame = self.run_script(&script).await?;
            let Some(section) = frame.section("scan") else {
                return Ok(scan_error("the host returned no listing"));
            };
            match section.exit {
                0 => {}
                EXIT_NO_DIRECTORY => {
                    return Ok(scan_error(format!(
                        "{repo_path} is not a directory on the host"
                    )));
                }
                EXIT_NO_HASH_TOOL => {
                    return Ok(scan_error(
                        "the host has no sha256sum, shasum or sha256, so files cannot be hashed",
                    ));
                }
                n => return Ok(scan_error(format!("listing the files exited with {n}"))),
            }
            let text = section.text();
            if page == 0 {
                total = parse_total(&text);
            }
            listed.extend(parse_scan_output(&text).1);
            page += 1;
            // No total (an unreadable first page) is one page, as before paging existed.
            if total.is_none_or(|t| page * SCAN_PAGE_FILES >= t) {
                break;
            }
        }
        listed.sort_by(|a, b| a.path.cmp(&b.path));
        listed.dedup_by(|a, b| a.path == b.path);

        index.clear_text();
        let previous = self.load_host_scan(&index, &store).await?;
        // The borrowed lookup tables live in their own block so that nothing borrowed is held
        // across the `.await` below (a borrowed-key map in the future trips rustc's
        // higher-ranked `Send` check once the future is boxed as `BoxFuture`).
        let (mut manifest, mut chunks, mut skipped, to_read) = {
            // Whatever an earlier scan of this repository kept, by (path, hash).
            let (known_files, known_skipped, known_chunks) =
                match previous.as_ref().filter(|s| s.repo_path == repo_path) {
                    Some(p) => {
                        let mut by_path: HashMap<&str, Vec<&ChunkEntry>> = HashMap::new();
                        for c in &p.scan.chunks {
                            by_path.entry(c.path.as_str()).or_default().push(c);
                        }
                        (
                            p.scan
                                .manifest
                                .iter()
                                .map(|m| ((m.path.as_str(), m.file_hash.as_str()), ()))
                                .collect::<HashMap<_, _>>(),
                            p.skipped
                                .iter()
                                .map(|m| ((m.path.as_str(), m.file_hash.as_str()), ()))
                                .collect::<HashMap<_, _>>(),
                            by_path,
                        )
                    }
                    None => Default::default(),
                };

            let mut manifest: Vec<ManifestEntry> = Vec::new();
            let mut chunks: Vec<ChunkEntry> = Vec::new();
            let mut skipped: Vec<ManifestEntry> = Vec::new();
            let mut to_read: Vec<String> = Vec::new();
            for file in &listed {
                let key = (file.path.as_str(), file.sha256.as_str());
                if known_files.contains_key(&key) {
                    manifest.push(ManifestEntry {
                        path: file.path.clone(),
                        file_hash: file.sha256.clone(),
                    });
                    chunks.extend(
                        known_chunks
                            .get(file.path.as_str())
                            .into_iter()
                            .flatten()
                            .map(|c| (*c).clone()),
                    );
                } else if known_skipped.contains_key(&key) {
                    skipped.push(ManifestEntry {
                        path: file.path.clone(),
                        file_hash: file.sha256.clone(),
                    });
                } else {
                    to_read.push(file.path.clone());
                }
            }
            (manifest, chunks, skipped, to_read)
        };

        let contents = self.read_files(&repo_path, tool, &to_read).await?;
        for (path, bytes) in to_read.iter().zip(contents) {
            // Gone or unreadable since the listing: not part of this scan.
            let Some(bytes) = bytes else { continue };
            let file_hash = sha256_hex(&bytes);
            let text = if bytes.len() as u64 >= MAX_FILE_BYTES || looks_binary(&bytes) {
                None
            } else {
                std::str::from_utf8(&bytes).ok()
            };
            let file_chunks = text.map(|t| chunk_file(path, t)).unwrap_or_default();
            if file_chunks.is_empty() {
                skipped.push(ManifestEntry {
                    path: path.clone(),
                    file_hash,
                });
                continue;
            }
            let text = text.expect("chunks imply text");
            for c in &file_chunks {
                index.stash(&c.content_hash, &text[c.byte_start..c.byte_end]);
            }
            manifest.push(ManifestEntry {
                path: path.clone(),
                file_hash,
            });
            chunks.extend(file_chunks);
        }

        manifest.sort_by(|a, b| a.path.cmp(&b.path));
        chunks.sort_by(|a, b| a.path.cmp(&b.path).then(a.byte_start.cmp(&b.byte_start)));
        let root_hash = root_hash(&manifest);
        let scan = Scan {
            manifest,
            chunks,
            root_hash,
        };
        // An unchanged scan writes nothing: the host's `meta.json` already says it.
        let unchanged = previous
            .as_ref()
            .is_some_and(|p| p.repo_path == repo_path && p.scan == scan && p.skipped == skipped);
        if !unchanged {
            if let Err(e) = store
                .local
                .save_scan_and_skipped(&repo_path, &scan, &skipped)
            {
                return Ok(scan_error(format!("{e:#}")));
            }
            let bytes = match std::fs::read(store.local.dir().join(IndexStore::META_FILE)) {
                Ok(bytes) => bytes,
                Err(e) => return Ok(scan_error(format!("{e}"))),
            };
            if let Err(e) = self
                .put_host_file(&index, &store, IndexStore::META_FILE, bytes)
                .await
            {
                // The mirror must not claim a table the host never got.
                self.forget_mirror(&index, &store, IndexStore::META_FILE);
                return Ok(scan_error(format!("{e}")));
            }
        }
        let live: HashSet<String> = scan.chunks.iter().map(|c| c.content_hash.clone()).collect();
        if let Err(e) = self.compact_host_vectors(&index, &store, &live).await {
            log::warn!("index compaction failed: {e}");
        }
        log::info!(
            "ScanCodebase (shell): {} files, {} fragments, {} read",
            scan.manifest.len(),
            scan.chunks.len(),
            to_read.len()
        );
        Ok(ScanCodebaseResponse {
            result: Some(scan_codebase_response::Result::Success(scan_to_proto(
                &scan,
            ))),
        })
    }

    pub(in crate::backend) async fn get_chunks_shell(
        &self,
        request: GetChunks,
    ) -> Result<GetChunksResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::CodebaseChunks);
        let index = self.index_handle("get_chunks")?;
        if request.content_hashes.len() > MAX_CHUNKS_PER_REQUEST {
            return Ok(chunks_error(format!(
                "{} hashes in one request; at most {MAX_CHUNKS_PER_REQUEST}",
                request.content_hashes.len()
            )));
        }
        let Some(store) = self.host_store(&index, &request.repo_path).await? else {
            return Ok(chunks_error(format!(
                "{} is not a directory on the host",
                request.repo_path
            )));
        };
        let Some(stored) = self.load_host_scan(&index, &store).await? else {
            return Ok(chunks_error("this repository has not been scanned"));
        };
        let known = if request.storage_key.is_empty() {
            HashSet::new()
        } else {
            let file = IndexStore::vector_file_name(&request.storage_key);
            self.sync_host_file(&index, &store, &file).await?;
            match store.local.known_hashes(&request.storage_key) {
                Ok(known) => known,
                Err(e) => return Ok(chunks_error(format!("{e:#}"))),
            }
        };
        let mut by_hash: HashMap<&str, &ChunkEntry> = HashMap::new();
        for c in &stored.scan.chunks {
            by_hash.entry(c.content_hash.as_str()).or_insert(c);
        }

        let mut already = 0u64;
        let mut chunks = Vec::new();
        let mut missing = Vec::new();
        let mut reread: Vec<(&ChunkEntry, &str)> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for hash in &request.content_hashes {
            if !seen.insert(hash.as_str()) {
                continue;
            }
            if known.contains(hash) {
                already += 1;
            } else if let Some(text) = index.take(hash) {
                chunks.push(CodebaseChunkText {
                    content_hash: hash.clone(),
                    text,
                });
            } else if let Some(entry) = by_hash.get(hash.as_str()) {
                reread.push((entry, hash));
            } else {
                missing.push(hash.clone());
            }
        }

        if !reread.is_empty() {
            let tool = if self.shell_kind().is_powershell() {
                None
            } else {
                match self.base64_tool().await? {
                    Some(tool) => Some(tool),
                    None => return Ok(chunks_error("The host has no base64, openssl or od.")),
                }
            };
            let mut paths: Vec<String> = reread.iter().map(|(e, _)| e.path.clone()).collect();
            paths.sort();
            paths.dedup();
            let contents = self.read_files(&request.repo_path, tool, &paths).await?;
            let by_path: HashMap<&str, Option<Vec<u8>>> =
                paths.iter().map(String::as_str).zip(contents).collect();
            for (entry, hash) in reread {
                let text = by_path
                    .get(entry.path.as_str())
                    .and_then(|b| b.as_deref())
                    .and_then(|bytes| bytes.get(entry.byte_start..entry.byte_end))
                    .filter(|slice| sha256_hex(slice) == *hash)
                    .and_then(|slice| std::str::from_utf8(slice).ok());
                match text {
                    Some(text) => chunks.push(CodebaseChunkText {
                        content_hash: hash.to_string(),
                        text: text.to_string(),
                    }),
                    None => missing.push(hash.to_string()),
                }
            }
        }
        Ok(GetChunksResponse {
            result: Some(get_chunks_response::Result::Success(GetChunksSuccess {
                chunks,
                already_embedded: already,
                missing_hashes: missing,
            })),
        })
    }

    pub(in crate::backend) async fn store_vectors_shell(
        &self,
        request: StoreVectors,
    ) -> Result<StoreVectorsResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::CodebaseChunks);
        let index = self.index_handle("store_vectors")?;
        if request.storage_key.is_empty() {
            return Ok(store_error("storage_key is empty"));
        }
        let Some(store) = self.host_store(&index, &request.repo_path).await? else {
            return Ok(store_error(format!(
                "{} is not a directory on the host",
                request.repo_path
            )));
        };
        let vectors: Vec<(String, Vec<f32>)> = request
            .vectors
            .into_iter()
            .map(|v| (v.content_hash, v.values))
            .collect();
        let (dims, records) = match IndexStore::encode_vectors(&vectors) {
            Ok(encoded) => encoded,
            Err(e) => return Ok(store_error(format!("{e:#}"))),
        };
        let stored = vectors.len();
        if stored > 0 {
            if let Err(e) = self
                .append_vectors_on_host(
                    &index,
                    &store,
                    &request.storage_key,
                    dims,
                    records,
                    &vectors,
                )
                .await
            {
                return Ok(store_error(format!("{e}")));
            }
        }
        // A changed embedding model makes every older space unusable.
        let keep = IndexStore::vector_file_name(&request.storage_key);
        if let Err(e) = self.drop_other_host_vectors(&store, &keep).await {
            log::warn!("dropping the host's old vector files failed: {e}");
        }
        Ok(StoreVectorsResponse {
            result: Some(crate::proto::store_vectors_response::Result::Success(
                crate::proto::StoreVectorsSuccess {
                    stored: stored as u64,
                },
            )),
        })
    }

    /// Adds `records` (already encoded: `vectors` is the same data, for the mirror) to the
    /// host's vector file for `storage_key`: appended when the file is ours and of this
    /// dimension, otherwise the file is started over, like the helper's store does.
    async fn append_vectors_on_host(
        &self,
        index: &Arc<ShellIndex>,
        store: &super::host_store::HostStore,
        storage_key: &str,
        dims: usize,
        records: Vec<u8>,
        vectors: &[(String, Vec<f32>)],
    ) -> Result<(), BackendError> {
        let name = IndexStore::vector_file_name(storage_key);
        let (in_sync, stat) = self.mirror_is_current(index, store, &name).await?;
        let sep = if store.dir.contains('\\') && !store.dir.contains('/') {
            '\\'
        } else {
            '/'
        };
        // Where the whole records end, if the host's file is ours, of this key and dimension.
        let append_at = match stat {
            Some(stat) => {
                let head = self
                    .read_host_head(
                        &format!("{}{sep}{name}", store.dir),
                        IndexStore::MAX_HEADER_BYTES.min(stat.size),
                    )
                    .await?;
                match IndexStore::parse_vector_header(&head) {
                    Some((key, file_dims, header_len, record_size))
                        if key == storage_key && file_dims == dims =>
                    {
                        let body = stat.size.saturating_sub(header_len);
                        Some(header_len + body / record_size * record_size)
                    }
                    _ => None,
                }
            }
            None => None,
        };
        if let Some(whole) = append_at {
            if self
                .append_host_file(store, &name, whole, records.clone())
                .await?
            {
                if in_sync {
                    // The mirror matched the host before this append: apply the same records
                    // to it instead of downloading the whole file again.
                    if store.local.append_vectors(storage_key, vectors).is_ok() {
                        self.restamp(index, store, &name).await?;
                        return Ok(());
                    }
                }
                self.forget_mirror(index, store, &name);
                return Ok(());
            }
        }
        // New, other key or dimension, torn beyond repair, or changed under us: start over.
        let mut bytes = IndexStore::vector_header(storage_key, dims);
        bytes.extend_from_slice(&records);
        self.put_host_file(index, store, &name, bytes).await
    }

    pub(in crate::backend) async fn query_vectors_shell(
        &self,
        request: QueryVectors,
    ) -> Result<QueryVectorsResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::CodebaseChunks);
        let index = self.index_handle("query_vectors")?;
        let Some(store) = self.host_store(&index, &request.repo_path).await? else {
            return Ok(query_error(format!(
                "{} is not a directory on the host",
                request.repo_path
            )));
        };
        // Brute force over the mirror of the host's files, refreshed when the host's differ.
        self.sync_host_file(&index, &store, IndexStore::META_FILE)
            .await?;
        if !request.storage_key.is_empty() {
            let file = IndexStore::vector_file_name(&request.storage_key);
            self.sync_host_file(&index, &store, &file).await?;
        }
        Ok(crate::index::ops::query_vectors(&store.local, request))
    }

    /// The bytes of each of `paths` (relative to `repo`), in order; `None` for a file that could
    /// not be read. Batches go over the out-of-band channel; one that exceeds the output cap is
    /// split in half and retried.
    async fn read_files(
        &self,
        repo: &str,
        tool: Option<Base64Tool>,
        paths: &[String],
    ) -> Result<Vec<Option<Vec<u8>>>, BackendError> {
        let mut out: Vec<Option<Vec<u8>>> = Vec::with_capacity(paths.len());
        let mut batch = READ_BATCH_FILES;
        let mut at = 0;
        while at < paths.len() {
            let n = batch.min(paths.len() - at);
            match self.read_batch(repo, tool, &paths[at..at + n]).await? {
                Some(files) => {
                    out.extend(files);
                    at += n;
                }
                None if n == 1 => {
                    out.push(None);
                    at += 1;
                }
                None => batch = n.div_ceil(2),
            }
        }
        Ok(out)
    }

    /// One command reading `paths`; `None` when its output hit the cap.
    async fn read_batch(
        &self,
        repo: &str,
        tool: Option<Base64Tool>,
        paths: &[String],
    ) -> Result<Option<Vec<Option<Vec<u8>>>>, BackendError> {
        let executor = self.executor_for("index file read")?;
        let mut script = Script::new(Self::new_nonce())?;
        for (i, path) in paths.iter().enumerate() {
            let command = match tool {
                Some(tool) => read_file_command(tool, repo, path),
                None => read_file_command_pwsh(repo, path),
            };
            script = script.section(format!("f{i}"), command, StderrMode::Discard)?;
        }
        let started = std::time::Instant::now();
        let result = executor
            .execute(ExecRequest {
                command: script.render(self.login_kind()),
                stdin: Vec::new(),
                max_output_bytes: MAX_EXEC_OUTPUT_BYTES,
            })
            .await;
        // One line per exec channel (#2156).
        log::debug!(
            "shell exec: producer=index_read files={} elapsed_ms={} ok={}",
            paths.len(),
            started.elapsed().as_millis(),
            result.is_ok()
        );
        let output = result?;
        if output.truncated {
            return Ok(None);
        }
        let frame = Frame::parse(&output.stdout, script.nonce())?;
        Ok(Some(
            (0..paths.len())
                .map(|i| {
                    let section = frame.section(&format!("f{i}"))?;
                    if section.exit != 0 {
                        return None;
                    }
                    match tool {
                        Some(tool) => tool.decode(&section.output).ok(),
                        None => crate::shellcmd::base64::decode(&section.output).ok(),
                    }
                })
                .collect(),
        ))
    }
}
