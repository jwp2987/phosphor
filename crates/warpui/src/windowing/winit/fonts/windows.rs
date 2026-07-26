use super::{
    font_handle::FontHandle, FontFamily, LoadedSystemFonts, TextLayoutSystem,
    ValidateFontSupportsEn,
};
use crate::fonts::FontId;
use anyhow::Result;
use font_kit::loader::Loader as _;
use font_kit::{
    family_name::FamilyName as FKFamilyName, properties::Properties as FKProperties,
    properties::Style as FKStyle, properties::Weight as FKWeight, source::SystemSource as FKSource,
};
use itertools::Itertools;
use owned_ttf_parser::OwnedFace;
use std::collections::HashMap;
use std::sync::Arc;

/// Returns the BCP-47 locale string used to bias the DirectWrite Han glyph
/// fallback. Kept in sync with the current UI locale (set by `app::i18n` via
/// `crate::set_ui_locale`).
fn current_fallback_locale() -> String {
    crate::current_ui_locale()
}

/// Windows symbol fonts that are used to render window control icons. We specifically do not do any
/// validation of these fonts (i.e. to check if the font contains english characters).
const SYMBOL_ICON_FONTS: &[&str] = &["Segoe Fluent Icons", "Segoe MDL2 Assets"];

pub(crate) mod loader {
    use crate::fonts::FontInfo;

    use super::*;

    pub fn load_all_system_fonts() -> LoadedSystemFonts {
        let source = font_kit::source::SystemSource::new();
        let fonts = match source.all_fonts() {
            Ok(fonts) => fonts,
            Err(err) => {
                log::warn!("unable to retrieve all fonts from DirectWrite source: {err:?}");
                return LoadedSystemFonts(vec![]);
            }
        };

        let mut family_map = HashMap::new();

        for font_handle in fonts.into_iter() {
            if let Ok(font) = font_handle.load() {
                let family_name = font.family_name();
                let is_monospace = font.is_monospace();

                if font.glyph_for_char('m').is_none() {
                    // Only allow the user to select fonts that have an English character set.
                    log::debug!("skipping family {family_name:?} because no 'm' glyph was found");
                    continue;
                }
                // Convert font_kit::Handle into UI framework-specific FontHandle.
                let font_handle = match font_handle {
                    font_kit::handle::Handle::Path { path, font_index } => {
                        FontHandle::new(path, font_index, is_monospace)
                    }
                    font_kit::handle::Handle::Memory { bytes, font_index } => {
                        let owned_face_result = match Arc::try_unwrap(bytes) {
                            // If we can ensure ownership of the bytes, create an OwnedFace without copying.
                            Ok(owned_bytes) => OwnedFace::from_vec(owned_bytes, font_index),
                            // If we can't get sole ownership, create on OwnedFace from a copy the bytes
                            // (created by .to_vec()).
                            Err(shared_bytes) => {
                                OwnedFace::from_vec(shared_bytes.to_vec(), font_index)
                            }
                        };
                        match owned_face_result {
                            Ok(typeface) => FontHandle::from(typeface),
                            Err(err) => {
                                // If we can't parse the typeface, skip it.
                                log::warn!(
                                    "unable to parse typeface from family {family_name}: {err:?}"
                                );
                                continue;
                            }
                        }
                    }
                };

                let (entry_info, entry_family) = family_map
                    .entry(family_name.clone())
                    .or_insert_with(move || {
                        (
                            FontInfo {
                                family_name: family_name.clone(),
                                is_monospace,
                            },
                            FontFamily {
                                name: family_name,
                                fonts: vec![],
                            },
                        )
                    });
                entry_info.is_monospace |= is_monospace;
                entry_family.fonts.push(font_handle);
            }
        }
        LoadedSystemFonts(family_map.into_values().collect_vec())
    }

    pub fn load_system_font(font_family: &str) -> Result<FontFamily> {
        let source = font_kit::source::SystemSource::new();
        let family = source.select_family_by_name(font_family)?;

        let validate_supports_en = if SYMBOL_ICON_FONTS.contains(&font_family) {
            ValidateFontSupportsEn::No
        } else {
            ValidateFontSupportsEn::Yes
        };

        Ok(FontFamily {
            name: font_family.to_string(),
            fonts: family
                .fonts()
                .iter()
                .flat_map(|font_kit_handle| {
                    load_font_from_handle(font_kit_handle, validate_supports_en)
                })
                .collect_vec(),
        })
    }
}

