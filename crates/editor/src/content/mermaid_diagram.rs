use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

use bytes::Bytes;
use warpui::{
    AppContext, SingletonEntity,
    assets::asset_cache::{AssetCache, AssetSource, AssetState, AsyncAssetId, AsyncAssetType},
    image_cache::ImageType,
    units::{IntoPixels, Pixels},
};

use crate::render::{
    layout::TextLayout,
    model::{BlockSpacing, ImageBlockConfig},
};

const DEFAULT_MERMAID_HEIGHT_LINE_MULTIPLIER: f32 = 10.0;
const FAILED_MERMAID_HEIGHT_LINE_MULTIPLIER: f32 = 2.0;

struct MermaidDiagramAsset;

impl AsyncAssetType for MermaidDiagramAsset {}

pub fn mermaid_asset_source(source: &str) -> AssetSource {
    let source = source.to_string();
    let mut hasher = DefaultHasher::new();
    source.hash(&mut hasher);
    let id = format!("configured:{:x}", hasher.finish());
    let fetch_source = source.clone();

    AssetSource::Async {
        id: AsyncAssetId::new::<MermaidDiagramAsset>(id),
        fetch: Arc::new(move || {
            let source = fetch_source.clone();
            Box::pin(async move {
                mermaid_to_svg::render_mermaid_to_svg(&source, None)
                    .map(|svg| Bytes::from(svg.into_bytes()))
                    .map_err(Into::into)
            })
        }),
    }
}

pub fn mermaid_diagram_layout(
    source: &str,
    layout: &TextLayout,
    spacing: BlockSpacing,
    app: &AppContext,
) -> (AssetSource, ImageBlockConfig) {
    let asset_source = mermaid_asset_source(source);
    // The available width can be non-positive when the editor's viewport has not
    // been measured yet, or when the block's horizontal insets are wider than the
    // viewport. Clamp it so the block never claims a negative width.
    let max_width = (layout.max_width() - spacing.x_axis_offset()).max(Pixels::zero());
    let (width, height) = mermaid_diagram_size(&asset_source, max_width, app).unwrap_or_else(|| {
        // A diagram that failed to render collapses to a short placeholder
        // height instead of reserving the full loading-state height.
        let height = layout.rich_text_styles().base_line_height()
            * mermaid_diagram_fallback_height_line_multiplier(&asset_source, app).into_pixels();
        (max_width, height)
    });

    (
        asset_source,
        ImageBlockConfig {
            width,
            height,
            spacing,
        },
    )
}

/// The line-height multiplier for a diagram with no intrinsic size yet: a
/// short placeholder once it has failed to load, otherwise the taller
/// loading/default reservation.
fn mermaid_diagram_fallback_height_line_multiplier(asset_source: &AssetSource, app: &AppContext) -> f32 {
    let asset_cache = AssetCache::as_ref(app);
    match asset_cache.load_asset::<ImageType>(asset_source.clone()) {
        AssetState::FailedToLoad(_) => FAILED_MERMAID_HEIGHT_LINE_MULTIPLIER,
        AssetState::Loading { .. } | AssetState::Loaded { .. } | AssetState::Evicted => {
            DEFAULT_MERMAID_HEIGHT_LINE_MULTIPLIER
        }
    }
}

fn mermaid_diagram_size(
    asset_source: &AssetSource,
    max_width: Pixels,
    app: &AppContext,
) -> Option<(Pixels, Pixels)> {
    // Scaling the diagram's intrinsic size into a zero-width slot produces a block
    // of zero height, which is indistinguishable from "no block" to every
    // height-keyed lookup in the layout sum tree (hit testing, autoscroll,
    // `block_at_height`). Until the viewport reports a usable width, keep the
    // reserved placeholder height instead.
    if max_width <= Pixels::zero() {
        return None;
    }
    let asset_cache = AssetCache::as_ref(app);
    let AssetState::Loaded { data } = asset_cache.load_asset::<ImageType>(asset_source.clone())
    else {
        return None;
    };
    let ImageType::Svg { svg } = data.as_ref() else {
        return None;
    };
    let intrinsic_size = svg.size();
    let intrinsic_width = intrinsic_size.width();
    let intrinsic_height = intrinsic_size.height();
    if intrinsic_width <= 0. || intrinsic_height <= 0. {
        return None;
    }
    let width = Pixels::new(max_width.as_f32().min(intrinsic_width));
    let height = Pixels::new(width.as_f32() * intrinsic_height / intrinsic_width);
    Some((width, height))
}

#[cfg(test)]
#[path = "mermaid_diagram_tests.rs"]
mod tests;
