use std::{borrow::Cow, rc::Rc};

use rust_embed::RustEmbed;

use crate::{
    r#async::executor::{Background, Foreground},
    AssetProvider,
};

use super::*;

#[derive(Clone, Copy, RustEmbed)]
#[folder = "test_data"]
pub struct Assets;

// Implement the AssetProvider trait here (required by App::new).
impl AssetProvider for Assets {
    fn get(&self, path: &str) -> Result<Cow<'_, [u8]>> {
        match path {
            "animated.webp" => Ok(Cow::Borrowed(include_bytes!("../test_data/animated.webp"))),
            "cache-test.svg" => Ok(Cow::Borrowed(
                br##"<svg width="20" height="10" viewBox="0 0 20 10" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="10" fill="#ffffff"/></svg>"##,
            )),
            "big-test.svg" => Ok(Cow::Borrowed(
                br##"<svg width="1000" height="1000" viewBox="0 0 1000 1000" xmlns="http://www.w3.org/2000/svg"><rect width="1000" height="1000" fill="#336699"/></svg>"##,
            )),
            "heavy.rgba" => {
                let mut bytes = b"warp-img:rgba:1024:1024:".to_vec();
                bytes.extend(std::iter::repeat_n(0x80u8, 1024 * 1024 * 4));
                Ok(Cow::Owned(bytes))
            }
            "many-nodes.svg" => {
                let mut svg = String::from(
                    r##"<svg width="1000" height="1000" xmlns="http://www.w3.org/2000/svg">"##,
                );
                for _ in 0..2001 {
                    svg.push_str(r##"<rect width="1" height="1"/>"##);
                }
                svg.push_str("</svg>");
                Ok(Cow::Owned(svg.into_bytes()))
            }
            "fit-test.rgba" => Ok(Cow::Borrowed(
                b"warp-img:rgba:4:2:\xff\x00\x00\xff\xff\x00\x00\xff\x00\x00\xff\xff\x00\x00\xff\xff\x00\xff\x00\xff\x00\xff\x00\xff\xff\xff\xff\xff\xff\xff\xff\xff",
            )),
            "numbers-1000ms.gif" => Ok(Cow::Borrowed(include_bytes!(
                "../../warpui/examples/assets/numbers-1000ms.gif"
            ))),
            _ => <Assets as RustEmbed>::get(path)
                .map(|f| f.data)
                .ok_or_else(|| anyhow!("no asset exists at path {}", path)),
        }
    }
}

fn new_asset_cache() -> AssetCache {
    AssetCache::new(
        Box::new(Assets),
        ImageCache::new(),
        Foreground::test().into(),
        Background::default().into(),
    )
}

fn load_bundled_image(
    image_cache: &ImageCache,
    asset_cache: &AssetCache,
    path: &'static str,
    bounds: Vector2I,
    fit_type: FitType,
    animated_image_behavior: AnimatedImageBehavior,
) -> Rc<Image> {
    let image = image_cache.image(
        AssetSource::Bundled { path },
        bounds,
        fit_type,
        animated_image_behavior,
        CacheOption::BySize,
        None,
        asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };
    image
}

#[test]
fn test_passes_through_asset_cache_original() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    let source = AssetSource::Bundled { path: "local.png" };
    let image_asset: AssetState<ImageType> = asset_cache.load_asset(source.clone());
    let AssetState::Loaded { data: image } = image_asset else {
        panic!("Bundled asset should be available immediately!");
    };
    let ImageType::StaticBitmap { image } = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };
    let image_asset_weak = Arc::downgrade(image);

    let bounds = Vector2I::new(1024, 1024);
    let image = image_cache.image(
        source,
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::Original,
        None,
        &asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };
    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };

    // Assert that the image returned from the image cache and the asset stored
    // in the asset cache point to the same underlying data (i.e.: there were
    // no copies made).
    assert!(image_asset_weak.ptr_eq(&Arc::downgrade(image)));
}

#[test]
fn test_passes_through_asset_cache_original_when_target_size_matches_source_size() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    let source = AssetSource::Bundled { path: "local.png" };
    let image_asset: AssetState<ImageType> = asset_cache.load_asset(source.clone());
    let AssetState::Loaded { data: image } = image_asset else {
        panic!("Bundled asset should be available immediately!");
    };
    let ImageType::StaticBitmap { image } = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };
    let image_asset_weak = Arc::downgrade(image);

    // Load the image with `CacheOption::BySize` but use the source asset's
    // size as the bounds.
    let bounds = image.size();
    let image = image_cache.image(
        source,
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::BySize,
        None,
        &asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };
    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };

    // Assert that the image returned from the image cache and the asset stored
    // in the asset cache point to the same underlying data (i.e.: there were
    // no copies made).
    assert!(image_asset_weak.ptr_eq(&Arc::downgrade(image)));
}

