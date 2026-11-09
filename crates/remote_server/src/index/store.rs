//! The flat-file index store: where one repository's scan and embedding vectors live between
//! syncs. No sqlite, no network, std + serde_json + sha2 only.
//!
//! The helper keeps one under `~/.phosphor/index/` on the host; the no-helper path keeps one
//! app-side under the app's data directory, keyed by host and repository. Layout of one
//! repository's directory (named by a hash of its key):
//!
//! * `meta.json`: `{format, repo_path, scan}` -- the last [`Scan`] (manifest, fragment spans,
//!   root hash). Written to a temporary file and renamed, so a reader sees the old file or the
//!   new one, never a torn one.
//! * `vectors-<hash of storage key>.bin`: the vectors for ONE embedding storage key (a different
//!   model or dimension is a different space, so a different file; a storage-key change
//!   therefore re-embeds everything). Header `PHVEC001`, the key (u32 length + bytes), the
//!   dimension (u32); then fixed-size records: the 32 raw bytes of the content hash and `dims`
//!   little-endian f32. Appended with one write per batch; a torn tail (a crash mid-append) is
//!   ignored on read and cut off by the next append. A repeated hash: the last record wins.
//!
//! Search is brute force: one pass over the file computing cosine similarity, no index structure.
//! That is the owner's ruling for this feature; measure before changing it.
//!
//! Format version [`FORMAT`]: a `meta.json` or vector file of another version reads as absent
//! (the caller rescans / re-embeds), never as an error.

use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, anyhow, bail};
use serde::{Deserialize, Serialize};

use super::types::{ChunkEntry, ManifestEntry, Scan, hex, sha256_hex};

/// Version of `meta.json` and of the vector file header.
pub const FORMAT: u32 = 1;
const MAGIC: &[u8; 8] = b"PHVEC001";
const HASH_BYTES: usize = 32;

/// Serialises writers within the process (the helper handles requests on several threads).
static WRITE_LOCK: Mutex<()> = Mutex::new(());
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// What `meta.json` holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredScan {
    pub format: u32,
    pub repo_path: String,
    pub scan: Scan,
    /// Files the shell arm read and found not indexable (binary, no fragments), by hash, so the
    /// next scan does not read them again. Always empty for the helper, which walks its own tree.
    #[serde(default)]
    pub skipped: Vec<ManifestEntry>,
}

/// One search hit: a fragment and its cosine similarity to the query.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedChunk {
    pub content_hash: String,
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub score: f32,
}

/// One repository's directory.
#[derive(Debug, Clone)]
pub struct IndexStore {
    dir: PathBuf,
}

