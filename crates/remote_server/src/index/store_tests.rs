use std::collections::HashSet;
use std::fs;

use super::*;
use crate::index::types::{ManifestEntry, chunk_file, root_hash};

fn scan_of(files: &[(&str, &str)]) -> Scan {
    let mut manifest = Vec::new();
    let mut chunks = Vec::new();
    for (path, body) in files {
        manifest.push(ManifestEntry {
            path: path.to_string(),
            file_hash: sha256_hex(body.as_bytes()),
        });
        chunks.extend(chunk_file(path, body));
    }
    let root_hash = root_hash(&manifest);
    Scan {
        manifest,
        chunks,
        root_hash,
    }
}

fn hash_of(body: &str) -> String {
    sha256_hex(body.as_bytes())
}

fn store() -> (tempfile::TempDir, IndexStore) {
    let dir = tempfile::tempdir().unwrap();
    let store = IndexStore::open(dir.path(), "host-a:/repo");
    (dir, store)
}

#[test]
fn a_scan_round_trips_and_a_missing_one_is_none() {
    let (_dir, store) = store();
    assert!(store.load_scan().is_none());
    let scan = scan_of(&[("a.txt", "alpha\nbeta\n"), ("b.txt", "gamma\n")]);
    store.save_scan("/repo", &scan).unwrap();
    let loaded = store.load_scan().unwrap();
    assert_eq!(loaded.scan, scan);
    assert_eq!(loaded.repo_path, "/repo");
}

#[test]
fn another_format_or_garbage_reads_as_absent() {
    let (_dir, store) = store();
    store.save_scan("/repo", &scan_of(&[("a", "x\n")])).unwrap();
    let path = store.dir().join("meta.json");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(&path, text.replace("\"format\":1", "\"format\":99")).unwrap();
    assert!(store.load_scan().is_none());
    fs::write(&path, b"not json").unwrap();
    assert!(store.load_scan().is_none());
}

#[test]
fn different_keys_use_different_directories() {
    let dir = tempfile::tempdir().unwrap();
    let a = IndexStore::open(dir.path(), "host-a:/repo");
    let b = IndexStore::open(dir.path(), "host-b:/repo");
    assert_ne!(a.dir(), b.dir());
    assert_eq!(a.dir(), IndexStore::open(dir.path(), "host-a:/repo").dir());
}

#[test]
fn vectors_round_trip_by_content_hash_and_storage_key() {
    let (_dir, store) = store();
    let (h1, h2) = (hash_of("one"), hash_of("two"));
    assert!(store.known_hashes("m:2").unwrap().is_empty());
    store
        .append_vectors(
            "m:2",
            &[(h1.clone(), vec![1.0, 0.0]), (h2.clone(), vec![0.0, 1.0])],
        )
        .unwrap();
    let known = store.known_hashes("m:2").unwrap();
    assert_eq!(known, HashSet::from([h1.clone(), h2.clone()]));
    // Another model's space is empty: a storage-key change re-embeds everything.
    assert!(store.known_hashes("other:2").unwrap().is_empty());
}

#[test]
fn appending_more_keeps_earlier_vectors_and_a_repeat_replaces() {
    let (_dir, store) = store();
    let (h1, h2) = (hash_of("one"), hash_of("two"));
    store
        .append_vectors("m:2", &[(h1.clone(), vec![1.0, 0.0])])
        .unwrap();
    store
        .append_vectors("m:2", &[(h2.clone(), vec![0.0, 1.0])])
        .unwrap();
    assert_eq!(store.known_hashes("m:2").unwrap().len(), 2);
    // The last record for a hash wins.
    store
        .append_vectors("m:2", &[(h1.clone(), vec![0.0, 1.0])])
        .unwrap();
    let scan = scan_of(&[("one", "one"), ("two", "two")]);
    store.save_scan("/repo", &scan).unwrap();
    let hits = store.query("m:2", &[0.0, 1.0], 5).unwrap();
    assert_eq!(hits.len(), 2);
    assert!((hits[0].score - 1.0).abs() < 1e-6 && (hits[1].score - 1.0).abs() < 1e-6);
}