#[test]
fn test_respects_max_dimensions_for_cacheoption_original() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    // We pass a very small value for bounds, which should get ignored due to
    // use of `CacheOption::Original`.
    let bounds = Vector2I::new(10, 10);

    let image = image_cache.image(
        AssetSource::Bundled { path: "local.png" },
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::Original,
        None,
        &asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };
    // Assert that the image, without resizing or a max dimension, matches our expectations.
    assert_eq!(image.img.dimensions(), (1024, 1024));

    let image = image_cache.image(
        AssetSource::Bundled { path: "local.png" },
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::Original,
        Some(512),
        &asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };
    // Assert that, when we specify a max dimension of 512, the image is resized accordingly.
    assert_eq!(image.img.dimensions(), (512, 512));
}

#[test]
fn test_first_frame_preview_returns_static_for_animated_gif() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    let image = load_bundled_image(
        &image_cache,
        &asset_cache,
        "numbers-1000ms.gif",
        Vector2I::new(16, 16),
        FitType::Contain,
        AnimatedImageBehavior::FirstFramePreview,
    );

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got animated image!");
    };
    assert_eq!(image.img.dimensions(), (16, 16));
}

#[test]
fn test_first_frame_preview_keeps_full_animation_in_asset_cache() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    for path in ["numbers-1000ms.gif", "animated.webp"] {
        let image = load_bundled_image(
            &image_cache,
            &asset_cache,
            path,
            Vector2I::new(16, 16),
            FitType::Contain,
            AnimatedImageBehavior::FirstFramePreview,
        );

        assert!(matches!(image.as_ref(), Image::Static(_)));

        let asset: AssetState<ImageType> = asset_cache.load_asset(AssetSource::Bundled { path });
        let AssetState::Loaded { data } = asset else {
            panic!("Animated asset should be available immediately!");
        };
        assert!(matches!(data.as_ref(), ImageType::AnimatedBitmap { .. }));
    }
}

#[test]
fn test_first_frame_preview_returns_static_for_animated_webp() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    let image = load_bundled_image(
        &image_cache,
        &asset_cache,
        "animated.webp",
        Vector2I::new(16, 16),
        FitType::Contain,
        AnimatedImageBehavior::FirstFramePreview,
    );

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got animated image!");
    };
    assert_eq!(image.img.dimensions(), (16, 16));
}

#[test]
fn test_full_animation_still_returns_animated_for_gif_and_webp() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    for path in ["numbers-1000ms.gif", "animated.webp"] {
        let image = load_bundled_image(
            &image_cache,
            &asset_cache,
            path,
            Vector2I::new(16, 16),
            FitType::Contain,
            AnimatedImageBehavior::FullAnimation,
        );

        let Image::Animated(image) = image.as_ref() else {
            panic!("Expected animated image but got static image!");
        };
        assert!(image.frames.len() > 1);
    }
}

#[test]
fn test_first_frame_preview_does_not_regress_static_formats() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    let image = load_bundled_image(
        &image_cache,
        &asset_cache,
        "local.png",
        Vector2I::new(16, 16),
        FitType::Contain,
        AnimatedImageBehavior::FirstFramePreview,
    );

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got animated image!");
    };
    assert_eq!(image.img.dimensions(), (16, 16));
}

#[test]
fn test_preview_and_full_animation_requests_do_not_collide_in_rendered_image_cache() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let bounds = Vector2I::new(16, 16);

    let preview = load_bundled_image(
        &image_cache,
        &asset_cache,
        "numbers-1000ms.gif",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FirstFramePreview,
    );
    let full = load_bundled_image(
        &image_cache,
        &asset_cache,
        "numbers-1000ms.gif",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );
    let preview_again = load_bundled_image(
        &image_cache,
        &asset_cache,
        "numbers-1000ms.gif",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FirstFramePreview,
    );
    let full_again = load_bundled_image(
        &image_cache,
        &asset_cache,
        "numbers-1000ms.gif",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );

    assert!(matches!(preview.as_ref(), Image::Static(_)));
    assert!(matches!(full.as_ref(), Image::Animated(_)));
    assert!(Rc::ptr_eq(&preview, &preview_again));
    assert!(Rc::ptr_eq(&full, &full_again));
    assert!(!Rc::ptr_eq(&preview, &full));
}

