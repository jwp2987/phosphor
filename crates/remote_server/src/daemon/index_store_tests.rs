use std::fs;
use std::path::Path;

use super::*;
use crate::proto::{
    CodebaseVector, StoreVectorsSuccess, query_vectors_response, store_vectors_response,
};

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

struct Env {
    repo: tempfile::TempDir,
    root: tempfile::TempDir,
}

fn env() -> Env {
    let repo = tempfile::tempdir().unwrap();
    write(repo.path(), "a.txt", "alpha\nbeta\n");
    write(repo.path(), "src/b.rs", "fn b() {}\n");
    Env {
        repo,
        root: tempfile::tempdir().unwrap(),
    }
}

impl Env {
    fn repo_path(&self) -> String {
        self.repo.path().to_string_lossy().into_owned()
    }

    fn scan(&self) -> crate::proto::ScanCodebaseSuccess {
        match scan_codebase(
            self.root.path(),
            ScanCodebase {
                repo_path: self.repo_path(),
            },
        )
        .result
        {
            Some(scan_codebase_response::Result::Success(s)) => s,
            other => panic!("scan failed: {other:?}"),
        }
    }

    fn chunks(&self, key: &str, hashes: Vec<String>) -> GetChunksSuccess {
        match get_chunks(
            self.root.path(),
            GetChunks {
                repo_path: self.repo_path(),
                storage_key: key.into(),
                content_hashes: hashes,
            },
        )
        .result
        {
            Some(get_chunks_response::Result::Success(s)) => s,
            other => panic!("get_chunks failed: {other:?}"),
        }
    }

    fn store(&self, key: &str, vectors: Vec<(String, Vec<f32>)>) -> StoreVectorsResponse {
        store_vectors(
            self.root.path(),
            StoreVectors {
                repo_path: self.repo_path(),
                storage_key: key.into(),
                vectors: vectors
                    .into_iter()
                    .map(|(content_hash, values)| CodebaseVector {
                        content_hash,
                        values,
                    })
                    .collect(),
            },
        )
    }

    fn query(&self, key: &str, q: Vec<f32>, k: u32) -> QueryVectorsResponse {
        query_vectors(
            self.root.path(),
            QueryVectors {
                repo_path: self.repo_path(),
                storage_key: key.into(),
                query_vector: q,
                top_k: k,
            },
        )
    }
}

fn hashes(s: &crate::proto::ScanCodebaseSuccess) -> Vec<String> {
    s.chunks.iter().map(|c| c.content_hash.clone()).collect()
}

#[test]
fn scan_reports_the_manifest_and_fragments_and_is_stable() {
    let e = env();
    let one = e.scan();
    assert_eq!(
        one.manifest
            .iter()
            .map(|m| m.path.as_str())
            .collect::<Vec<_>>(),
        ["a.txt", "src/b.rs"]
    );
    assert_eq!(one.chunks.len(), 2);
    assert_eq!(one.root_hash.len(), 64);
    assert_eq!(one, e.scan());
}

#[test]
fn get_chunks_returns_the_text_of_fragments_without_a_vector() {
    let e = env();
    let scan = e.scan();
    let got = e.chunks("m:2", hashes(&scan));
    assert_eq!(got.already_embedded, 0);
    assert!(got.missing_hashes.is_empty());
    let mut texts: Vec<&str> = got.chunks.iter().map(|c| c.text.as_str()).collect();
    texts.sort();
    assert_eq!(texts, ["alpha\nbeta", "fn b() {}"]);
}

#[test]
fn stored_vectors_are_not_sent_again_and_a_new_storage_key_asks_for_everything() {
    let e = env();
    let scan = e.scan();
    let hs = hashes(&scan);
    let stored = e.store("m:2", vec![(hs[0].clone(), vec![1.0, 0.0])]);
    assert!(matches!(
        stored.result,
        Some(store_vectors_response::Result::Success(
            StoreVectorsSuccess { stored: 1 }
        ))
    ));
    let got = e.chunks("m:2", hs.clone());
    assert_eq!(got.already_embedded, 1);
    assert_eq!(got.chunks.len(), 1);
    assert_eq!(got.chunks[0].content_hash, hs[1]);
    // Another model: nothing is embedded in its space.
    let other = e.chunks("other:3", hs);
    assert_eq!(other.already_embedded, 0);
    assert_eq!(other.chunks.len(), 2);
}