fn decode_hash(hash: &str) -> Option<[u8; HASH_BYTES]> {
    if hash.len() != HASH_BYTES * 2 || !hash.is_ascii() {
        return None;
    }
    let mut out = [0u8; HASH_BYTES];
    for (i, byte) in out.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hash[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(out)
}

/// Cosine similarity; 0.0 when either vector is all zeros or the lengths differ.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0f32, 0f32, 0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

struct VectorHeader {
    dims: usize,
    /// Bytes before the first record.
    len: u64,
}

impl VectorHeader {
    fn record_size(&self) -> u64 {
        (HASH_BYTES + self.dims * 4) as u64
    }
}

impl IndexStore {
    /// The store for `key` (any stable string naming host and repository) under `root`.
    pub fn open(root: &Path, key: &str) -> Self {
        Self {
            dir: root.join(&sha256_hex(key.as_bytes())[..32]),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn meta_path(&self) -> PathBuf {
        self.dir.join("meta.json")
    }

    fn vector_path(&self, storage_key: &str) -> PathBuf {
        self.dir.join(format!(
            "vectors-{}.bin",
            &sha256_hex(storage_key.as_bytes())[..16]
        ))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
        fs::create_dir_all(&self.dir)
            .with_context(|| format!("creating {}", self.dir.display()))?;
        let tmp = self.dir.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| -> std::io::Result<()> {
            let mut f = File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
            fs::rename(&tmp, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.with_context(|| format!("writing {}", path.display()))
    }

    /// Stores `scan` as the repository's current manifest and fragment table.
    pub fn save_scan(&self, repo_path: &str, scan: &Scan) -> anyhow::Result<()> {
        self.save_scan_and_skipped(repo_path, scan, &[])
    }

    /// [`Self::save_scan`] plus the files that were read and not indexed.
    pub fn save_scan_and_skipped(
        &self,
        repo_path: &str,
        scan: &Scan,
        skipped: &[ManifestEntry],
    ) -> anyhow::Result<()> {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let stored = StoredScan {
            format: FORMAT,
            repo_path: repo_path.to_string(),
            scan: scan.clone(),
            skipped: skipped.to_vec(),
        };
        self.write_atomic(&self.meta_path(), &serde_json::to_vec(&stored)?)
    }

    /// The stored scan, or `None` when there is none, it is unreadable, or of another format.
    pub fn load_scan(&self) -> Option<StoredScan> {
        let bytes = fs::read(self.meta_path()).ok()?;
        let stored: StoredScan = serde_json::from_slice(&bytes).ok()?;
        (stored.format == FORMAT).then_some(stored)
    }

    /// The storage key and layout a vector file declares, or `None` if it is not one of ours.
    fn read_any_header(file: &mut impl Read) -> Option<(String, VectorHeader)> {
        let mut magic = [0u8; 8];
        file.read_exact(&mut magic).ok()?;
        if &magic != MAGIC {
            return None;
        }
        let mut word = [0u8; 4];
        file.read_exact(&mut word).ok()?;
        let key_len = u32::from_le_bytes(word) as usize;
        if key_len > 4096 {
            return None;
        }
        let mut key = vec![0u8; key_len];
        file.read_exact(&mut key).ok()?;
        let key = String::from_utf8(key).ok()?;
        file.read_exact(&mut word).ok()?;
        let dims = u32::from_le_bytes(word) as usize;
        (dims > 0).then_some((
            key,
            VectorHeader {
                dims,
                len: (8 + 4 + key_len + 4) as u64,
            },
        ))
    }

    fn read_header(file: &mut impl Read, storage_key: &str) -> Option<VectorHeader> {
        let (key, header) = Self::read_any_header(file)?;
        (key == storage_key).then_some(header)
    }

    /// Appends `vectors` (content hash, vector) under `storage_key`. All vectors must have the
    /// dimension the file already has (or, for a new file, the first vector's). A different
    /// storage key's file is left alone here; [`Self::drop_other_spaces`] removes those.
    pub fn append_vectors(
        &self,
        storage_key: &str,
        vectors: &[(String, Vec<f32>)],
    ) -> anyhow::Result<usize> {
        let Some(first) = vectors.first() else {
            return Ok(0);
        };
        let dims = first.1.len();
        if dims == 0 {
            bail!("empty vector for {}", first.0);
        }
        let mut records = Vec::with_capacity(vectors.len() * (HASH_BYTES + dims * 4));
        for (hash, vector) in vectors {
            if vector.len() != dims {
                bail!(
                    "vector for {hash} has {} dimensions, expected {dims}",
                    vector.len()
                );
            }
            if vector.iter().any(|x| !x.is_finite()) {
                bail!("vector for {hash} has a non-finite value");
            }
            let raw = decode_hash(hash).ok_or_else(|| anyhow!("not a content hash: {hash:?}"))?;
            records.extend_from_slice(&raw);
            for x in vector {
                records.extend_from_slice(&x.to_le_bytes());
            }
        }

        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        fs::create_dir_all(&self.dir)?;
        let path = self.vector_path(storage_key);
        let existing = File::open(&path)
            .ok()
            .and_then(|mut f| Self::read_header(&mut f, storage_key));
        match existing {
            Some(header) if header.dims == dims => {
                let mut file = OpenOptions::new().write(true).open(&path)?;
                let len = file.metadata()?.len();
                let body = len.saturating_sub(header.len);
                // Cut a torn tail so the new records start on a record boundary.
                let whole = header.len + body / header.record_size() * header.record_size();
                if whole != len {
                    file.set_len(whole)?;
                }
                file.seek(SeekFrom::Start(whole))?;
                file.write_all(&records)?;
                file.sync_all()?;
            }
            // New, unreadable, other format or other dimension: start the file over.
            _ => {
                let mut bytes = Vec::with_capacity(20 + storage_key.len() + records.len());
                bytes.extend_from_slice(MAGIC);
                bytes.extend_from_slice(&(storage_key.len() as u32).to_le_bytes());
                bytes.extend_from_slice(storage_key.as_bytes());
                bytes.extend_from_slice(&(dims as u32).to_le_bytes());
                bytes.extend_from_slice(&records);
                self.write_atomic(&path, &bytes)?;
            }
        }
        Ok(vectors.len())
    }

    /// Removes the vector files of every storage key but `keep`: a changed embedding model makes
    /// the old vectors unusable, and they would otherwise stay on the host for ever.
    pub fn drop_other_spaces(&self, keep: &str) {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let keep_path = self.vector_path(keep);
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_vectors = path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("vectors-") && n.ends_with(".bin"));
            if is_vectors && path != keep_path {
                let _ = fs::remove_file(path);
            }
        }
    }

    /// Calls `visit(hash_bytes, vector_bytes)` for every whole record of `storage_key`'s file, in
    /// file order. Returns the file's dimension, or `None` when there is no usable file for the
    /// key.
    fn for_each_record(
        &self,
        storage_key: &str,
        mut visit: impl FnMut(&[u8], &[u8]),
    ) -> anyhow::Result<Option<usize>> {
        let Ok(file) = File::open(self.vector_path(storage_key)) else {
            return Ok(None);
        };
        let mut reader = BufReader::with_capacity(1 << 20, file);
        let Some(header) = Self::read_header(&mut reader, storage_key) else {
            return Ok(None);
        };
        let mut record = vec![0u8; header.record_size() as usize];
        loop {
            match reader.read_exact(&mut record) {
                Ok(()) => visit(&record[..HASH_BYTES], &record[HASH_BYTES..]),
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(Some(header.dims))
    }

    /// The content hashes that have a vector under `storage_key`.
    pub fn known_hashes(&self, storage_key: &str) -> anyhow::Result<HashSet<String>> {
        let mut out = HashSet::new();
        self.for_each_record(storage_key, |hash, _| {
            out.insert(hex(hash));
        })?;
        Ok(out)
    }

    /// Brute-force search: the `top_k` fragments of the stored scan whose vectors are most
    /// similar to `query`, best first (ties by path, then position, so the order is
    /// deterministic). Fragments with no vector are not candidates. The same content in several
    /// places yields one hit per place.
    pub fn query(
        &self,
        storage_key: &str,
        query: &[f32],
        top_k: usize,
    ) -> anyhow::Result<Vec<RankedChunk>> {
        let stored = self
            .load_scan()
            .ok_or_else(|| anyhow!("no scan is stored for this repository"))?;
        let mut scores: HashMap<[u8; HASH_BYTES], f32> = HashMap::new();
        let found = self.for_each_record(storage_key, |hash, vector| {
            // Decode in place; a later record for the same hash replaces an earlier one.
            let decoded: Vec<f32> = vector
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            if decoded.len() == query.len() {
                let mut key = [0u8; HASH_BYTES];
                key.copy_from_slice(hash);
                scores.insert(key, cosine(query, &decoded));
            }
        })?;
        match found {
            None => bail!("no vectors are stored for this embedding model"),
            Some(dims) if dims != query.len() => {
                bail!(
                    "the query has {} dimensions but the stored vectors have {dims}",
                    query.len()
                )
            }
            _ => {}
        }
        let mut hits: Vec<(f32, &ChunkEntry)> = stored
            .scan
            .chunks
            .iter()
            .filter_map(|c| {
                let score = *scores.get(&decode_hash(&c.content_hash)?)?;
                Some((score, c))
            })
            .collect();
        hits.sort_by(|(sa, a), (sb, b)| {
            sb.total_cmp(sa)
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.start_line.cmp(&b.start_line))
        });
        hits.truncate(top_k);
        Ok(hits
            .into_iter()
            .map(|(score, c)| RankedChunk {
                content_hash: c.content_hash.clone(),
                path: c.path.clone(),
                start_line: c.start_line,
                end_line: c.end_line,
                score,
            })
            .collect())
    }

    /// Rewrites each vector file that holds much more than `live` needs (a deleted or edited file
    /// leaves dead records behind), keeping only the live hashes. Cheap when nothing is wasted:
    /// a size check per file.
    pub fn compact_wasteful(&self, live: &HashSet<String>) -> anyhow::Result<()> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !(name.starts_with("vectors-") && name.ends_with(".bin")) {
                continue;
            }
            self.compact_file(&path, live)?;
        }
        Ok(())
    }

    fn compact_file(&self, path: &Path, live: &HashSet<String>) -> anyhow::Result<()> {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let mut file = File::open(path)?;
        let len = file.metadata()?.len();
        let Some((storage_key, header)) = Self::read_any_header(&mut file) else {
            return Ok(());
        };
        let records = len.saturating_sub(header.len) / header.record_size();
        if records <= (live.len() as u64) * 3 / 2 + 64 {
            return Ok(());
        }
        // Keep the last record of each live hash.
        let mut kept: HashMap<[u8; HASH_BYTES], Vec<u8>> = HashMap::new();
        let mut reader = BufReader::with_capacity(1 << 20, file);
        let mut record = vec![0u8; header.record_size() as usize];
        while reader.read_exact(&mut record).is_ok() {
            let mut key = [0u8; HASH_BYTES];
            key.copy_from_slice(&record[..HASH_BYTES]);
            if live.contains(&hex(&key)) {
                kept.insert(key, record[HASH_BYTES..].to_vec());
            }
        }
        let mut keys: Vec<_> = kept.keys().copied().collect();
        keys.sort();
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&(storage_key.len() as u32).to_le_bytes());
        bytes.extend_from_slice(storage_key.as_bytes());
        bytes.extend_from_slice(&(header.dims as u32).to_le_bytes());
        for key in keys {
            bytes.extend_from_slice(&key);
            bytes.extend_from_slice(&kept[&key]);
        }
        self.write_atomic(path, &bytes)
    }

    /// Deletes everything stored for the repository.
    pub fn drop_all(&self) {
        let _guard = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
