use crate::fonts::{canvas, RasterizedGlyph};
use crate::rendering::atlas::{self, AllocatedRegion, TextureId};
use crate::{fonts::SubpixelAlignment, rendering, scene::GlyphKey};
use anyhow::Result;
use ordered_float::OrderedFloat;
use pathfinder_geometry::rect::RectI;
use pathfinder_geometry::{
    rect::RectF,
    vector::{Vector2F, Vector2I},
};
use rustc_hash::FxHashMap;
use std::time::{Duration, Instant};

const ATLAS_SIZE: usize = 1024;

/// Default time a single frame may spend rasterising new glyphs before the rest wait for the next
/// frame (#1168 F6). Generous: a key's glyphs are cached or tiny, so only a first-seen screen of
/// emoji / CJK ever reaches it.
const DEFAULT_RASTER_BUDGET: Duration = Duration::from_millis(8);

/// Environment variable overriding the per-frame rasterisation budget in milliseconds. `0`
/// turns the cap off (the kill switch): every missing glyph is rasterised in the frame that
/// needs it, as before.
const RASTER_BUDGET_ENV: &str = "PHOSPHOR_GLYPH_RASTER_BUDGET_MS";

thread_local! {
    /// Set when the render in progress left glyphs unrasterised; the window layer takes it after
    /// drawing and asks for another frame (see [`take_deferred_redraw`]). Rendering is
    /// single-threaded per frame, so the flag is scoped to ONE window's render call: it is
    /// cleared by [`begin_window_render`] before the draw and taken right after it, so an early
    /// return (a failed draw) cannot leave it set for whichever window renders next (#1379).
    static DEFERRED_REDRAW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Starts one window's render: forgets any deferral flag a previous render left behind.
pub(crate) fn begin_window_render() {
    DEFERRED_REDRAW.with(|flag| flag.set(false));
}

/// Whether the render since [`begin_window_render`] deferred glyph rasterisation, clearing the
/// flag. The caller requests another frame (keeping the scene) when this is true.
pub(crate) fn take_deferred_redraw() -> bool {
    DEFERRED_REDRAW.with(|flag| flag.replace(false))
}

fn raster_budget_from_env() -> Option<Duration> {
    static BUDGET: std::sync::OnceLock<Option<Duration>> = std::sync::OnceLock::new();
    *BUDGET.get_or_init(|| match std::env::var(RASTER_BUDGET_ENV) {
        Ok(value) => match value.trim().parse::<u64>() {
            Ok(0) => None,
            Ok(ms) => Some(Duration::from_millis(ms)),
            Err(_) => Some(DEFAULT_RASTER_BUDGET),
        },
        Err(_) => Some(DEFAULT_RASTER_BUDGET),
    })
}

/// Callback to create a texture at a given size.
type CreateTextureCallback<'a, T> = dyn Fn(usize) -> T + 'a;

/// Callback to insert [`RasterizedGlyph`] at a region identified by [`AllocatedRegion`] into a
/// texture, `T`.
type InsertIntoTextureCallback<'a, T> = dyn Fn(AllocatedRegion, &RasterizedGlyph, &mut T) + 'a;

/// Callback to compute the bounds of a glyph when rasterized.
pub(crate) type GlyphRasterBoundsFn<'a> =
    dyn Fn(GlyphKey, Vector2F, &rendering::GlyphConfig) -> Result<RectI> + 'a;

/// Callback to rasterize a glyph.
pub(crate) type RasterizeGlyphFn<'a> = dyn Fn(
        GlyphKey,
        Vector2F,
        SubpixelAlignment,
        &rendering::GlyphConfig,
        canvas::RasterFormat,
    ) -> Result<RasterizedGlyph>
    + 'a;

/// A cache that caches glyphs in a texture atlas.  
pub struct GlyphCache<Texture> {
    textures: Vec<Texture>,
    /// `None` records a glyph with nothing to draw (zero-size raster bounds, e.g. a blank glyph
    /// such as a no-break space) so it is not re-measured on every frame.
    cache: FxHashMap<GlyphCacheKey, Option<GlyphTextureOffset>>,
    glyph_config: rendering::GlyphConfig,
    atlas_manager: atlas::Manager,
    /// Time one frame may spend rasterising new glyphs; `None` = unlimited.
    raster_budget: Option<Duration>,
    /// Rasterisation time spent and glyphs rasterised in the current frame.
    frame_raster_time: Duration,
    frame_rasterized: u32,
    /// A glyph was left for a later frame in the current frame.
    deferred: bool,
}

#[derive(Hash, PartialEq, Eq)]
struct GlyphCacheKey {
    glyph_key: GlyphKey,
    scale_factor: OrderedFloat<f32>,
    subpixel_alignment: SubpixelAlignment,
}

impl GlyphCacheKey {
    fn new(glyph_key: GlyphKey, scale_factor: f32, subpixel_alignment: SubpixelAlignment) -> Self {
        GlyphCacheKey {
            glyph_key,
            scale_factor: scale_factor.into(),
            subpixel_alignment,
        }
    }
}

