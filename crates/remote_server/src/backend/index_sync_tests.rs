use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::super::contract_tests::scratch_home_executor;
use super::super::{ShellBackend, ShellIndex};
use super::*;

/// A fake provider: a deterministic 8-dimensional vector per text (a bag of letters), counting
/// what it was asked to embed. No network.
struct FakeEmbedder {
    key: Mutex<Option<String>>,
    texts: Mutex<Vec<String>>,
    calls: AtomicUsize,
    fail: Mutex<bool>,
    batch: usize,
}

impl FakeEmbedder {
    fn new(key: &str) -> Self {
        Self {
            key: Mutex::new(Some(key.into())),
            texts: Mutex::default(),
            calls: AtomicUsize::new(0),
            fail: Mutex::new(false),
            batch: 64,
        }
    }

    fn embedded(&self) -> Vec<String> {
        let mut texts = self.texts.lock().unwrap().clone();
        texts.sort();
        texts
    }

    fn reset(&self) {
        self.texts.lock().unwrap().clear();
        self.calls.store(0, Ordering::SeqCst);
    }
}

fn vector_of(text: &str) -> Vec<f32> {
    let mut v = vec![0f32; 8];
    for b in text.bytes() {
        v[(b % 8) as usize] += 1.0;
    }
    v
}

impl Embedder for FakeEmbedder {
    fn storage_key(&self) -> Option<String> {
        self.key.lock().unwrap().clone()
    }

    fn max_batch(&self) -> usize {
        self.batch
    }

    fn embed(&self, texts: Vec<String>) -> BoxFuture<'_, Result<Vec<Vec<f32>>, String>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if *self.fail.lock().unwrap() {
                return Err("HTTP 500".into());
            }
            let vectors = texts.iter().map(|t| vector_of(t)).collect();
            self.texts.lock().unwrap().extend(texts);
            Ok(vectors)
        })
    }
}

fn shell() -> ShellBackend {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()))
        .path()
        .to_path_buf();
    ShellBackend::new(Some(scratch_home_executor()))
        .with_index(std::sync::Arc::new(ShellIndex::new(root, "t".into())))
}

fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "alpha beta\n").unwrap();
    std::fs::write(dir.path().join("b.txt"), "gamma delta\n").unwrap();
    std::fs::write(dir.path().join("c.txt"), "epsilon\n").unwrap();
    dir
}

async fn sync(b: &ShellBackend, e: &FakeEmbedder, repo: &str) -> Result<SyncOutcome, SyncError> {
    sync_repo(b, e, repo, |_| {}).await
}

#[tokio::test]
async fn the_first_sync_embeds_everything_and_the_second_embeds_nothing() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    let one = sync(&b, &e, &repo).await.unwrap();
    assert_eq!(
        (one.files, one.fragments, one.embedded, one.already),
        (3, 3, 3, 0)
    );
    assert_eq!(e.embedded(), ["alpha beta", "epsilon", "gamma delta"]);

    e.reset();
    let two = sync(&b, &e, &repo).await.unwrap();
    assert_eq!((two.embedded, two.already), (0, 3));
    assert_eq!(e.calls.load(Ordering::SeqCst), 0, "no request at all");
    assert_eq!(one.root_hash, two.root_hash);
}

#[tokio::test]
async fn an_edit_embeds_only_the_changed_fragment() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    let one = sync(&b, &e, &repo).await.unwrap();
    e.reset();
    std::fs::write(dir.path().join("b.txt"), "gamma delta zeta\n").unwrap();
    let two = sync(&b, &e, &repo).await.unwrap();
    assert_eq!((two.embedded, two.already), (1, 2));
    assert_eq!(e.embedded(), ["gamma delta zeta"]);
    assert_ne!(one.root_hash, two.root_hash);
}

#[tokio::test]
async fn a_new_storage_key_embeds_everything_again() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    sync(&b, &e, &repo).await.unwrap();
    e.reset();
    *e.key.lock().unwrap() = Some("other:8".into());
    let again = sync(&b, &e, &repo).await.unwrap();
    assert_eq!((again.embedded, again.already), (3, 0));
}

#[tokio::test]
async fn batches_follow_the_embedders_limit() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, mut e) = (shell(), FakeEmbedder::new("m:8"));
    e.batch = 2;
    sync(&b, &e, &repo).await.unwrap();
    assert_eq!(
        e.calls.load(Ordering::SeqCst),
        2,
        "3 fragments in batches of 2"
    );
}