#[test]
fn test_svg_text_rasterizes_with_loaded_system_fonts() {
    let image_type = ImageType::try_from_bytes(
        br##"<svg width="160" height="40" viewBox="0 0 160 40" xmlns="http://www.w3.org/2000/svg">
  <text x="10" y="24" font-size="20" fill="#000000">Zap</text>
</svg>
"##,
    )
    .expect("SVG should parse");
    let ImageType::Svg { svg } = &image_type else {
        panic!("Expected SVG image type");
    };
    let font_family = svg
        .fontdb()
        .faces()
        .flat_map(|face| face.families.iter().map(|(family, _)| family.as_str()))
        .find(|family| {
            matches!(
                *family,
                "Arial"
                    | "Helvetica"
                    | "Inter"
                    | "DejaVu Sans"
                    | "Liberation Sans"
                    | "Noto Sans"
                    | "Cantarell"
                    | "Segoe UI"
            )
        })
        .or_else(|| {
            svg.fontdb()
                .faces()
                .find_map(|face| face.families.first().map(|(family, _)| family.as_str()))
        })
        .expect("System fonts should be loaded");

    let svg = format!(
        "<svg width=\"160\" height=\"40\" viewBox=\"0 0 160 40\" xmlns=\"http://www.w3.org/2000/svg\">\
  <text x=\"10\" y=\"24\" font-family=\"{font_family}\" font-size=\"20\" fill=\"#000000\">Zap</text>\
</svg>"
    );

    let image_type =
        ImageType::try_from_bytes(svg.as_bytes()).expect("SVG with installed font should parse");
    let image = image_type
        .to_image(
            Vector2I::new(160, 40),
            FitType::Contain,
            true,
            AnimatedImageBehavior::FullAnimation,
        )
        .expect("SVG should rasterize");
    let Image::Static(image) = image else {
        panic!("Expected static image");
    };

    assert!(image
        .rgba_bytes()
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 0));
}

#[test]
fn test_svg_text_rasterizes_with_bundled_sans_serif_fallback() {
    let image_type = ImageType::try_from_bytes(
        br##"<svg width="160" height="40" viewBox="0 0 160 40" xmlns="http://www.w3.org/2000/svg">
  <text x="10" y="24" font-family="sans-serif" font-size="20" fill="#000000">Zap</text>
</svg>
"##,
    )
    .expect("SVG should parse");
    let ImageType::Svg { svg } = &image_type else {
        panic!("Expected SVG image type");
    };

    assert!(svg.fontdb().faces().any(|face| {
        face.families
            .iter()
            .any(|(family, _)| family.as_str() == "Roboto")
    }));

    let image = image_type
        .to_image(
            Vector2I::new(160, 40),
            FitType::Contain,
            true,
            AnimatedImageBehavior::FullAnimation,
        )
        .expect("SVG should rasterize");
    let Image::Static(image) = image else {
        panic!("Expected static image");
    };

    assert!(image
        .rgba_bytes()
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 0));
}

#[test]
fn test_evict_image_drops_arc_for_resized_bysize() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let source = AssetSource::Bundled { path: "local.png" };

    // Request the image at a smaller size than its 1024x1024 source, which forces a resize
    // and allocates a fresh Arc<StaticImage> not shared with AssetCache.
    let bounds = Vector2I::new(64, 64);
    let weak = {
        let image = image_cache.image(
            source.clone(),
            bounds,
            FitType::Cover,
            AnimatedImageBehavior::FullAnimation,
            CacheOption::BySize,
            None,
            &asset_cache,
        );
        let AssetState::Loaded { data: image } = image else {
            panic!("Bundled asset should be available immediately!");
        };
        let Image::Static(arc) = image.as_ref() else {
            panic!("Expected static image!");
        };
        Arc::downgrade(arc)
        // The local Rc<Image> clone is dropped here; only ImageCache holds the entry now.
    };

    assert_eq!(
        weak.strong_count(),
        1,
        "ImageCache should be the sole strong holder after the caller drops its Rc clone"
    );

    // Evicting from ImageCache should make the Arc releasable by TextureCache.
    image_cache.evict_image(&source);
    assert_eq!(
        weak.strong_count(),
        0,
        "After evict_image, the resized Arc should have no strong holders (cascade invariant)"
    );
}

#[test]
fn test_evict_size_drops_arc_only_for_targeted_entry() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let source = AssetSource::Bundled { path: "local.png" };

    // Cache the same asset at two distinct sizes.
    let small_bounds = Vector2I::new(32, 32);
    let large_bounds = Vector2I::new(256, 256);

    let weak_small = {
        let image = image_cache.image(
            source.clone(),
            small_bounds,
            FitType::Cover,
            AnimatedImageBehavior::FullAnimation,
            CacheOption::BySize,
            None,
            &asset_cache,
        );
        let AssetState::Loaded { data: image } = image else {
            panic!("Bundled asset should be available immediately!");
        };
        let Image::Static(arc) = image.as_ref() else {
            panic!("Expected static image!");
        };
        Arc::downgrade(arc)
    };

    let weak_large = {
        let image = image_cache.image(
            source.clone(),
            large_bounds,
            FitType::Cover,
            AnimatedImageBehavior::FullAnimation,
            CacheOption::BySize,
            None,
            &asset_cache,
        );
        let AssetState::Loaded { data: image } = image else {
            panic!("Bundled asset should be available immediately!");
        };
        let Image::Static(arc) = image.as_ref() else {
            panic!("Expected static image!");
        };
        Arc::downgrade(arc)
    };

    assert_eq!(weak_small.strong_count(), 1);
    assert_eq!(weak_large.strong_count(), 1);

    // Evict only the small size entry.
    image_cache.evict_size(
        &source,
        small_bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
    );

    assert_eq!(
        weak_small.strong_count(),
        0,
        "Small size Arc should have no strong holders after evict_size"
    );
    assert_eq!(
        weak_large.strong_count(),
        1,
        "Large size Arc should remain alive; only the small size was evicted"
    );
}