impl TextLayoutSystem {
    /// Given a specific character and FontID, find alternate system fonts that can
    /// render that character.
    pub fn get_fallback_fonts_for_character(
        &self,
        character: char,
        font_id: FontId,
    ) -> Result<Vec<FontId>> {
        // Retrieve the font's family name and properties from the font store.
        // First, find the font's fontdb ID.
        let &original_font_id =
            self.font_id_map
                .read()
                .get_by_left(&font_id)
                .ok_or(anyhow::format_err!(
                    "No left entry found for {font_id:?} in font_id_map"
                ))?;
        let (style, weight, family_name) = self.get_font_info_from_store(original_font_id)?;
        let source = FKSource::new();
        let style = match style {
            fontdb::Style::Normal => FKStyle::Normal,
            fontdb::Style::Italic => FKStyle::Italic,
            fontdb::Style::Oblique => FKStyle::Oblique,
        };
        let weight = FKWeight(weight.0 as f32);
        let properties = FKProperties {
            style,
            weight,
            stretch: Default::default(),
        };

        let font_handle = source
            .select_best_match(
                &[
                    FKFamilyName::Title(family_name.to_owned()),
                    FKFamilyName::Monospace,
                ],
                &properties,
            )
            .map_err(|err| anyhow::anyhow!("Didn't find {family_name} in fontdb: {err}"))?;

        // Load fallback fonts for the requested character.
        let loaded_font = font_handle.load().map_err(|err| {
            anyhow::anyhow!("Unable to load typeface from font_kit Handle: {err:?}")
        })?;

        let locale = current_fallback_locale();

        // Locale-preferred CJK system fonts first: DirectWrite's IDWriteFontFallback
        // does not consult the locale to resolve Han glyph ambiguity on a Windows
        // English / dev environment, defaulting to Microsoft YaHei, which causes a
        // Japanese UI to get Simplified Chinese glyphs instead. So for shared CJK
        // Han characters, we prepend the system font corresponding to the current
        // locale (e.g. ja-* -> Yu Gothic UI) before falling back to DirectWrite.
        let mut fallback_font_vec: Vec<FontId> = Vec::new();
        if crate::is_shared_cjk_han(character) {
            for family in preferred_cjk_families_for_locale(&locale) {
                if let Ok(fam) = source.select_family_by_name(family) {
                    for fk_handle in fam.fonts() {
                        if let Ok(handle) =
                            load_font_from_handle(fk_handle, ValidateFontSupportsEn::No)
                        {
                            if let Ok(id) = self.insert_font(handle) {
                                fallback_font_vec.push(id);
                            }
                        }
                    }
                    if !fallback_font_vec.is_empty() {
                        break;
                    }
                }
            }
        }

        let fallback_result = loaded_font.get_fallbacks(character.to_string().as_str(), &locale);

        // Convert each font-kit fallback `Font` into a UI framework `FontHandle` and load it into
        // fontdb. We deliberately avoid `font_kit::Font::handle()` here: its default impl reads
        // the full font file into an `Arc<Vec<u8>>` and returns a `Handle::Memory` with
        // `font_index` hard-coded to `0` (see the FIXME at font-kit/src/loader.rs:172), which
        // bypasses `TextLayoutSystem::insert_font`'s path-based dedup and loses TTC face indices.
        // Instead we reach through `NativeFont` to the underlying `IDWriteFontFace` and recover
        // the on-disk file path + real face index, the same way
        // `DirectWriteSource::create_handle_from_dwrite_font` does for enumerated system fonts.
        // This lets fontdb mmap the file lazily and lets `insert_font` dedup by `(path, index)`,
        // so the same fallback family is loaded at most once per process.
        fallback_font_vec.extend(fallback_result.fonts.into_iter().flat_map(|fallback_font| {
            let loaded_handle = fallback_font_path_handle(&fallback_font.font).or_else(|| {
                // Last-resort fallback for fonts that aren't backed by a local file (e.g.
                // custom collection loaders). These don't appear in practice for DirectWrite
                // system fallbacks, but preserve the original byte-copy behavior so we
                // degrade gracefully instead of dropping the glyph.
                let handle = fallback_font.font.handle()?;
                load_font_from_handle(&handle, ValidateFontSupportsEn::No).ok()
            })?;
            self.insert_font(loaded_handle).ok()
        }));

        Ok(fallback_font_vec)
    }

