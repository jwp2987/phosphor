//! Module that rasterizes text using `swash`.

use crate::fonts::canvas::{Canvas, RasterFormat};
use crate::fonts::{FontId, GlyphId, RasterizedGlyph, SubpixelAlignment};
use crate::platform::FontDB as _;
use crate::rendering::GlyphConfig;
use crate::windowing::winit::fonts::FontDB;
use anyhow::{anyhow, Result};
use cosmic_text::{CacheKey, CacheKeyFlags};
use parking_lot::RwLock;
use pathfinder_geometry::rect::RectI;
use pathfinder_geometry::vector::{vec2i, Vector2F, Vector2I};
use std::collections::HashMap;

type RasterBoundsMemoKey = (FontId, GlyphId, u32, u32, u32);

/// Remembers the successfully computed raster bounds of a glyph. Computing them renders the
/// glyph, and the glyph cache asks for them once to size the atlas slot and again (via
/// `rasterize_glyph`) for every subpixel variant it rasterizes, with identical arguments each
/// time. Bounds do not depend on the subpixel offset, so one render per glyph and size suffices.
#[derive(Default)]
pub(super) struct RasterBoundsMemo {
    bounds: RwLock<HashMap<RasterBoundsMemoKey, RectI>>,
}

impl RasterBoundsMemo {
    fn key(font_id: FontId, size: f32, glyph_id: GlyphId, scale: Vector2F) -> RasterBoundsMemoKey {
        (
            font_id,
            glyph_id,
            size.to_bits(),
            scale.x().to_bits(),
            scale.y().to_bits(),
        )
    }

    /// Returns the memoized bounds, or computes them with `compute`. Errors are not memoized.
    fn get_or_compute(
        &self,
        key: RasterBoundsMemoKey,
        compute: impl FnOnce() -> Result<RectI>,
    ) -> Result<RectI> {
        if let Some(bounds) = self.bounds.read().get(&key) {
            return Ok(*bounds);
        }
        let bounds = compute()?;
        self.bounds.write().insert(key, bounds);
        Ok(bounds)
    }
}

impl FontDB {
    pub(super) fn glyph_raster_bounds(
        &self,
        font_id: FontId,
        size: f32,
        glyph_id: GlyphId,
        scale: Vector2F,
        glyph_config: &GlyphConfig,
    ) -> Result<RectI> {
        self.raster_bounds_memo.get_or_compute(
            RasterBoundsMemo::key(font_id, size, glyph_id, scale),
            || self.compute_glyph_raster_bounds(font_id, size, glyph_id, scale, glyph_config),
        )
    }

    fn compute_glyph_raster_bounds(
        &self,
        font_id: FontId,
        size: f32,
        glyph_id: GlyphId,
        scale: Vector2F,
        _glyph_config: &GlyphConfig,
    ) -> Result<RectI> {
        let Ok(_typographic_bounds) = self
            .glyph_typographic_bounds(font_id, glyph_id)
            .map(|bounds| bounds.to_f32())
        else {
            // We can't render this glyph using this font, return an empty rect to indicate we
            // don't need to rasterize this glyph. This can happen if the font doesn't contain
            // a glyph _or_ if the glyph isn't renderable (some fonts contain a glyph for the
            // space character, but don't provide outlines for it).
            return Ok(RectI::new(Vector2I::zero(), Vector2I::zero()));
        };

        let id = *self
            .text_layout_system
            .font_id_map
            .read()
            .get_by_left(&font_id)
            .unwrap();
        let image = self
            .swash_cache
            .write()
            .get_image_uncached(
                &mut self.text_layout_system.font_store.write(),
                CacheKey::new(
                    id,
                    glyph_id as u16,
                    size * scale.x(),
                    (0., 0.),
                    CacheKeyFlags::empty(),
                )
                .0,
            )
            // `get_image_uncached` already returns an owned `SwashImage`; cloning it here
            // would deep-copy the glyph bitmap (`data: Vec<u8>`) only to drop the original.
            .ok_or_else(|| anyhow!("Failed to get raster image"))?;

        let origin = vec2i(image.placement.left, -image.placement.top);
        let size = vec2i(image.placement.width as i32, image.placement.height as i32);
        Ok(RectI::new(origin, size))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn rasterize_glyph(
        &self,
        font_id: FontId,
        size: f32,
        glyph_id: GlyphId,
        scale: Vector2F,
        subpixel_alignment: SubpixelAlignment,
        glyph_config: &GlyphConfig,
        requested_format: RasterFormat,
    ) -> Result<RasterizedGlyph> {
        let raster_bounds =
            self.glyph_raster_bounds(font_id, size, glyph_id, scale, glyph_config)?;

        let id = *self
            .text_layout_system
            .font_id_map
            .read()
            .get_by_left(&font_id)
            .unwrap();
        // Get the raster image without caching--the parent FontDB handles all caching for us.
        let image = self
            .swash_cache
            .write()
            .get_image_uncached(
                &mut self.text_layout_system.font_store.write(),
                CacheKey::new(
                    id,
                    glyph_id as u16,
                    size * scale.x(),
                    (subpixel_alignment.to_offset().x(), 0.),
                    CacheKeyFlags::empty(),
                )
                .0,
            )
            // Owned already -- see the note in `glyph_raster_bounds`.
            .unwrap();

        let (original_format, is_color) = match image.content {
            cosmic_text::SwashContent::Mask => (RasterFormat::A8, false),
            cosmic_text::SwashContent::SubpixelMask => (RasterFormat::Rgba32, false),
            cosmic_text::SwashContent::Color => (RasterFormat::Rgba32, true),
        };

        // Ensure the pixmap is in the correct requested format (in practice this converts A8 to
        // RGBA32).
        // TODO(alokedesai): Ensure our font rasterization code is robust to returned formats that
        // are different than incoming formats. Right now, we create text bounds based on the
        // _incoming_ format.
        let pixmap = if original_format == RasterFormat::A8 {
            let bytes_per_pixel = requested_format.bytes_per_pixel() as usize;
            let mut pixmap = Vec::with_capacity(image.data.len() * bytes_per_pixel);
            for byte in image.data {
                for _ in 0..bytes_per_pixel {
                    pixmap.push(byte);
                }
            }
            pixmap
        } else {
            image.data
        };

        let canvas = Canvas {
            pixels: pixmap,
            size: raster_bounds.size(),
            row_stride: image.placement.width as usize * original_format.bytes_per_pixel() as usize,
            format: RasterFormat::Rgba32,
        };

        anyhow::Ok(RasterizedGlyph {
            canvas,
            is_emoji: is_color,
        })
    }
}

#[cfg(test)]
#[path = "swash_rasterizer_tests.rs"]
mod tests;