#[test]
fn test_svg_image_size_returns_intrinsic_dimensions() {
    let image_type = ImageType::try_from_bytes(
        br##"<svg width="160" height="40" viewBox="0 0 160 40" xmlns="http://www.w3.org/2000/svg"></svg>"##,
    )
    .expect("SVG should parse");

    assert_eq!(image_type.image_size(), Some(Vector2I::new(160, 40)));
}

#[test]
fn test_respects_max_dimensions_for_cacheoption_bysize() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();

    let bounds = Vector2I::new(768, 768);

    let image = image_cache.image(
        AssetSource::Bundled { path: "local.png" },
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::BySize,
        None,
        &asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };
    // Assert that the image gets resized to match the provided bounds.
    assert_eq!(image.img.dimensions(), (768, 768));

    let image = image_cache.image(
        AssetSource::Bundled { path: "local.png" },
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::BySize,
        Some(512),
        &asset_cache,
    );
    let AssetState::Loaded { data: image } = image else {
        panic!("Bundled asset should be available immediately!");
    };

    let Image::Static(image) = image.as_ref() else {
        panic!("Expected static image but got dynamic image!");
    };
    // Assert that, when we specify a max dimension of 512, the image is resized accordingly.
    assert_eq!(image.img.dimensions(), (512, 512));
}

#[test]
fn test_caches_svg_rendered_at_intrinsic_size() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let bounds = Vector2I::new(20, 10);

    let image = load_bundled_image(
        &image_cache,
        &asset_cache,
        "cache-test.svg",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );
    let image_again = load_bundled_image(
        &image_cache,
        &asset_cache,
        "cache-test.svg",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );

    assert!(Rc::ptr_eq(&image, &image_again));
}

#[test]
fn cloned_image_cache_evicts_shared_rendered_images() {
    let image_cache = ImageCache::new();
    let evicting_image_cache = image_cache.clone();
    let asset_cache = AssetCache::new(
        Box::new(Assets),
        image_cache.clone(),
        Foreground::test().into(),
        Background::default().into(),
    );
    let source = AssetSource::Bundled {
        path: "cache-test.svg",
    };
    let bounds = Vector2I::new(20, 10);
    let image = load_bundled_image(
        &image_cache,
        &asset_cache,
        "cache-test.svg",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );

    evicting_image_cache.evict_image(&source);

    let image_after_eviction = load_bundled_image(
        &image_cache,
        &asset_cache,
        "cache-test.svg",
        bounds,
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );
    assert!(!Rc::ptr_eq(&image, &image_after_eviction));
}

#[test]
fn test_different_fit_types_do_not_collide_in_rendered_image_cache() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let bounds = Vector2I::new(8, 8);

    let cover = load_bundled_image(
        &image_cache,
        &asset_cache,
        "fit-test.rgba",
        bounds,
        FitType::Cover,
        AnimatedImageBehavior::FullAnimation,
    );
    let stretch = load_bundled_image(
        &image_cache,
        &asset_cache,
        "fit-test.rgba",
        bounds,
        FitType::Stretch,
        AnimatedImageBehavior::FullAnimation,
    );
    assert!(!Rc::ptr_eq(&cover, &stretch));
    let Image::Static(cover) = cover.as_ref() else {
        panic!("Expected static image but got animated image!");
    };
    let Image::Static(stretch) = stretch.as_ref() else {
        panic!("Expected static image but got animated image!");
    };
    assert_eq!(cover.img.dimensions(), (8, 8));
    assert_eq!(stretch.img.dimensions(), (8, 8));
    assert_ne!(cover.rgba_bytes(), stretch.rgba_bytes());
}