    /// Warms up the CJK font families preferred by the current UI locale
    /// (`preferred_cjk_families_for_locale`) at startup, called synchronously
    /// once right after `FontDB` construction.
    ///
    /// Fixes the zerx-lab/warp#68 regression ("Chinese font rendering is
    /// broken after startup, only fixed by closing and reopening the panel"):
    /// PR #62 prepends a locale-appropriate system CJK font to cosmic-text's
    /// fallback chain in `get_fallback_fonts_for_character`; but the first
    /// time CJK fallback is triggered on the first frame,
    /// `SystemSource::select_family_by_name` occasionally fails to get a font
    /// on the Windows DirectWrite cold path, leaving the prepend segment
    /// empty, so the fallback falls through to `IDWriteFontFallback::MapCharacters`'s
    /// cold output (which may give a family that doesn't match the locale
    /// preference). Once that result is written into cosmic-text's
    /// `font_codepoint_support_info_cache` / `shape_run_cache` (instance-level
    /// on FontSystem, not invalidated while the locale stays the same),
    /// subsequent rendering keeps reusing the wrong fallback, until a panel
    /// teardown / font size / font_id change bypasses the cache key and it
    /// runs again.
    ///
    /// The warm-up here synchronously feeds the preferred family into fontdb
    /// (`insert_font` dedups via `loaded_fonts` by `(path, index)`, so a
    /// subsequent hit in `get_fallback_fonts_for_character` directly returns
    /// the existing `FontId` without reloading), eliminating the cold path's
    /// nondeterminism.
    ///
    /// Performance cost: constructs a `SystemSource` once at startup, and
    /// selects, loads, and inserts one preferred family. `load_font_from_handle`
    /// converts a font_kit Path handle to an `OwnedFace`; fontdb mmaps it
    /// lazily internally. Measured at only a few milliseconds on Windows 11
    /// with the stock YaHei UI installed, and the net benefit is positive —
    /// previously, every CJK character cache miss in
    /// `get_fallback_fonts_for_character` would construct a new `SystemSource`
    /// and re-select/load; after warm-up, this path hits an already-loaded
    /// FontId on the very first frame.
    ///
    /// Non-CJK locales also warm up Windows' default Simplified Chinese UI
    /// font family, to guarantee that Chinese filenames and other plain
    /// `Text` elements have usable Han glyphs on the first frame even under
    /// an English UI, without needing to enumerate all system fonts.
    ///
    /// Failure (the system doesn't have the family installed / handle
    /// loading fails) is only logged as a warning and doesn't block startup —
    /// it degrades to DirectWrite's default fallback in that case.
    pub(crate) fn warm_up_preferred_cjk_families(&self) {
        let locale = current_fallback_locale();
        let families = preferred_cjk_families_for_locale(&locale);
        if families.is_empty() {
            return;
        }
        let source = FKSource::new();
        let mut warmed_any = false;
        for family in families {
            let Ok(fam) = source.select_family_by_name(family) else {
                // The system doesn't have this family installed (e.g. a clean Windows 11
                // install may not have SimSun) — try the next one.
                continue;
            };
            let mut family_loaded = false;
            for fk_handle in fam.fonts() {
                match load_font_from_handle(fk_handle, ValidateFontSupportsEn::No) {
                    Ok(handle) => {
                        if self.insert_font(handle).is_ok() {
                            family_loaded = true;
                        }
                    }
                    Err(err) => {
                        log::debug!(
                            "warm_up_preferred_cjk_families: skipping a face of {family:?}: {err:?}"
                        );
                    }
                }
            }
            if family_loaded {
                warmed_any = true;
                // Aligns with `get_fallback_fonts_for_character`'s "break on the first
                // family hit" behavior, to avoid warming up beyond the font set that
                // the actual fallback would use.
                break;
            }
        }
        if !warmed_any {
            log::warn!(
                "warm_up_preferred_cjk_families: failed to warm up any CJK family ({families:?}) for locale={locale:?} — first-frame CJK fallback will go through the DirectWrite cold path"
            );
        }
    }