#[tokio::test]
async fn progress_reports_the_scan_then_the_count_of_embedded_fragments() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    let mut seen = Vec::new();
    sync_repo(&b, &e, &repo, |p| seen.push(p)).await.unwrap();
    assert_eq!(seen.first(), Some(&SyncProgress::Scanning));
    assert_eq!(
        seen.last(),
        Some(&SyncProgress::Embedding {
            completed: 3,
            total: 3
        })
    );
}

#[tokio::test]
async fn no_provider_is_reported_before_anything_runs() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    *e.key.lock().unwrap() = None;
    assert!(matches!(
        sync(&b, &e, &repo).await,
        Err(SyncError::NoProvider)
    ));
    assert!(matches!(
        search_repo(&b, &e, &repo, "q", 3).await,
        Err(SyncError::NoProvider)
    ));
}

#[tokio::test]
async fn a_failing_provider_fails_the_sync_and_the_next_one_resumes() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    *e.fail.lock().unwrap() = true;
    assert!(matches!(
        sync(&b, &e, &repo).await,
        Err(SyncError::Embed(_))
    ));
    *e.fail.lock().unwrap() = false;
    let ok = sync(&b, &e, &repo).await.unwrap();
    assert_eq!(ok.embedded, 3);
}

#[tokio::test]
async fn a_scan_the_host_refuses_is_a_host_error() {
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    let err = sync(&b, &e, "/no/such/directory").await.unwrap_err();
    assert!(matches!(err, SyncError::Host(_)), "{err:?}");
}

#[tokio::test]
async fn search_ranks_by_the_query_vector_and_returns_absolute_paths() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    sync(&b, &e, &repo).await.unwrap();
    e.reset();
    // The query is the text of one fragment: its own vector is the best match.
    let hits = search_repo(&b, &e, &repo, "gamma delta", 2).await.unwrap();
    assert_eq!(e.embedded(), ["gamma delta"], "only the query is embedded");
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].path, format!("{repo}/b.txt"));
    assert!((hits[0].score - 1.0).abs() < 1e-5);
    assert!(hits[0].score >= hits[1].score);
    assert_eq!(ranked_paths(&hits)[0], format!("{repo}/b.txt"));
}

#[tokio::test]
async fn searching_an_unsynced_repository_says_it_is_not_indexed() {
    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let (b, e) = (shell(), FakeEmbedder::new("m:8"));
    let err = search_repo(&b, &e, &repo, "q", 3).await.unwrap_err();
    assert!(matches!(err, SyncError::NotIndexed(_)), "{err:?}");
}

#[test]
fn ranked_paths_keeps_best_first_and_drops_repeats() {
    let hit = |p: &str, s: f32| Hit {
        path: p.into(),
        start_line: 0,
        end_line: 0,
        score: s,
    };
    let hits = [hit("/r/a", 0.9), hit("/r/b", 0.8), hit("/r/a", 0.7)];
    assert_eq!(ranked_paths(&hits), ["/r/a", "/r/b"]);
    assert_eq!(absolute_path("/r/", "x/y"), "/r/x/y");
}

#[cfg(all(feature = "daemon", unix))]
#[tokio::test]
async fn the_helper_arm_syncs_and_searches_the_same_way() {
    use super::super::HelperBackend;
    use crate::client::RemoteServerClient;
    use warpui::r#async::executor;

    let stream = crate::daemon::spawn_in_process_for_tests();
    let stream = async_io::Async::new(stream).unwrap();
    let (reader, writer) = futures::io::AsyncReadExt::split(stream);
    let background = executor::Background::default();
    let (client, _events, _host_responses) = RemoteServerClient::new(reader, writer, &background);
    let helper = HelperBackend::new(std::sync::Arc::new(client));

    let dir = repo();
    let repo = dir.path().to_string_lossy().into_owned();
    let e = FakeEmbedder::new("m:8");
    let one = sync_repo(&helper, &e, &repo, |_| {}).await.unwrap();
    assert_eq!((one.embedded, one.already), (3, 0));
    e.reset();
    let two = sync_repo(&helper, &e, &repo, |_| {}).await.unwrap();
    assert_eq!((two.embedded, two.already), (0, 3));
    let hits = search_repo(&helper, &e, &repo, "epsilon", 1).await.unwrap();
    assert_eq!(hits[0].path, format!("{repo}/c.txt"));
}
