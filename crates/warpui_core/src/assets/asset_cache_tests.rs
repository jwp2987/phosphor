use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::{Duration, UNIX_EPOCH};

use super::{AssetCache, AssetHandle, AssetSource, AssetStateInternal, LocalFileContentVersion};

#[cfg(not(target_arch = "wasm32"))]
fn unique_temp_path(name: &str) -> std::path::PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "warp_asset_cache_test_{}_{name}",
        std::process::id()
    ));
    path
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn local_file_content_version_changes_when_file_contents_change() {
    let path = unique_temp_path("contents_change.png");
    std::fs::write(&path, b"aaaa").expect("write temp file");
    let path_string = path.to_string_lossy().to_string();

    let source = AssetSource::LocalFile {
        path: path_string.clone(),
        content_version: None,
    }
    .with_local_file_content_version();

    match &source {
        AssetSource::LocalFile {
            path: resolved_path,
            content_version,
        } => {
            assert_eq!(resolved_path, &path_string);
            let content_version = content_version
                .as_ref()
                .expect("expected a content version for an existing file");
            assert_eq!(
                content_version.modified,
                std::fs::metadata(&path)
                    .expect("read temp file metadata")
                    .modified()
                    .ok()
            );
            assert_eq!(content_version.file_size, 4);
        }
        other => panic!("expected a local file source, got {other:?}"),
    }

    let unchanged = AssetSource::LocalFile {
        path: path_string.clone(),
        content_version: None,
    }
    .with_local_file_content_version();
    assert_eq!(
        source, unchanged,
        "an unmodified file should produce the same cache key"
    );

    std::fs::write(&path, b"bbbbbbbb").expect("rewrite temp file");
    let after_change = AssetSource::LocalFile {
        path: path_string,
        content_version: None,
    }
    .with_local_file_content_version();
    assert_ne!(
        source, after_change,
        "changing file contents should produce a different cache key"
    );

    let _ = std::fs::remove_file(&path);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn local_file_content_version_is_none_for_missing_file() {
    let path = unique_temp_path("definitely_missing.png");
    let _ = std::fs::remove_file(&path);
    assert!(
        LocalFileContentVersion::for_path(&path).is_none(),
        "a missing file should not produce a content version"
    );
}

#[test]
fn with_local_file_content_version_leaves_non_local_sources_unchanged() {
    let bundled = AssetSource::Bundled { path: "icon.svg" };
    assert_eq!(bundled.clone().with_local_file_content_version(), bundled);
}

#[test]
fn evicts_oldest_versioned_local_file_assets_over_limit() {
    let oldest_source = AssetSource::LocalFile {
        path: "oldest.png".to_string(),
        content_version: Some(LocalFileContentVersion {
            modified: Some(UNIX_EPOCH + Duration::from_secs(1)),
            file_size: 4,
        }),
    };
    let newest_source = AssetSource::LocalFile {
        path: "newest.png".to_string(),
        content_version: Some(LocalFileContentVersion {
            modified: Some(UNIX_EPOCH + Duration::from_secs(2)),
            file_size: 4,
        }),
    };
    let unversioned_source = AssetSource::LocalFile {
        path: "unversioned.png".to_string(),
        content_version: None,
    };
    let oldest_handle = AssetHandle {
        source: oldest_source.clone(),
        asset_type: TypeId::of::<String>(),
    };
    let newest_handle = AssetHandle {
        source: newest_source.clone(),
        asset_type: TypeId::of::<String>(),
    };
    let unversioned_handle = AssetHandle {
        source: unversioned_source,
        asset_type: TypeId::of::<String>(),
    };
    let mut assets = HashMap::from([
        (
            oldest_handle.clone(),
            AssetStateInternal::Loaded {
                data: Rc::new(String::new()) as Rc<dyn Any>,
                timestamp: 1,
                size_in_bytes: 4,
            },
        ),
        (
            newest_handle.clone(),
            AssetStateInternal::Loaded {
                data: Rc::new(String::new()) as Rc<dyn Any>,
                timestamp: 2,
                size_in_bytes: 4,
            },
        ),
        (
            unversioned_handle.clone(),
            AssetStateInternal::Loaded {
                data: Rc::new(String::new()) as Rc<dyn Any>,
                timestamp: 0,
                size_in_bytes: 10,
            },
        ),
    ]);

    let evicted_sources = AssetCache::evict_versioned_local_file_assets(&mut assets, 4);

    assert_eq!(evicted_sources, vec![oldest_source]);
    assert!(matches!(
        assets.get(&oldest_handle),
        Some(AssetStateInternal::Evicted)
    ));
    assert!(matches!(
        assets.get(&newest_handle),
        Some(AssetStateInternal::Loaded { .. })
    ));
    assert!(matches!(
        assets.get(&unversioned_handle),
        Some(AssetStateInternal::Loaded { .. })
    ));
}

// --- Off-thread raw asset decoding (#880) -----------------------------------------------------

mod async_raw {
    use super::*;
    use crate::assets::asset_cache::Asset;
    use crate::assets::asset_cache::PreparedAsset;
    use crate::image_cache::{DecodedImage, ImageCache, ImageType, MAX_RAW_IMAGE_SIDE};
    use crate::r#async::executor::{Background, Foreground};
    use crate::App;
    use std::sync::Arc;

    fn png_bytes(width: u32, height: u32) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::new(width, height)
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("encode png");
        out.into_inner()
    }

    fn new_cache() -> AssetCache {
        AssetCache::new(
            Box::new(()),
            ImageCache::new(),
            Foreground::test().into(),
            Background::default().into(),
        )
    }

    fn raw_handle(id: &str) -> AssetHandle {
        AssetHandle {
            source: AssetSource::Raw { id: id.to_owned() },
            asset_type: TypeId::of::<ImageType>(),
        }
    }

    fn state(cache: &AssetCache, id: &str) -> crate::assets::asset_cache::AssetState<ImageType> {
        cache.load_asset::<ImageType>(AssetSource::Raw { id: id.to_owned() })
    }

    #[test]
    fn pending_then_ready() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    cache.insert_raw_asset_bytes_async::<ImageType>(
                        "1".into(),
                        Arc::new(png_bytes(3, 2)),
                        mctx,
                    );
                    // Nothing is decoded synchronously: the asset reads as loading.
                    assert!(cache.is_raw_asset_pending::<ImageType>("1"));
                    assert!(matches!(
                        state(cache, "1"),
                        crate::assets::asset_cache::AssetState::Loading { .. }
                    ));
                });
            });
            app.finish_pending_tasks().await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                assert!(!cache.is_raw_asset_pending::<ImageType>("1"));
                match state(cache, "1") {
                    crate::assets::asset_cache::AssetState::Loaded { data } => {
                        assert_eq!(data.image_size().map(|s| (s.x(), s.y())), Some((3, 2)));
                    }
                    _ => panic!("expected the decoded image"),
                }
            });
        });
    }

    #[test]
    fn failure_is_recorded_and_not_pending() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    // Starts like a PNG but is truncated garbage.
                    let mut bytes = png_bytes(4, 4);
                    bytes.truncate(40);
                    cache.insert_raw_asset_bytes_async::<ImageType>(
                        "bad".into(),
                        Arc::new(bytes),
                        mctx,
                    );
                });
            });
            app.finish_pending_tasks().await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                assert!(!cache.is_raw_asset_pending::<ImageType>("bad"));
                assert!(matches!(
                    state(cache, "bad"),
                    crate::assets::asset_cache::AssetState::FailedToLoad(_)
                ));
            });
        });
    }

    #[test]
    fn size_cap_is_still_enforced_off_thread() {
        // An SVG that would rasterise to ~40 GB is refused by the decode, wherever it runs.
        let svg = br#"<svg width="100000" height="100000" xmlns="http://www.w3.org/2000/svg"/>"#;
        assert!(DecodedImage::decode(svg).is_err());
        assert!(ImageType::decode_in_background(svg).is_err());
        // And an in-limit image still decodes.
        assert!(ImageType::decode_in_background(&png_bytes(2, 2)).is_ok());
    }

    #[test]
    fn stale_result_is_dropped_when_reinserted() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    let handle = raw_handle("7");
                    let first = cache.begin_pending_raw(&handle);
                    let second = cache.begin_pending_raw(&handle);
                    assert_ne!(first, second);

                    let stale = ImageType::decode_in_background(&png_bytes(1, 1));
                    assert!(!cache.finish_pending_raw::<ImageType>(
                        handle.clone(),
                        first,
                        stale,
                        mctx
                    ));
                    // Still waiting on the current request.
                    assert!(cache.is_raw_asset_pending::<ImageType>("7"));

                    let current = ImageType::decode_in_background(&png_bytes(5, 5));
                    assert!(cache.finish_pending_raw::<ImageType>(handle, second, current, mctx));
                    match state(cache, "7") {
                        crate::assets::asset_cache::AssetState::Loaded { data } => {
                            assert_eq!(data.image_size().map(|s| s.x()), Some(5));
                        }
                        _ => panic!("expected the second image"),
                    }
                });
            });
        });
    }

    #[test]
    fn result_is_dropped_when_removed_while_pending() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    let handle = raw_handle("9");
                    let generation = cache.begin_pending_raw(&handle);
                    assert!(cache.remove_raw_asset::<ImageType>("9"));
                    assert!(!cache.is_raw_asset_pending::<ImageType>("9"));

                    let late = ImageType::decode_in_background(&png_bytes(2, 2));
                    assert!(!cache.finish_pending_raw::<ImageType>(handle, generation, late, mctx));
                    // The late result did not resurrect the asset.
                    assert!(!matches!(
                        state(cache, "9"),
                        crate::assets::asset_cache::AssetState::Loaded { .. }
                    ));
                });
            });
        });
    }

    #[test]
    fn remove_right_after_insert_drops_the_decode_result() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    cache.insert_raw_asset_bytes_async::<ImageType>(
                        "11".into(),
                        Arc::new(png_bytes(2, 2)),
                        mctx,
                    );
                    cache.remove_raw_asset::<ImageType>("11");
                });
            });
            app.finish_pending_tasks().await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                assert!(!matches!(
                    state(cache, "11"),
                    crate::assets::asset_cache::AssetState::Loaded { .. }
                ));
            });
        });
    }

    struct PanicAsset;
    impl Asset for PanicAsset {
        fn try_from_bytes(_: &[u8]) -> anyhow::Result<Self> {
            panic!("boom")
        }
        fn size_in_bytes(&self) -> usize {
            0
        }
        fn decode_in_background(_: &[u8]) -> anyhow::Result<PreparedAsset<Self>> {
            panic!("boom in decoder")
        }
    }

    #[test]
    fn decoder_panic_ends_in_failure_not_loading_forever() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    cache.insert_raw_asset_bytes_async::<PanicAsset>(
                        "p".into(),
                        Arc::new(vec![1]),
                        mctx,
                    );
                });
            });
            app.finish_pending_tasks().await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                assert!(!cache.is_raw_asset_pending::<PanicAsset>("p"));
                assert!(matches!(
                    cache.load_asset::<PanicAsset>(AssetSource::Raw { id: "p".into() }),
                    crate::assets::asset_cache::AssetState::FailedToLoad(_)
                ));
            });
        });
    }

    /// Waits until no raw decode is pending, including ones that only start when a slot frees.
    async fn settle(app: &mut App, cache: &crate::ModelHandle<AssetCache>) {
        for _ in 0..200 {
            app.finish_pending_tasks().await;
            let any = app.read(|ctx| !cache.as_ref(ctx).pending_raw.borrow().is_empty());
            if !any {
                return;
            }
        }
        panic!("decodes never settled");
    }

    #[test]
    fn decodes_past_the_in_flight_cap_are_queued_and_all_load() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    for i in 0..40 {
                        cache.insert_raw_asset_bytes_async::<ImageType>(
                            format!("q{i}"),
                            Arc::new(png_bytes(1 + i % 3, 1)),
                            mctx,
                        );
                    }
                    assert_eq!(
                        cache.in_flight_decodes.get(),
                        AssetCache::MAX_IN_FLIGHT_RAW_DECODES
                    );
                    assert_eq!(cache.decode_queue.borrow().len(), 40 - 16);
                    // Queued ones read as loading, not failed.
                    assert!(matches!(
                        state(cache, "q39"),
                        crate::assets::asset_cache::AssetState::Loading { .. }
                    ));
                });
            });
            settle(&mut app, &cache).await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                for i in 0..40u32 {
                    match state(cache, &format!("q{i}")) {
                        crate::assets::asset_cache::AssetState::Loaded { data } => {
                            assert_eq!(data.image_size().map(|s| s.x()), Some((1 + i % 3) as i32));
                        }
                        _ => panic!("q{i} should be loaded"),
                    }
                }
                assert_eq!(cache.in_flight_decodes.get(), 0);
                assert_eq!(cache.in_flight_decode_bytes.get(), 0);
                assert_eq!(cache.queued_decode_bytes.get(), 0);
                assert!(cache.decode_queue.borrow().is_empty());
            });
        });
    }

    #[test]
    fn mixed_burst_of_small_and_large_images_all_load_and_none_is_evicted() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    for i in 0..50 {
                        cache.insert_raw_asset_bytes_async::<ImageType>(
                            format!("s{i}"),
                            Arc::new(png_bytes(30, 20)),
                            mctx,
                        );
                    }
                    // Over the retained-side cap: decoded 4200x4200 is 70 MB each.
                    for i in 0..3 {
                        cache.insert_raw_asset_bytes_async::<ImageType>(
                            format!("big{i}"),
                            Arc::new(png_bytes(4200, 4200)),
                            mctx,
                        );
                    }
                });
            });
            settle(&mut app, &cache).await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                for id in (0..50).map(|i| format!("s{i}")) {
                    assert!(
                        matches!(
                            state(cache, &id),
                            crate::assets::asset_cache::AssetState::Loaded { .. }
                        ),
                        "{id} should be loaded"
                    );
                }
                for i in 0..3 {
                    match state(cache, &format!("big{i}")) {
                        crate::assets::asset_cache::AssetState::Loaded { data } => {
                            let size = data.image_size().expect("size");
                            assert_eq!(size.x().max(size.y()) as u32, MAX_RAW_IMAGE_SIDE);
                        }
                        _ => panic!("big{i} should be loaded"),
                    }
                }
                // Downscaled, the whole burst stays well under the raw-asset budget.
                assert!(cache.get_total_raw_asset_size() < 200 * 1000 * 1000);
            });
        });
    }

    #[test]
    fn decode_estimate_comes_from_the_header_and_big_images_are_bounded() {
        let png = png_bytes(8000, 8000);
        assert_eq!(ImageType::estimated_decode_bytes(&png), 8000 * 8000 * 4);
        assert_eq!(ImageType::estimated_decode_bytes(b"<svg/>"), 6);
        // Raw decode retains at most MAX_RAW_IMAGE_SIDE, keeping the aspect ratio.
        let wide = png_bytes(5000, 1000);
        let prepared = ImageType::decode_raw_in_background(&wide).expect("decode");
        let image = prepared().expect("finish");
        let size = image.image_size().expect("size");
        assert_eq!((size.x() as u32, size.y() as u32), (4096, 819));
        // The non-raw path keeps full resolution.
        let full = ImageType::decode_in_background(&wide).expect("decode")().expect("finish");
        assert_eq!(full.image_size().map(|s| s.x()), Some(5000));
    }

    #[test]
    fn touching_a_loaded_raw_asset_refreshes_its_eviction_order() {
        let cache = new_cache();
        let handle = raw_handle("t");
        cache.inner.borrow_mut().insert(
            handle.clone(),
            AssetStateInternal::Loaded {
                data: Rc::new(ImageType::Unrecognized) as Rc<dyn Any>,
                timestamp: 1,
                size_in_bytes: 0,
            },
        );
        let _ = state(&cache, "t");
        match cache.inner.borrow().get(&handle) {
            Some(AssetStateInternal::Loaded { timestamp, .. }) => assert!(*timestamp > 1),
            _ => panic!("still loaded"),
        }
    }

    #[test]
    fn queue_byte_cap_refuses_only_beyond_the_bound() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    for i in 0..AssetCache::MAX_IN_FLIGHT_RAW_DECODES {
                        cache.insert_raw_asset_bytes_async::<ImageType>(
                            format!("f{i}"),
                            Arc::new(png_bytes(1, 1)),
                            mctx,
                        );
                    }
                    // Zeroed allocation is lazy; this is not a real gigabyte of RAM.
                    let big = Arc::new(vec![0u8; AssetCache::MAX_QUEUED_RAW_DECODE_BYTES + 1]);
                    cache.insert_raw_asset_bytes_async::<ImageType>("big".into(), big, mctx);
                    assert!(!cache.is_raw_asset_pending::<ImageType>("big"));
                    assert!(matches!(
                        state(cache, "big"),
                        crate::assets::asset_cache::AssetState::FailedToLoad(_)
                    ));
                    assert_eq!(cache.queued_decode_bytes.get(), 0);
                });
            });
            settle(&mut app, &cache).await;
        });
    }

    #[test]
    fn removing_a_queued_decode_cancels_it() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    for i in 0..20 {
                        cache.insert_raw_asset_bytes_async::<ImageType>(
                            format!("x{i}"),
                            Arc::new(png_bytes(1, 1)),
                            mctx,
                        );
                    }
                    assert!(cache.remove_raw_asset::<ImageType>("x19"));
                    assert_eq!(cache.decode_queue.borrow().len(), 3);
                });
            });
            settle(&mut app, &cache).await;
            app.read(|ctx| {
                let cache = cache.as_ref(ctx);
                assert!(matches!(
                    state(cache, "x18"),
                    crate::assets::asset_cache::AssetState::Loaded { .. }
                ));
                assert!(!matches!(
                    state(cache, "x19"),
                    crate::assets::asset_cache::AssetState::Loaded { .. }
                ));
            });
        });
    }

    #[test]
    fn resending_an_id_keeps_the_old_image_until_the_new_one_is_ready() {
        App::test((), |mut app| async move {
            let cache = app.add_model(|_| new_cache());
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    cache.insert_raw_asset_bytes_async::<ImageType>(
                        "r".into(),
                        Arc::new(png_bytes(2, 2)),
                        mctx,
                    );
                });
            });
            app.finish_pending_tasks().await;
            app.update(|ctx| {
                cache.update(ctx, |cache, mctx| {
                    cache.insert_raw_asset_bytes_async::<ImageType>(
                        "r".into(),
                        Arc::new(png_bytes(6, 6)),
                        mctx,
                    );
                    assert!(cache.is_raw_asset_pending::<ImageType>("r"));
                    match state(cache, "r") {
                        crate::assets::asset_cache::AssetState::Loaded { data } => {
                            assert_eq!(data.image_size().map(|s| s.x()), Some(2), "no blank gap");
                        }
                        _ => panic!("old image should stay visible"),
                    }
                });
            });
            app.finish_pending_tasks().await;
            app.read(|ctx| match state(cache.as_ref(ctx), "r") {
                crate::assets::asset_cache::AssetState::Loaded { data } => {
                    assert_eq!(data.image_size().map(|s| s.x()), Some(6));
                }
                _ => panic!("expected the new image"),
            });
        });
    }
}