#[test]
fn a_vector_of_the_wrong_size_or_a_bad_hash_is_refused() {
    let (_dir, store) = store();
    let h = hash_of("x");
    assert!(
        store
            .append_vectors(
                "m:2",
                &[(h.clone(), vec![1.0, 0.0]), (hash_of("y"), vec![1.0])]
            )
            .is_err()
    );
    assert!(
        store
            .append_vectors("m:2", &[("zz".into(), vec![1.0, 0.0])])
            .is_err()
    );
    assert!(
        store
            .append_vectors("m:2", &[(h.clone(), vec![f32::NAN, 0.0])])
            .is_err()
    );
    assert!(store.known_hashes("m:2").unwrap().is_empty());
}

#[test]
fn brute_force_ranking_orders_by_cosine_and_honours_top_k() {
    let (_dir, store) = store();
    let files = [
        ("exact.rs", "exact\n"),
        ("near.rs", "near\n"),
        ("far.rs", "far\n"),
        ("opposite.rs", "opposite\n"),
    ];
    store.save_scan("/repo", &scan_of(&files)).unwrap();
    store
        .append_vectors(
            "m:2",
            &[
                (hash_of("exact"), vec![1.0, 0.0]),
                (hash_of("near"), vec![1.0, 0.2]),
                (hash_of("far"), vec![0.0, 1.0]),
                (hash_of("opposite"), vec![-1.0, 0.0]),
            ],
        )
        .unwrap();
    let hits = store.query("m:2", &[2.0, 0.0], 3).unwrap();
    let order: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(order, ["exact.rs", "near.rs", "far.rs"]);
    assert!((hits[0].score - 1.0).abs() < 1e-6);
    assert!(hits[1].score < hits[0].score && hits[1].score > hits[2].score);
    assert_eq!((hits[0].start_line, hits[0].end_line), (0, 0));
    assert_eq!(store.query("m:2", &[2.0, 0.0], 1).unwrap().len(), 1);
}

#[test]
fn equal_scores_are_ordered_by_path_so_results_are_deterministic() {
    let (_dir, store) = store();
    // Two files with the same text share one hash and so one vector: both are returned.
    store
        .save_scan("/repo", &scan_of(&[("z.rs", "same\n"), ("a.rs", "same\n")]))
        .unwrap();
    store
        .append_vectors("m:2", &[(hash_of("same"), vec![1.0, 0.0])])
        .unwrap();
    let hits = store.query("m:2", &[1.0, 0.0], 10).unwrap();
    let order: Vec<&str> = hits.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(order, ["a.rs", "z.rs"]);
}

#[test]
fn fragments_without_a_vector_are_not_candidates() {
    let (_dir, store) = store();
    store
        .save_scan("/repo", &scan_of(&[("a.rs", "a\n"), ("b.rs", "b\n")]))
        .unwrap();
    store
        .append_vectors("m:2", &[(hash_of("a"), vec![1.0, 0.0])])
        .unwrap();
    let hits = store.query("m:2", &[1.0, 0.0], 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "a.rs");
}

#[test]
fn querying_without_a_scan_vectors_or_the_right_size_is_an_error() {
    let (_dir, store) = store();
    assert!(store.query("m:2", &[1.0, 0.0], 3).is_err());
    store
        .save_scan("/repo", &scan_of(&[("a.rs", "a\n")]))
        .unwrap();
    assert!(store.query("m:2", &[1.0, 0.0], 3).is_err());
    store
        .append_vectors("m:2", &[(hash_of("a"), vec![1.0, 0.0])])
        .unwrap();
    assert!(store.query("m:2", &[1.0, 0.0, 0.0], 3).is_err());
    assert!(store.query("m:2", &[1.0, 0.0], 3).is_ok());
}

