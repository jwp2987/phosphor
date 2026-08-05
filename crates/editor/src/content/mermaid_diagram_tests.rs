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
