//! Unit tests for the models.dev option (#1791): the `models_dev_fetch` gate on both entry
//! points, the configured URL, response validation, and the cache -> bundled fallback.
use super::*;
use sha2::{Digest, Sha256};
use std::cell::{Cell, RefCell};
use std::sync::Mutex;

/// The tests share the process-wide catalog singleton; run them one at a time.
static SERIAL: Mutex<()> = Mutex::new(());

fn serial() -> std::sync::MutexGuard<'static, ()> {
    let g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    reset_state_for_test();
    g
}

const SMALL_CATALOG: &str = r#"{"cache-only":{"id":"cache-only","name":"Cache Only","models":{"m":{"id":"m","name":"M"}}}}"#;

fn cache_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join("models-dev.json")
}

fn ok_fetch(_url: String) -> std::future::Ready<Result<(), String>> {
    std::future::ready(Ok(()))
}

#[test]
fn disabled_startup_path_never_fetches_even_with_a_custom_url() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let called = Cell::new(false);
    let r = futures::executor::block_on(ensure_loaded(
        false,
        "https://example.invalid/custom.json".to_owned(),
        cache_in(&dir),
        |_url| {
            called.set(true);
            ok_fetch(String::new())
        },
    ));
    assert!(
        !called.get(),
        "EnsureModelsDevLoaded must not fetch when disabled"
    );
    assert_eq!(r, Ok(LoadedFrom::Bundled));
}

#[test]
fn disabled_refresh_path_never_fetches_even_with_a_custom_url() {
    let _g = serial();
    let called = Cell::new(false);
    let r = futures::executor::block_on(refresh_catalog(
        false,
        "https://example.invalid/custom.json".to_owned(),
        |_url| {
            called.set(true);
            ok_fetch(String::new())
        },
    ));
    assert!(
        !called.get(),
        "RefreshModelsDev must not fetch when disabled"
    );
    assert!(r.is_err());
    assert!(
        has_catalog(),
        "a refused refresh still leaves the bundled catalog"
    );
}

#[test]
fn disabled_with_no_cache_loads_the_bundled_catalog() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let r = futures::executor::block_on(ensure_loaded(
        false,
        DEFAULT_MODELS_DEV_URL.to_owned(),
        cache_in(&dir),
        ok_fetch,
    ));
    assert_eq!(r, Ok(LoadedFrom::Bundled));
    let catalog = cached().expect("catalog loaded");
    assert!(
        catalog.len() >= 200,
        "bundled catalog has {} providers",
        catalog.len()
    );
    assert!(!catalog.values().all(|p| p.models.is_empty()));
}

#[test]
fn disabled_with_a_cache_prefers_the_cache_over_the_bundled_catalog() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = cache_in(&dir);
    std::fs::write(&path, SMALL_CATALOG).unwrap();
    // Make the cache stale so the only reason not to fetch is the setting.
    let old = SystemTime::now() - Duration::from_secs(3 * 24 * 60 * 60);
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let called = Cell::new(false);
    let r = futures::executor::block_on(ensure_loaded(
        false,
        DEFAULT_MODELS_DEV_URL.to_owned(),
        path,
        |_url| {
            called.set(true);
            ok_fetch(String::new())
        },
    ));
    assert!(!called.get());
    assert_eq!(r, Ok(LoadedFrom::Cache));
    let catalog = cached().unwrap();
    assert_eq!(catalog.len(), 1);
    assert!(catalog.contains_key("cache-only"));
}

#[test]
fn enabled_fetches_from_the_configured_url() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let seen = RefCell::new(None);
    let r = futures::executor::block_on(ensure_loaded(
        true,
        resolve_url("  https://mirror.example/api.json "),
        cache_in(&dir),
        |url| {
            *seen.borrow_mut() = Some(url);
            ok_fetch(String::new())
        },
    ));
    assert_eq!(r, Ok(LoadedFrom::Fetched));
    assert_eq!(
        seen.borrow().as_deref(),
        Some("https://mirror.example/api.json")
    );

    let seen = RefCell::new(None);
    futures::executor::block_on(refresh_catalog(
        true,
        "https://models.dev/api.json".to_owned(),
        |url| {
            *seen.borrow_mut() = Some(url);
            ok_fetch(String::new())
        },
    ))
    .unwrap();
    assert_eq!(
        seen.borrow().as_deref(),
        Some("https://models.dev/api.json")
    );
}

#[test]
fn default_url_is_the_repo_raw_copy_and_blank_falls_back_to_it() {
    assert_eq!(
        DEFAULT_MODELS_DEV_URL,
        "https://gitlab.com/winters-phosphor/phosphor/-/raw/main/assets/models_dev/api.json"
    );
    assert_eq!(resolve_url(""), DEFAULT_MODELS_DEV_URL);
    assert_eq!(resolve_url("   "), DEFAULT_MODELS_DEV_URL);
    let repo_copy = Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/models_dev/api.json");
    assert!(
        repo_copy.is_file(),
        "the default URL's path must exist in the repo"
    );
}

#[test]
fn an_invalid_response_is_not_cached_and_falls_back_to_the_bundled_catalog() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = cache_in(&dir);
    for body in [&b"<html>captive portal</html>"[..], b"[]", b"{}", b""] {
        assert!(ingest_response(body, &path).is_err());
        assert!(!path.exists(), "a bad response must never be cached");
    }
    let p2 = path.clone();
    let r = futures::executor::block_on(ensure_loaded(
        true,
        "https://example.invalid/x".to_owned(),
        path.clone(),
        move |_url| std::future::ready(ingest_response(b"not json", &p2)),
    ));
    assert!(r.is_err(), "the failed fetch is reported");
    assert!(!path.exists());
    assert!(
        cached().unwrap().len() >= 200,
        "fell back to the bundled catalog"
    );
}

#[test]
fn a_valid_response_is_cached_and_installed() {
    let _g = serial();
    let dir = tempfile::tempdir().unwrap();
    let path = cache_in(&dir);
    ingest_response(SMALL_CATALOG.as_bytes(), &path).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), SMALL_CATALOG.as_bytes());
    assert!(cached().unwrap().contains_key("cache-only"));
}

#[test]
fn bundled_catalog_is_the_recorded_download() {
    let json = bundled_json().expect("decompresses");
    let sha = format!("{:x}", Sha256::digest(&json));
    assert_eq!(Some(sha.as_str()), bundled_provenance("sha256"));
    let repo_copy =
        std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../assets/models_dev/api.json"))
            .expect("repo copy");
    assert_eq!(
        repo_copy, json,
        "assets/models_dev/api.json and the bundled gzip must match"
    );
    let catalog: Catalog = serde_json::from_slice(&json).expect("parses");
    assert_eq!(
        Some(catalog.len().to_string().as_str()),
        bundled_provenance("providers")
    );
    assert!(bundled_provenance("fetched_at").is_some());
}