/// #876: `bytes_per_pixel * width * height` used to be computed in `u32` and
/// overflowed (a panic with overflow checks, a silent wrap without them).
#[test]
fn expected_byte_count_does_not_overflow_u32() {
    assert_eq!(CustomImageFormat::Rgba.expected_byte_count(4, 2), Some(32));
    assert_eq!(CustomImageFormat::Rgb.expected_byte_count(4, 2), Some(24));
    assert_eq!(
        CustomImageFormat::Rgba.expected_byte_count(0, u32::MAX),
        Some(0)
    );

    // 4 * 65536 * 65536 wraps to exactly 0 in u32.
    #[cfg(target_pointer_width = "64")]
    assert_eq!(
        CustomImageFormat::Rgba.expected_byte_count(65_536, 65_536),
        Some(1 << 34)
    );
    // 4 * (2^32 - 1)^2 does not even fit in a u64.
    assert_eq!(
        CustomImageFormat::Rgba.expected_byte_count(u32::MAX, u32::MAX),
        None
    );
}

/// #876: the exact width from the fuzz crash, with no pixel data, is an error
/// rather than an arithmetic panic.
#[test]
fn prepend_custom_header_rejects_overflowing_dimensions() {
    let result =
        CustomImageHeader::prepend_custom_header(vec![], 2_555_555_558, 1, CustomImageFormat::Rgba);
    assert!(
        matches!(
            result,
            Err(CustomHeaderCreationError::ExpectedDataSizeMismatch {
                actual_bytes: 0,
                ..
            }) | Err(CustomHeaderCreationError::DimensionsTooLarge { .. })
        ),
        "{result:?}"
    );

    // A wrapped-to-zero size must not be accepted with an empty payload.
    let result =
        CustomImageHeader::prepend_custom_header(vec![], 65_536, 65_536, CustomImageFormat::Rgba);
    assert!(result.is_err(), "{result:?}");

    let result = CustomImageHeader::prepend_custom_header(
        vec![],
        u32::MAX,
        u32::MAX,
        CustomImageFormat::Rgb,
    );
    assert!(result.is_err(), "{result:?}");
}

/// #876: a `warp-img:` header is reachable from any image payload a program
/// prints (e.g. iTerm inline images), so its dimensions are untrusted too.
#[test]
fn custom_header_parse_rejects_overflowing_dimensions() {
    // 4 * 65536 * 65536 wraps to 0 in u32, which used to match an empty body.
    let err = CustomImageHeader::try_from_bytes(b"warp-img:rgba:65536:65536:")
        .err()
        .expect("an image whose size wraps to zero must be rejected");
    assert!(
        matches!(
            err,
            CustomHeaderParsingError::ExpectedDataSizeMismatch {
                actual_bytes: 0,
                ..
            } | CustomHeaderParsingError::DimensionsTooLarge { .. }
        ),
        "{err:?}"
    );

    let err = CustomImageHeader::try_from_bytes(b"warp-img:rgba:4294967295:4294967295:")
        .err()
        .expect("u32::MAX x u32::MAX must be rejected");
    assert!(
        matches!(err, CustomHeaderParsingError::DimensionsTooLarge { .. }),
        "{err:?}"
    );

    let (header, body) =
        CustomImageHeader::try_from_bytes(b"warp-img:rgb:1:1:\x01\x02\x03").expect("valid");
    assert_eq!((header.width, header.height), (1, 1));
    assert_eq!(body, b"\x01\x02\x03");
}

/// A GIF header declaring a `width` x `height` logical screen and nothing else.
fn gif_header(width: u16, height: u16) -> Vec<u8> {
    let mut data = b"GIF89a".to_vec();
    data.extend_from_slice(&width.to_le_bytes());
    data.extend_from_slice(&height.to_le_bytes());
    data.extend_from_slice(&[0, 0, 0]);
    data
}

/// A WebP whose VP8X chunk declares a `width` x `height` canvas and nothing else.
fn webp_vp8x_header(width: u32, height: u32) -> Vec<u8> {
    let mut data = b"RIFF".to_vec();
    data.extend_from_slice(&22u32.to_le_bytes());
    data.extend_from_slice(b"WEBPVP8X");
    data.extend_from_slice(&10u32.to_le_bytes());
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    data.extend_from_slice(&(height - 1).to_le_bytes()[..3]);
    data
}

/// #880: a tiny GIF header claiming a 65535x65535 screen must be rejected
/// before the decoder allocates a ~17 GB canvas.
#[test]
fn gif_decompression_bomb_header_is_rejected() {
    let data = gif_header(u16::MAX, u16::MAX);
    assert!(<ImageType as Asset>::try_from_bytes(&data).is_err());
    assert!(ImageType::dimensions_from_bytes(&data).is_err());
}

/// #880: same for a WebP canvas past the dimension limit.
#[test]
fn webp_oversized_canvas_is_rejected() {
    let data = webp_vp8x_header(MAX_DECODED_IMAGE_DIMENSION + 1, 1);
    assert!(<ImageType as Asset>::try_from_bytes(&data).is_err());
    assert!(ImageType::dimensions_from_bytes(&data).is_err());
}