#[test]
fn only_the_changed_file_needs_new_text_after_an_edit() {
    let e = env();
    let before = e.scan();
    let vectors: Vec<_> = hashes(&before)
        .into_iter()
        .map(|h| (h, vec![1.0, 0.0]))
        .collect();
    e.store("m:2", vectors);
    write(e.repo.path(), "a.txt", "alpha\nbeta\ngamma\n");
    let after = e.scan();
    assert_ne!(before.root_hash, after.root_hash);
    let got = e.chunks("m:2", hashes(&after));
    assert_eq!(got.already_embedded, 1);
    assert_eq!(got.chunks.len(), 1);
    assert_eq!(got.chunks[0].text, "alpha\nbeta\ngamma");
}

#[test]
fn a_file_edited_after_the_scan_is_reported_missing_not_served_stale() {
    let e = env();
    let scan = e.scan();
    write(e.repo.path(), "a.txt", "something else entirely\n");
    let got = e.chunks("m:2", hashes(&scan));
    assert_eq!(got.chunks.len(), 1);
    assert_eq!(got.chunks[0].text, "fn b() {}");
    assert_eq!(got.missing_hashes.len(), 1);
}

#[test]
fn an_unknown_hash_is_missing_and_a_duplicate_request_is_answered_once() {
    let e = env();
    let scan = e.scan();
    let h = hashes(&scan)[0].clone();
    let got = e.chunks("m:2", vec![h.clone(), h, "0".repeat(64)]);
    assert_eq!(got.chunks.len(), 1);
    assert_eq!(got.missing_hashes, vec!["0".repeat(64)]);
}

#[test]
fn get_chunks_before_any_scan_and_oversized_requests_are_errors() {
    let e = env();
    assert!(matches!(
        get_chunks(
            e.root.path(),
            GetChunks {
                repo_path: e.repo_path(),
                storage_key: "m:2".into(),
                content_hashes: vec![]
            }
        )
        .result,
        Some(get_chunks_response::Result::Error(_))
    ));
    e.scan();
    let many: Vec<String> = (0..MAX_CHUNKS_PER_REQUEST + 1)
        .map(|i| format!("{i:064}"))
        .collect();
    assert!(matches!(
        get_chunks(
            e.root.path(),
            GetChunks {
                repo_path: e.repo_path(),
                storage_key: "m:2".into(),
                content_hashes: many
            }
        )
        .result,
        Some(get_chunks_response::Result::Error(_))
    ));
}

#[test]
fn query_ranks_by_cosine_over_the_stored_vectors() {
    let e = env();
    let scan = e.scan();
    let by_path = |p: &str| {
        scan.chunks
            .iter()
            .find(|c| c.path == p)
            .unwrap()
            .content_hash
            .clone()
    };
    e.store(
        "m:2",
        vec![
            (by_path("a.txt"), vec![1.0, 0.0]),
            (by_path("src/b.rs"), vec![0.0, 1.0]),
        ],
    );
    let hits = match e.query("m:2", vec![0.1, 1.0], 5).result {
        Some(query_vectors_response::Result::Success(s)) => s.fragments,
        other => panic!("{other:?}"),
    };
    assert_eq!(hits[0].path, "src/b.rs");
    assert_eq!(hits[1].path, "a.txt");
    assert!(hits[0].score > hits[1].score);
    assert_eq!((hits[1].start_line, hits[1].end_line), (0, 1));
}

#[test]
fn bad_requests_are_errors_not_panics() {
    let e = env();
    e.scan();
    for response in [
        e.store("", vec![])
            .result
            .map(|r| matches!(r, store_vectors_response::Result::Error(_))),
        e.store("m:2", vec![("nothex".into(), vec![1.0])])
            .result
            .map(|r| matches!(r, store_vectors_response::Result::Error(_))),
    ] {
        assert_eq!(response, Some(true));
    }
    assert!(matches!(
        e.query("m:2", vec![], 3).result,
        Some(query_vectors_response::Result::Error(_))
    ));
    // No vectors under this key yet.
    assert!(matches!(
        e.query("m:2", vec![1.0], 3).result,
        Some(query_vectors_response::Result::Error(_))
    ));
    assert!(matches!(
        scan_codebase(
            e.root.path(),
            ScanCodebase {
                repo_path: "relative/path".into()
            }
        )
        .result,
        Some(scan_codebase_response::Result::Error(_))
    ));
}

#[test]
fn the_store_lives_under_the_root_never_in_the_repository() {
    let e = env();
    e.scan();
    assert!(
        fs::read_dir(e.repo.path())
            .unwrap()
            .all(|entry| entry.unwrap().file_name() != "meta.json")
    );
    assert!(fs::read_dir(e.root.path()).unwrap().count() >= 1);
}