    /// Critical section for fetching the font style, weight and family name from fontdb.
    /// This function performs the minimum work required to fetch this information from
    /// fontdb to minimize the amount of time spent holding a read lock on the font store.
    fn get_font_info_from_store(
        &self,
        font_id: fontdb::ID,
    ) -> Result<(fontdb::Style, fontdb::Weight, String)> {
        let store_read_lock = self.font_store.read();
        let db_read = store_read_lock.db();
        let face = db_read.face(font_id).ok_or(anyhow::anyhow!(
            "Unable to retrieve font face from fontdb font_store"
        ))?;
        let style = face.style;
        let weight = face.weight;
        let Some(en_us_family_info) = face.families.first() else {
            return Err(anyhow::anyhow!("Font face doesn't have any family names"));
        };
        let (family_name, _) = en_us_family_info;
        // Clone the family name because it's protected by the font store's RWLock.
        Ok((style, weight, family_name.to_owned()))
    }
}

fn load_font_from_handle(
    font_handle: &font_kit::handle::Handle,
    validate_supports_en_charset: ValidateFontSupportsEn,
) -> Result<FontHandle> {
    let font = font_handle.load()?;
    let is_monospace = font.is_monospace();
    if matches!(validate_supports_en_charset, ValidateFontSupportsEn::Yes) {
        font.glyph_for_char('m').ok_or(anyhow::format_err!(
            "No 'm' glyph found for font {}",
            font.full_name()
        ))?;
    }
    match font_handle {
        font_kit::handle::Handle::Path { path, font_index } => {
            Ok(FontHandle::new(path, *font_index, is_monospace))
        }
        font_kit::handle::Handle::Memory { bytes, font_index } => {
            let typeface = OwnedFace::from_vec(bytes.to_vec(), *font_index)?;
            Ok(FontHandle::from(typeface))
        }
    }
}

/// Extracts the primary language subtag of a BCP-47 tag, uniformly ASCII
/// lowercased. E.g. `ja-jp` -> `ja`, `zh-hant-tw` -> `zh`, `kok-in` -> `kok`.
/// Used to precisely determine the primary language, avoiding prefix matches
/// like `starts_with("ko")` mistaking `kok-IN` (Konkani) for Korean, or
/// `zha-CN` (Zhuang) for Chinese.
fn primary_subtag(lower: &str) -> &str {
    lower.split(['-', '_']).next().unwrap_or("")
}

const SIMPLIFIED_CHINESE_CJK_FAMILIES: &[&str] =
    &["Microsoft YaHei UI", "Microsoft YaHei", "SimSun"];
const TRADITIONAL_CHINESE_CJK_FAMILIES: &[&str] = &[
    "Microsoft JhengHei UI",
    "Microsoft JhengHei",
    "PMingLiU",
    "MingLiU",
];
const JAPANESE_CJK_FAMILIES: &[&str] = &[
    "Yu Gothic UI",
    "Yu Gothic",
    "Meiryo UI",
    "Meiryo",
    "MS Gothic",
];
const KOREAN_CJK_FAMILIES: &[&str] = &["Malgun Gothic", "Gulim", "Dotum"];

/// Returns the Windows system CJK font families preferred for a locale (in
/// priority order). Used to override DirectWrite's Han fallback, which
/// doesn't consult the locale.
///
/// The routing recognizes both BCP-47 region subtags (zh-TW / zh-HK / zh-MO)
/// and script subtags (zh-Hant / zh-Hans, optionally with a region:
/// zh-Hant-TW etc.), so callers don't need to normalize the tag beforehand.
/// Non-CJK locales use the Simplified Chinese font family as a stable
/// fallback, to avoid missing glyphs on the first frame for Chinese
/// filenames under an English UI.
fn preferred_cjk_families_for_locale(locale: &str) -> &'static [&'static str] {
    let lower = locale.to_ascii_lowercase();
    match primary_subtag(&lower) {
        "ja" => JAPANESE_CJK_FAMILIES,
        "ko" => KOREAN_CJK_FAMILIES,
        "zh" if is_zh_traditional(&lower) => TRADITIONAL_CHINESE_CJK_FAMILIES,
        "zh" => SIMPLIFIED_CHINESE_CJK_FAMILIES,
        _ => SIMPLIFIED_CHINESE_CJK_FAMILIES,
    }
}