/// #880: a frame inside the dimension limit is still bounded by total bytes.
#[test]
fn decoded_size_check_enforces_byte_budget() {
    assert!(check_decoded_size(MAX_DECODED_IMAGE_DIMENSION, MAX_DECODED_IMAGE_DIMENSION).is_err());
    assert!(check_decoded_size(MAX_DECODED_IMAGE_DIMENSION + 1, 1).is_err());
    assert!(check_decoded_size(1, MAX_DECODED_IMAGE_DIMENSION + 1).is_err());
    assert!(check_decoded_size(1024, 1024).is_ok());
}

/// #880: size from the header alone matches the decoded size for real images.
#[test]
fn header_dimensions_match_decoded_dimensions() {
    let gif = include_bytes!("../../warpui/examples/assets/numbers-1000ms.gif");
    let from_header = ImageType::dimensions_from_bytes(gif).unwrap();
    let decoded = <ImageType as Asset>::try_from_bytes(gif)
        .unwrap()
        .image_size()
        .unwrap();
    assert_eq!(from_header, decoded);

    let webp = include_bytes!("../test_data/animated.webp");
    let from_header = ImageType::dimensions_from_bytes(webp).unwrap();
    let decoded = <ImageType as Asset>::try_from_bytes(webp)
        .unwrap()
        .image_size()
        .unwrap();
    assert_eq!(from_header, decoded);
}

/// #880: SVGs used to report 0 bytes and so bypassed the asset cache budget.
#[test]
fn svg_size_is_estimated_from_rasterized_dimensions() {
    assert_eq!(svg_rasterized_byte_estimate(20., 10.), 800);
    assert_eq!(svg_rasterized_byte_estimate(0.5, 0.5), 4);
    assert_eq!(svg_rasterized_byte_estimate(-5., 10.), 0);
    assert_eq!(svg_rasterized_byte_estimate(f32::NAN, 10.), 0);
    assert!(svg_rasterized_byte_estimate(f32::MAX, f32::MAX) >= MAX_DECODED_IMAGE_BYTES as usize);

    let svg = br##"<svg width="20" height="10" viewBox="0 0 20 10" xmlns="http://www.w3.org/2000/svg"><rect width="20" height="10" fill="#ffffff"/></svg>"##;
    let image = <ImageType as Asset>::try_from_bytes(svg).unwrap();
    assert!(matches!(image, ImageType::Svg { .. }));
    assert_eq!(image.size_in_bytes(), 20 * 10 * 4);
}

#[test]
fn test_oversized_svg_is_rejected_at_parse_time() {
    // 10_000 x 10_000 x 4 = 400 MB, over MAX_DECODED_IMAGE_BYTES.
    let huge = br##"<svg width="10000" height="10000" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"##;
    assert!(ImageType::try_from_bytes(huge).is_err());
    let small = br##"<svg width="20" height="10" xmlns="http://www.w3.org/2000/svg"><rect width="1" height="1"/></svg>"##;
    assert!(ImageType::try_from_bytes(small).is_ok());
}

fn deferred_big_svg(
    image_cache: &ImageCache,
    asset_cache: &AssetCache,
    side: i32,
) -> DeferredImage {
    image_cache.image_deferred(
        AssetSource::Bundled {
            path: "big-test.svg",
        },
        Vector2I::new(side, side),
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::BySize,
        None,
        asset_cache,
    )
}

fn wait_for_deferred_big_svg(
    image_cache: &ImageCache,
    asset_cache: &AssetCache,
    side: i32,
) -> Rc<Image> {
    for _ in 0..2000 {
        if let DeferredImage::State(AssetState::Loaded { data }) =
            deferred_big_svg(image_cache, asset_cache, side)
        {
            return data;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("background rasterisation never finished");
}

#[test]
fn deferred_large_svg_is_pending_then_ready_at_the_wanted_size() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    assert!(matches!(
        deferred_big_svg(&image_cache, &asset_cache, 300),
        DeferredImage::Pending { stale: None }
    ));
    let image = wait_for_deferred_big_svg(&image_cache, &asset_cache, 300);
    let Image::Static(image) = image.as_ref() else {
        panic!("expected a static image");
    };
    assert_eq!(image.size(), Vector2I::new(300, 300));

    // A new size shows the previous one while it is rasterised, then the exact size.
    match deferred_big_svg(&image_cache, &asset_cache, 400) {
        DeferredImage::Pending { stale: Some(stale) } => {
            assert_eq!(stale.from_bounds, Vector2I::new(300, 300));
            assert_eq!(stale.to_bounds, Vector2I::new(400, 400));
        }
        _ => panic!("expected pending with a stale image"),
    }
    let image = wait_for_deferred_big_svg(&image_cache, &asset_cache, 400);
    let Image::Static(image) = image.as_ref() else {
        panic!("expected a static image");
    };
    assert_eq!(image.size(), Vector2I::new(400, 400));
}

#[test]
fn small_images_are_never_deferred() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let deferred = image_cache.image_deferred(
        AssetSource::Bundled {
            path: "cache-test.svg",
        },
        Vector2I::new(40, 20),
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::BySize,
        None,
        &asset_cache,
    );
    assert!(matches!(
        deferred,
        DeferredImage::State(AssetState::Loaded { .. })
    ));
}

