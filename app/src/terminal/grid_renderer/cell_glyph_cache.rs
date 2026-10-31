//! This module defines CellGlyphCache, a struct which manages the caching of glyph values for cells
//! when rendering Grids within Zap.
use warpui::elements::DEFAULT_LINE_HEIGHT_RATIO;

use warpui::fonts::{Cache as FontCache, FamilyId, FontId, GlyphId, Properties};
use warpui::platform::LineStyle;
use warpui::text_layout::{StyleAndFont, DEFAULT_TOP_BOTTOM_RATIO};
use warpui::PaintContext;

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

/// Stores cached glyph values for characters/strings. Note that we normally only need to look up
/// characters - we only look up strings in the case of zerowidth characters (which act as modifiers
/// to the first character e.g. emoji variant selectors). We have 2 separate caches internally for
/// performance reasons (avoid allocating strings when we don't need to!).
#[derive(Default)]
pub struct CellGlyphCache {
    glyph_cache: HashMap<(char, FontId), Option<(GlyphId, FontId)>>,
    string_cache: HashMap<(String, FontId), Option<(GlyphId, FontId)>>,
    /// What the entries above were computed against. Entries are only valid while the font
    /// cache's glyph epoch (identity + set of loaded fonts) and the fallback family are unchanged.
    validity: Option<CacheValidity>,
}

#[derive(Clone, Copy, PartialEq)]
struct CacheValidity {
    glyph_epoch: (u64, u64),
    fallback_font_family: Option<FamilyId>,
}

/// Upper bound on the entries kept across frames, so that a session which has displayed many
/// distinct characters cannot grow the cache without limit.
const MAX_PERSISTENT_ENTRIES: usize = 16 * 1024;

thread_local! {
    /// The cache shared by every grid painted on the UI thread. It outlives a single paint so
    /// that characters (and in particular characters no font has a glyph for) are resolved once,
    /// not once per frame.
    static SHARED: RefCell<Option<CellGlyphCache>> = const { RefCell::new(None) };
}

/// Exclusive access to the thread's persistent [`CellGlyphCache`] for the duration of a paint.
/// The cache is returned to the thread on drop. A paint nested inside another paint gets a fresh
/// (empty) cache instead of aliasing the outer one.
pub struct SharedCellGlyphCache(Option<CellGlyphCache>);

impl SharedCellGlyphCache {
    pub fn acquire() -> Self {
        Self(Some(
            SHARED
                .with(|shared| shared.borrow_mut().take())
                .unwrap_or_default(),
        ))
    }
}

impl Deref for SharedCellGlyphCache {
    type Target = CellGlyphCache;

    fn deref(&self) -> &CellGlyphCache {
        self.0.as_ref().expect("cache is present until drop")
    }
}

impl DerefMut for SharedCellGlyphCache {
    fn deref_mut(&mut self) -> &mut CellGlyphCache {
        self.0.as_mut().expect("cache is present until drop")
    }
}

impl Drop for SharedCellGlyphCache {
    fn drop(&mut self) {
        if let Some(cache) = self.0.take() {
            SHARED.with(|shared| *shared.borrow_mut() = Some(cache));
        }
    }
}

impl CellGlyphCache {
    /// Drops every cached entry if the fonts they were resolved against have changed since.
    pub(super) fn sync(&mut self, font_cache: &FontCache, fallback_font_family: Option<FamilyId>) {
        let current = CacheValidity {
            glyph_epoch: font_cache.glyph_epoch(),
            fallback_font_family,
        };
        if self.validity != Some(current)
            || self.glyph_cache.len() + self.string_cache.len() > MAX_PERSISTENT_ENTRIES
        {
            self.glyph_cache.clear();
            self.string_cache.clear();
            self.validity = Some(current);
        }
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.glyph_cache.len() + self.string_cache.len()
    }

    pub(super) fn glyph_for_char(
        &mut self,
        char: char,
        font_id: FontId,
        fallback_font_family: Option<FamilyId>,
        properties: Properties,
        font_cache: &FontCache,
    ) -> Option<(GlyphId, FontId)> {
        self.sync(font_cache, fallback_font_family);
        *self.glyph_cache.entry((char, font_id)).or_insert_with(|| {
            if let Some(glyph) = font_cache.glyph_for_char(font_id, char, false) {
                return Some(glyph);
            }

            if let Some(fallback_font_family) = fallback_font_family {
                let fallback_font = font_cache.select_font(fallback_font_family, properties);
                if let Some(glyph) = font_cache.glyph_for_char(fallback_font, char, false) {
                    return Some(glyph);
                }
            }

            font_cache.glyph_for_char(font_id, char, true)
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn glyph_for_string(
        &mut self,
        string: &str,
        font_id: FontId,
        font_cache: &FontCache,
        font_family: FamilyId,
        fallback_font_family: Option<FamilyId>,
        font_size: f32,
        properties: Properties,
        ctx: &mut PaintContext,
    ) -> Option<(GlyphId, FontId)> {
        self.sync(font_cache, fallback_font_family);
        let key = (string.to_owned(), font_id);
        if let Some(glyph) = self.string_cache.get(&key) {
            return *glyph;
        }

        // Calculate the length of total characters in the string.
        let run_length_chars = string.chars().count();
        let layout_font_family = string
            .chars()
            .next()
            .and_then(|char| {
                if font_cache.glyph_for_char(font_id, char, false).is_some() {
                    return None;
                }

                let fallback_font_family = fallback_font_family?;
                let fallback_font = font_cache.select_font(fallback_font_family, properties);
                font_cache
                    .glyph_for_char(fallback_font, char, false)
                    .map(|_| fallback_font_family)
            })
            .unwrap_or(font_family);
        let line = ctx.text_layout_cache.layout_line(
            string,
            LineStyle {
                font_size,
                // Note that we DO NOT paint the `Line` in this particular instance. As such,
                // the line height ratio and baseline ratio are both NOT used. Hence, we arbitrarily
                // set them to the default values.
                line_height_ratio: DEFAULT_LINE_HEIGHT_RATIO,
                baseline_ratio: DEFAULT_TOP_BOTTOM_RATIO,
                fixed_width_tab_size: None,
            },
            &[(
                (0..run_length_chars),
                StyleAndFont {
                    font_family: layout_font_family,
                    properties,
                    style: Default::default(),
                },
            )],
            f32::MAX,
            Default::default(),
            &font_cache.text_layout_system(),
        );
        let glyph = line.runs.first().and_then(|run| {
            if run.glyphs.len() > 1 {
                // If we have more than one glyph, something has gone wrong.
                return None;
            }
            run.glyphs.first().map(|glyph| (glyph.id, run.font_id))
        });

        let glyph = glyph.or_else(|| {
            #[cfg(debug_assertions)]
            log::debug!("Falling back to glyph for first character of string, could not get glyph for entire string: {string:?}");
            let first_char = string.chars().next()?;
            self.glyph_for_char(
                first_char,
                font_id,
                fallback_font_family,
                properties,
                font_cache,
            )
        });

        self.string_cache.insert(key, glyph);
        glyph
    }
}