/// Whether `lower` (an ASCII-lowercased BCP-47 tag) refers to Traditional
/// Chinese. Matches both the region form (zh-tw / zh-hk / zh-mo) and the
/// script subtag form (zh-hant, zh-hant-tw, zh-foo-hant, etc.). Requires a
/// hyphen boundary to avoid accidental matches like `zh-hansolo`.
fn is_zh_traditional(lower: &str) -> bool {
    if primary_subtag(lower) != "zh" {
        return false;
    }
    if lower.starts_with("zh-tw") || lower.starts_with("zh-hk") || lower.starts_with("zh-mo") {
        return true;
    }
    // Iterate over the hyphenated subtags after the primary tag.
    lower.split('-').skip(1).any(|sub| sub == "hant")
}

/// Builds a path-backed [`FontHandle`] for a font-kit DirectWrite `Font` by reaching through
/// [`font_kit::loaders::directwrite::NativeFont`] to the underlying `IDWriteFontFace`.
///
/// This mirrors what font-kit itself does for enumerated system fonts in
/// `DirectWriteSource::create_handle_from_dwrite_font` (font-kit/src/sources/directwrite.rs:103),
/// and is the reason we carry `dwrote` as a direct dependency: font-kit's generic
/// `Loader::handle()` default returns a `Handle::Memory` with a byte copy of the full file, which
/// we specifically need to avoid on the per-character fallback path.
///
/// Returns `None` when DirectWrite cannot produce a local file path for the font, i.e. the font
/// was loaded via a custom collection loader or backed only by an in-memory stream. For system
/// fallback fonts returned by `IDWriteFontFallback::MapCharacters` against the system font
/// collection, a path is always available.
fn fallback_font_path_handle(font: &font_kit::loaders::directwrite::Font) -> Option<FontHandle> {
    let native = font.native_font();
    let file = native.dwrite_font_face.files().ok()?.into_iter().next()?;
    let path = file.font_file_path().ok()?;
    let font_index = native.dwrite_font_face.get_index();
    Some(FontHandle::new(path, font_index, font.is_monospace()))
}

#[cfg(test)]
mod tests {
    use super::{
        preferred_cjk_families_for_locale, JAPANESE_CJK_FAMILIES, KOREAN_CJK_FAMILIES,
        SIMPLIFIED_CHINESE_CJK_FAMILIES, TRADITIONAL_CHINESE_CJK_FAMILIES,
    };

    #[test]
    fn preferred_cjk_families_defaults_to_simplified_chinese_for_non_cjk_locale() {
        assert_eq!(
            preferred_cjk_families_for_locale("en-US"),
            SIMPLIFIED_CHINESE_CJK_FAMILIES
        );
        assert_eq!(
            preferred_cjk_families_for_locale(""),
            SIMPLIFIED_CHINESE_CJK_FAMILIES
        );
    }

    #[test]
    fn preferred_cjk_families_respects_cjk_locale() {
        assert_eq!(
            preferred_cjk_families_for_locale("zh-CN"),
            SIMPLIFIED_CHINESE_CJK_FAMILIES
        );
        assert_eq!(
            preferred_cjk_families_for_locale("zh-Hans-US"),
            SIMPLIFIED_CHINESE_CJK_FAMILIES
        );
        assert_eq!(
            preferred_cjk_families_for_locale("zh-TW"),
            TRADITIONAL_CHINESE_CJK_FAMILIES
        );
        assert_eq!(
            preferred_cjk_families_for_locale("zh-Hant-HK"),
            TRADITIONAL_CHINESE_CJK_FAMILIES
        );
        assert_eq!(
            preferred_cjk_families_for_locale("ja-JP"),
            JAPANESE_CJK_FAMILIES
        );
        assert_eq!(
            preferred_cjk_families_for_locale("ko-KR"),
            KOREAN_CJK_FAMILIES
        );
    }
}