#[test]
fn evicting_while_pending_drops_the_stale_result() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    assert!(matches!(
        deferred_big_svg(&image_cache, &asset_cache, 300),
        DeferredImage::Pending { .. }
    ));
    image_cache.evict_image(&AssetSource::Bundled {
        path: "big-test.svg",
    });
    // Let the worker finish; its result must be discarded, not inserted.
    std::thread::sleep(std::time::Duration::from_millis(500));
    image_cache.drain_ready();
    assert!(image_cache.images.read().is_empty());
}

#[test]
fn rendered_sizes_per_asset_are_bounded() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    for side in 300..(300 + 2 * MAX_RENDERED_SIZES_PER_ASSET as i32) {
        wait_for_deferred_big_svg(&image_cache, &asset_cache, side);
    }
    let cache = image_cache.images.read();
    assert!(cache.values().all(|sizes| sizes.len() <= MAX_RENDERED_SIZES_PER_ASSET));
}

fn request(
    image_cache: &ImageCache,
    asset_cache: &AssetCache,
    path: &'static str,
    side: i32,
) -> DeferredImage {
    image_cache.image_deferred(
        AssetSource::Bundled { path },
        Vector2I::new(side, side),
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
        CacheOption::BySize,
        None,
        asset_cache,
    )
}

fn wait_until(mut done: impl FnMut() -> bool) {
    for _ in 0..2000 {
        if done() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    panic!("condition never became true");
}

fn is_failed(deferred: &DeferredImage) -> bool {
    matches!(deferred, DeferredImage::State(AssetState::FailedToLoad(_)))
}

#[test]
fn evict_then_restart_ignores_the_old_result() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    // The first job fails; evicting while it runs must make its result irrelevant.
    image_cache.test_hook.set(1);
    assert!(matches!(
        request(&image_cache, &asset_cache, "big-test.svg", 300),
        DeferredImage::Pending { .. }
    ));
    image_cache.evict_image(&AssetSource::Bundled {
        path: "big-test.svg",
    });
    image_cache.test_hook.set(0);
    wait_until(|| image_cache.inflight.load(Ordering::Acquire) == 0);
    // New generation for the same key.
    assert!(matches!(
        request(&image_cache, &asset_cache, "big-test.svg", 300),
        DeferredImage::Pending { .. }
    ));
    let image = wait_for_deferred_big_svg(&image_cache, &asset_cache, 300);
    assert!(matches!(image.as_ref(), Image::Static(_)));
    assert!(image_cache.failed.borrow().is_empty());
}

#[test]
fn inflight_cap_defers_the_third_request_until_a_slot_frees() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    image_cache.test_hook.set(3);
    for side in [300, 301] {
        assert!(matches!(
            request(&image_cache, &asset_cache, "big-test.svg", side),
            DeferredImage::Pending { .. }
        ));
    }
    assert_eq!(image_cache.inflight.load(Ordering::Acquire), 2);
    assert!(matches!(
        request(&image_cache, &asset_cache, "big-test.svg", 302),
        DeferredImage::Pending { .. }
    ));
    assert_eq!(image_cache.pending.borrow().len(), 2, "third must not start");
    assert_eq!(image_cache.inflight.load(Ordering::Acquire), 2);
    image_cache.test_hook.set(0);
    wait_until(|| image_cache.inflight.load(Ordering::Acquire) == 0);
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 302);
}

#[test]
fn failures_and_panics_are_remembered_not_retried() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    for (hook, side) in [(1, 300), (2, 301)] {
        image_cache.test_hook.set(hook);
        assert!(matches!(
            request(&image_cache, &asset_cache, "big-test.svg", side),
            DeferredImage::Pending { .. }
        ));
        wait_until(|| is_failed(&request(&image_cache, &asset_cache, "big-test.svg", side)));
        // Healthy again, but the failed key is not retried.
        image_cache.test_hook.set(0);
        assert!(is_failed(&request(
            &image_cache,
            &asset_cache,
            "big-test.svg",
            side
        )));
        assert!(image_cache.pending.borrow().is_empty());
    }
    wait_until(|| image_cache.live_threads.load(Ordering::Acquire) == 0);
    assert_eq!(image_cache.inflight.load(Ordering::Acquire), 0);
    // Other sizes still work after a panic.
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 305);
}

