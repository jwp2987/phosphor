use warpui::assets::asset_cache::{AssetCache, AssetSource, AssetState};
use warpui::image_cache::ImageType;
use warpui::text_layout::LayoutCache;
use warpui::App;

use super::*;
use crate::render::layout::TextLayout;
use crate::render::model::test_utils::TEST_STYLES;

fn mermaid_block_spacing() -> BlockSpacing {
    TEST_STYLES.block_spacings.from_block_style(
        &crate::content::text::BufferBlockStyle::CodeBlock {
            code_block_type: crate::content::text::CodeBlockType::Mermaid,
        },
    )
}

#[test]
fn loading_mermaid_layout_uses_default_height() {
    App::test((), |app| async move {
        app.read(|ctx| {
            let source = "graph TD\nA[Start] --> B[Finish]\n";
            let layout_cache = LayoutCache::new();
            let text_layout = TextLayout::new(
                &layout_cache,
                ctx.font_cache().text_layout_system(),
                &TEST_STYLES,
                800.,
            );
            let (_asset_source, config) =
                mermaid_diagram_layout(source, &text_layout, mermaid_block_spacing(), ctx);
            let expected_height = TEST_STYLES.base_line_height()
                * DEFAULT_MERMAID_HEIGHT_LINE_MULTIPLIER.into_pixels();

            assert!((config.height.as_f32() - expected_height.as_f32()).abs() < 0.5);
        });
    })
}

/// An editor whose element has never laid out reports a zero-width viewport, and
/// subtracting the code block's horizontal insets from that leaves a negative
/// width. Scaling a loaded diagram's intrinsic size into that width used to give
/// the block a negative height, which made it invisible to every height-keyed
/// lookup in the layout sum tree (`block_at_height`, hit testing, autoscroll).
#[test]
fn unmeasured_viewport_keeps_positive_height_for_loaded_mermaid() {
    App::test((), |app| async move {
        let source = "graph TD\nA[Start] --> B[Finish]\n";
        let asset_source = mermaid_asset_source(source);

        // Render the SVG first, so the layout below takes the loaded-intrinsic-size
        // path rather than the loading placeholder path.
        let mermaid_load = app.read(|ctx| {
            let asset_cache = AssetCache::as_ref(ctx);
            match asset_cache.load_asset::<ImageType>(asset_source.clone()) {
                AssetState::Loading { handle } => handle.when_loaded(asset_cache),
                AssetState::Loaded { .. } => None,
                AssetState::Evicted => panic!("Mermaid asset should not be evicted during test"),
                AssetState::FailedToLoad(err) => {
                    panic!("Mermaid asset should load successfully: {err}")
                }
            }
        });
        if let Some(future) = mermaid_load {
            future.await;
        }

        app.read(|ctx| {
            let layout_cache = LayoutCache::new();
            let text_layout = TextLayout::new(
                &layout_cache,
                ctx.font_cache().text_layout_system(),
                &TEST_STYLES,
                0.,
            );
            let (_asset_source, config) =
                mermaid_diagram_layout(source, &text_layout, mermaid_block_spacing(), ctx);

            assert!(
                config.width.as_f32() >= 0.,
                "mermaid block width should never be negative, got {}",
                config.width
            );
            assert!(
                config.height.as_f32() > 0.,
                "mermaid block height should stay positive when the viewport is unmeasured, got {}",
                config.height
            );
        });
    })
}

// Adapted from Warp's `failed_mermaid_layout_uses_compact_height`. Warp injects
// an already-failed `AssetSource` into `mermaid_diagram_config`; the fork fuses
// that into `mermaid_diagram_layout` (which takes a source string and always
// starts an async load), so the failed branch is exercised directly through the
// height-multiplier helper that implements it.
#[test]
fn failed_mermaid_layout_uses_compact_height() {
    App::test((), |app| async move {
        app.read(|ctx| {
            let asset_source = AssetSource::Raw {
                id: "missing-mermaid-test-asset".to_string(),
            };
            let asset_cache = AssetCache::as_ref(ctx);
            assert!(matches!(
                asset_cache.load_asset::<ImageType>(asset_source.clone()),
                AssetState::FailedToLoad(_)
            ));

            assert_eq!(
                mermaid_diagram_fallback_height_line_multiplier(&asset_source, ctx),
                FAILED_MERMAID_HEIGHT_LINE_MULTIPLIER,
                "a failed mermaid diagram collapses to the compact fallback height"
            );
        });
    })
}

#[test]
fn mermaid_asset_source_renders_frontmatter_formatting_directives() {
    let source = r##"---
config:
  theme: base
  themeVariables:
    primaryColor: "#ff0000"
  fontFamily: Inter
  fontSize: 18px
  flowchart:
    curve: linear
    nodeSpacing: 80
---
flowchart TD
  A[Start] --> B[Done]
"##;

    let AssetSource::Async { fetch, .. } = mermaid_asset_source(source) else {
        panic!("expected Mermaid diagrams to be async assets");
    };
    let bytes = match futures_lite::future::block_on(fetch()) {
        Ok(bytes) => bytes,
        Err(error) => panic!("expected frontmatter directives to render: {error:#}"),
    };
    let svg = match String::from_utf8(bytes.to_vec()) {
        Ok(svg) => svg,
        Err(error) => panic!("expected Mermaid SVG to be valid UTF-8: {error}"),
    };

    assert!(svg.contains("<svg "));
    assert!(svg.contains(r##"fill="#ff0000""##));
    assert!(svg.contains(r#"font-family="Inter""#));
}