/// A glyph within a texture atlas.
#[derive(Copy, Debug, Clone)]
pub(crate) struct GlyphTextureOffset {
    pub texture_id: TextureId,
    pub allocated_region: AllocatedRegion,
    pub raster_bounds: RectF,
    pub is_emoji: bool,
}

impl<Texture> GlyphCache<Texture> {
    pub(crate) fn new(glyph_config: rendering::GlyphConfig) -> Self {
        GlyphCache {
            textures: Vec::new(),
            cache: FxHashMap::default(),
            glyph_config,
            atlas_manager: atlas::Manager::new(ATLAS_SIZE),
            raster_budget: raster_budget_from_env(),
            frame_raster_time: Duration::ZERO,
            frame_rasterized: 0,
            deferred: false,
        }
    }

    /// Starts a new frame's rasterisation budget. Called once per frame by the renderers (via
    /// [`Self::update_config`]).
    fn begin_frame(&mut self) {
        self.frame_raster_time = Duration::ZERO;
        self.frame_rasterized = 0;
        self.deferred = false;
    }

    /// Whether the current frame left glyphs to be rasterised in a later one.
    #[cfg(test)]
    pub(crate) fn has_deferred(&self) -> bool {
        self.deferred
    }

    /// Whether this frame has used up its rasterisation budget. At least one glyph is always
    /// rasterised per frame so a backlog keeps shrinking and nothing stays blank for good.
    fn over_budget(&self) -> bool {
        self.frame_rasterized > 0
            && self
                .raster_budget
                .is_some_and(|budget| self.frame_raster_time >= budget)
    }

    pub(crate) fn update_config(&mut self, glyph_config: &rendering::GlyphConfig) {
        // If the glyph rendering configuration has changed, blow away the cache
        // and replace ourself with a new one.
        if *glyph_config != self.glyph_config {
            *self = GlyphCache::new(*glyph_config);
        }
        self.begin_frame();
    }

    /// Returns the texture identified by [`TextureId`].
    pub(crate) fn texture(&self, texture_id: &TextureId) -> Option<&Texture> {
        self.textures.get(texture_id.as_usize())
    }

    /// Returns a [`GlyphTextureOffset`] identified by [`GlyphKey`]. If the [`GlyphKey`] has not
    /// been previously cached, the glyph is rasterized and inserted into the texture via the
    /// `insert_into_texture` callback. If a new texture needs to be created (since a previous
    /// texture is now fill), the `create_texture` callback is called to construct a new texture
    /// atlas.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn get(
        &mut self,
        glyph_key: GlyphKey,
        scale_factor: f32,
        subpixel_alignment: SubpixelAlignment,
        create_texture: &CreateTextureCallback<'_, Texture>,
        insert_into_texture: &InsertIntoTextureCallback<'_, Texture>,
        raster_bounds_fn: &GlyphRasterBoundsFn<'_>,
        rasterize_glyph_fn: &RasterizeGlyphFn<'_>,
    ) -> Result<Option<GlyphTextureOffset>> {
        let cache_key = GlyphCacheKey::new(glyph_key, scale_factor, subpixel_alignment);

        match self.cache.get(&cache_key) {
            None => {
                if self.over_budget() {
                    // Not cached, so the next frame retries it; the caller is told to draw
                    // again and the glyph is simply absent from this frame.
                    self.deferred = true;
                    DEFERRED_REDRAW.with(|flag| flag.set(true));
                    return Ok(None);
                }
                let started = Instant::now();
                let bounds =
                    raster_bounds_fn(glyph_key, Vector2F::splat(scale_factor), &self.glyph_config)?;

                if bounds.size() == Vector2I::zero() {
                    self.cache.insert(cache_key, None);
                    return Ok(None);
                }

                let rasterized_glyph = rasterize_glyph_fn(
                    glyph_key,
                    Vector2F::splat(scale_factor),
                    subpixel_alignment,
                    &self.glyph_config,
                    crate::fonts::canvas::RasterFormat::Rgba32,
                )?;

                let texture_offset = self.atlas_manager.insert(rasterized_glyph.canvas.size)?;
                let idx = texture_offset.texture_id.as_usize();
                if idx >= self.textures.len() {
                    self.textures
                        .resize_with(idx + 1, || create_texture(ATLAS_SIZE));
                }
                let texture = &mut self.textures[idx];
                insert_into_texture(texture_offset.allocated_region, &rasterized_glyph, texture);

                let glyph_texture_offset = GlyphTextureOffset {
                    texture_id: texture_offset.texture_id,
                    raster_bounds: bounds.to_f32(),
                    is_emoji: rasterized_glyph.is_emoji,
                    allocated_region: texture_offset.allocated_region,
                };

                self.frame_raster_time += started.elapsed();
                self.frame_rasterized += 1;
                self.cache.insert(cache_key, Some(glyph_texture_offset));
                Ok(Some(glyph_texture_offset))
            }
            Some(gto) => Ok(*gto),
        }
    }
}

#[cfg(test)]
#[path = "glyph_cache_tests.rs"]
mod tests;