#[test]
fn byte_budget_drops_the_least_recently_used_size() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    image_cache.test_budget.set(1_500_000);
    for side in [300, 301, 302, 303] {
        wait_for_deferred_big_svg(&image_cache, &asset_cache, side);
    }
    // Touch 300 so that 301 is now the least recently used.
    assert!(matches!(
        request(&image_cache, &asset_cache, "big-test.svg", 300),
        DeferredImage::State(AssetState::Loaded { .. })
    ));
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 304);
    let cached: Vec<i32> = {
        let cache = image_cache.images.read();
        let mut sides: Vec<i32> = cache
            .values()
            .flat_map(|sizes| sizes.keys().map(|k| k.bounds.x()))
            .collect();
        sides.sort();
        sides
    };
    assert_eq!(cached, vec![300, 302, 303, 304]);
    let total: usize = image_cache.used.borrow().values().map(|e| e.bytes).sum();
    assert_eq!(total, image_cache.total_bytes.get());
    assert!(total <= 1_500_000);
}

#[test]
fn evict_image_clears_all_bookkeeping() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 300);
    image_cache.test_hook.set(3);
    assert!(matches!(
        request(&image_cache, &asset_cache, "big-test.svg", 301),
        DeferredImage::Pending { .. }
    ));
    image_cache.evict_image(&AssetSource::Bundled {
        path: "big-test.svg",
    });
    assert!(image_cache.images.read().is_empty());
    assert!(image_cache.used.borrow().is_empty());
    assert!(image_cache.pending.borrow().is_empty());
    assert!(image_cache.failed.borrow().is_empty());
    assert_eq!(image_cache.total_bytes.get(), 0);
    wait_until(|| image_cache.inflight.load(Ordering::Acquire) == 0);
}

#[test]
fn evict_size_keeps_the_bookkeeping_consistent() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 300);
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 301);
    image_cache.evict_size(
        &AssetSource::Bundled {
            path: "big-test.svg",
        },
        Vector2I::new(300, 300),
        FitType::Contain,
        AnimatedImageBehavior::FullAnimation,
    );
    assert_eq!(image_cache.used.borrow().len(), 1);
    let total: usize = image_cache.used.borrow().values().map(|e| e.bytes).sum();
    assert_eq!(total, image_cache.total_bytes.get());
}

#[test]
fn large_bitmap_scaling_is_deferred() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    let source = AssetSource::Bundled {
        path: "heavy.rgba",
    };
    let _: AssetState<ImageType> = asset_cache.load_asset(source);
    let mut image = None;
    let mut saw_pending = false;
    for _ in 0..2000 {
        match request(&image_cache, &asset_cache, "heavy.rgba", 100) {
            DeferredImage::Pending { .. } => saw_pending = true,
            DeferredImage::State(AssetState::Loaded { data }) => {
                image = Some(data);
                break;
            }
            DeferredImage::State(AssetState::FailedToLoad(err)) => panic!("failed: {err}"),
            DeferredImage::State(_) => {}
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(saw_pending, "a 1M pixel bitmap must be scaled off-thread");
    let image = image.expect("scaled image");
    let Image::Static(image) = image.as_ref() else {
        panic!("expected a static image");
    };
    assert_eq!(image.size(), Vector2I::new(100, 100));
}

#[test]
fn a_stuck_rasterisation_times_out_and_frees_its_slot() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    image_cache.test_deadline_ms.set(50);
    image_cache.test_hook.set(3);
    assert!(matches!(
        request(&image_cache, &asset_cache, "big-test.svg", 300),
        DeferredImage::Pending { .. }
    ));
    std::thread::sleep(std::time::Duration::from_millis(80));
    assert!(is_failed(&request(
        &image_cache,
        &asset_cache,
        "big-test.svg",
        300
    )));
    // The slot is free again while the stuck thread is still running.
    assert_eq!(image_cache.inflight.load(Ordering::Acquire), 0);
    image_cache.test_hook.set(0);
    wait_for_deferred_big_svg(&image_cache, &asset_cache, 301);
    // The stuck thread finishing later must not release its slot a second time.
    wait_until(|| image_cache.live_threads.load(Ordering::Acquire) == 0);
    assert_eq!(image_cache.inflight.load(Ordering::Acquire), 0);
    assert!(is_failed(&request(
        &image_cache,
        &asset_cache,
        "big-test.svg",
        300
    )));
}

#[test]
fn an_svg_with_too_many_nodes_is_refused() {
    let asset_cache = new_asset_cache();
    let image_cache = ImageCache::new();
    image_cache.test_max_svg_nodes.set(2000);
    wait_until(|| {
        matches!(
            request(&image_cache, &asset_cache, "many-nodes.svg", 300),
            DeferredImage::State(AssetState::FailedToLoad(_))
        )
    });
}