#[test]
fn a_torn_tail_is_ignored_and_the_next_append_starts_clean() {
    let (_dir, store) = store();
    let (h1, h2) = (hash_of("one"), hash_of("two"));
    store
        .append_vectors("m:2", &[(h1.clone(), vec![1.0, 0.0])])
        .unwrap();
    // A crash mid-append: half a record on the end.
    let path = fs::read_dir(store.dir())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "bin"))
        .unwrap();
    let mut bytes = fs::read(&path).unwrap();
    let whole = bytes.len();
    bytes.extend_from_slice(&[7u8; 20]);
    fs::write(&path, &bytes).unwrap();
    assert_eq!(
        store.known_hashes("m:2").unwrap(),
        HashSet::from([h1.clone()])
    );
    store
        .append_vectors("m:2", &[(h2.clone(), vec![0.0, 1.0])])
        .unwrap();
    assert_eq!(store.known_hashes("m:2").unwrap(), HashSet::from([h1, h2]));
    assert_eq!(fs::read(&path).unwrap().len(), whole + 32 + 8);
}

#[test]
fn a_new_dimension_starts_the_file_over() {
    let (_dir, store) = store();
    store
        .append_vectors("m:2", &[(hash_of("one"), vec![1.0, 0.0])])
        .unwrap();
    store
        .append_vectors("m:2", &[(hash_of("two"), vec![1.0, 0.0, 0.5])])
        .unwrap();
    assert_eq!(
        store.known_hashes("m:2").unwrap(),
        HashSet::from([hash_of("two")])
    );
}

#[test]
fn other_spaces_are_dropped_when_the_model_changes() {
    let (_dir, store) = store();
    store
        .append_vectors("old:2", &[(hash_of("one"), vec![1.0, 0.0])])
        .unwrap();
    store
        .append_vectors("new:2", &[(hash_of("one"), vec![1.0, 0.0])])
        .unwrap();
    store.drop_other_spaces("new:2");
    assert!(store.known_hashes("old:2").unwrap().is_empty());
    assert_eq!(store.known_hashes("new:2").unwrap().len(), 1);
}

#[test]
fn compaction_drops_dead_records_only_when_much_is_wasted() {
    let (_dir, store) = store();
    let bodies: Vec<String> = (0..300).map(|i| format!("body {i}")).collect();
    let vectors: Vec<(String, Vec<f32>)> = bodies
        .iter()
        .map(|b| (hash_of(b), vec![1.0, 2.0]))
        .collect();
    store.append_vectors("m:2", &vectors).unwrap();
    let before = store.known_hashes("m:2").unwrap();
    assert_eq!(before.len(), 300);

    // Nearly everything is live: nothing is rewritten.
    let live_most: HashSet<String> = bodies.iter().take(290).map(|b| hash_of(b)).collect();
    store.compact_wasteful(&live_most).unwrap();
    assert_eq!(store.known_hashes("m:2").unwrap().len(), 300);

    // Little is live: the dead records go.
    let live_few: HashSet<String> = bodies.iter().take(10).map(|b| hash_of(b)).collect();
    store.compact_wasteful(&live_few).unwrap();
    assert_eq!(store.known_hashes("m:2").unwrap(), live_few);
}

#[test]
fn drop_all_forgets_the_repository() {
    let (_dir, store) = store();
    store.save_scan("/repo", &scan_of(&[("a", "x\n")])).unwrap();
    store
        .append_vectors("m:2", &[(hash_of("x"), vec![1.0, 0.0])])
        .unwrap();
    store.drop_all();
    assert!(store.load_scan().is_none());
    assert!(store.known_hashes("m:2").unwrap().is_empty());
}

#[test]
fn cosine_handles_zero_and_mismatched_vectors() {
    assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
    assert_eq!(cosine(&[1.0], &[1.0, 0.0]), 0.0);
    assert!((cosine(&[3.0, 4.0], &[3.0, 4.0]) - 1.0).abs() < 1e-6);
    assert!((cosine(&[1.0, 0.0], &[0.0, 1.0])).abs() < 1e-6);
}
