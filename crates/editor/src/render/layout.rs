//! Shared text-layout utilities needed throughout the editor implementation.

#[cfg(test)]
use markdown_parser::FormattedTextInline;
use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::content::text::{BufferBlockStyle, TextStylesWithMetadata};
use warpui::AppContext;
use warpui::fonts::TextLayoutSystem;
#[cfg(test)]
use warpui::fonts::{Style, Weight};
use warpui::text_layout::{
    ClipConfig, LayoutCache, Line, StyleAndFont, TextAlignment, TextBorder, TextStyle,
};
use warpui::units::{IntoPixels, Pixels};
use warpui::{color::ColorU, text_layout::TextFrame};

use super::model::{BlockSpacing, ParagraphStyles, RenderState, RichTextStyles};

const HYPERLINK_UNDERLINE_COLOR: u32 = 0x7aa6daff;

/// Cap on the number of `char`s shaped into a single text frame.
///
/// The editor lays out at an effectively infinite width, so one logical line becomes one unwrapped
/// shaping call. Without a cap, a single multi-megabyte line allocates a text frame plus per-`char`
/// glyph and caret-position vectors bounded only by that line's length (APP-5392). Text past the
/// cap is dropped before shaping, which is safe because callers track buffer offsets from their own
/// content length rather than from the shaped frame (`LayOutArgs::layout_run` in `content/edit.rs`
/// accumulates `content_length`/`frame_offset_from_block_start` from buffer content, never from the
/// shaped frame), and consumers that map an offset or coordinate into a frame already clamp to its
/// end (`OffsetMap::translate` in `render/model/offset_map.rs`, and
/// `TextFrame::caret_index_for_x_unbounded`/`caret_position_for_index` in `warpui_core::text_layout`,
/// which already special-case a shaped frame narrower than its logical line). The cap sits far past
/// what can be read on screen, so a cursor pinning at the truncation point is only reachable in
/// lines nobody edits visually.
const MAX_LAYOUT_LINE_CHARS: usize = 2 * 1024 * 1024;

/// Shortens `text` to at most [`MAX_LAYOUT_LINE_CHARS`], returning it unchanged when it already
/// fits.
fn truncate_text_for_layout(text: &str) -> &str {
    // Every `char` occupies at least one byte, so a byte length within the cap puts the char count
    // within it too. This keeps the common case off the linear `char_indices` walk below.
    if text.len() <= MAX_LAYOUT_LINE_CHARS {
        return text;
    }

    // `nth` yields the byte offset at which the first `MAX_LAYOUT_LINE_CHARS` chars end, and `None`
    // when the text holds no more chars than that. Slicing there is always on a char boundary.
    match text.char_indices().nth(MAX_LAYOUT_LINE_CHARS) {
        Some((truncate_at, _)) => &text[..truncate_at],
        None => text,
    }
}

/// Drops style runs starting past [`MAX_LAYOUT_LINE_CHARS`] and clamps the tail of a run straddling
/// it, keeping char-indexed runs consistent with text shortened by [`truncate_text_for_layout`].
fn clamp_style_runs_for_layout(
    style_runs: &[(Range<usize>, StyleAndFont)],
) -> Vec<(Range<usize>, StyleAndFont)> {
    style_runs
        .iter()
        .filter(|(range, _)| range.start < MAX_LAYOUT_LINE_CHARS)
        .map(|(range, style)| (range.start..range.end.min(MAX_LAYOUT_LINE_CHARS), *style))
        .collect()
}

#[derive(Clone, Debug, Default)]
pub(crate) struct InlineTextLayoutInput {
    pub text: String,
    pub style_runs: Vec<(Range<usize>, StyleAndFont)>,
}

/// Minimum number of blocks in one edit delta before its plain-text paragraphs are laid out
/// lazily (dehydrated) instead of being shaped up front. Typing, small pastes and every
/// interactive edit stay below it and are laid out eagerly.
pub const LAZY_LAYOUT_MIN_BLOCKS: usize = 512;

/// Number of the widest dehydrated paragraphs of a delta that are shaped once at layout time so
/// the content width is exact for them (their glyphs are dropped again immediately).
pub const LAZY_LAYOUT_EXACT_WIDTH_LINES: usize = 8;

/// Whether lazy (dehydrated) paragraph layout is disabled by the kill switch
/// `PHOSPHOR_EDITOR_EAGER_LAYOUT=1`. Read once.
pub fn eager_layout_forced() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED.get_or_init(|| {
        std::env::var("PHOSPHOR_EDITOR_EAGER_LAYOUT").is_ok_and(|v| !v.is_empty() && v != "0")
    })
}

/// Utility for laying out rich text.
pub struct TextLayout<'a> {
    font_cache: TextLayoutSystem<'a>,
    rich_text_styles: &'a RichTextStyles,
    max_width: f32,
    container_scrolls_horizontally: bool,
    /// Whether the owner supports dehydrated paragraphs (the unwrapped code editor).
    lazy_capable: bool,
    /// Set for the duration of one large edit delta: plain-text paragraphs are then built
    /// dehydrated (see [`Self::dehydrate_paragraphs`]).
    dehydrate_now: AtomicBool,
    /// Advance of a representative monospace glyph, measured on first use.
    mono_advance: Mutex<Option<(FamilyIdKey, f32)>>,
}

/// Identity of the font a cached monospace advance belongs to.
type FamilyIdKey = (warpui::fonts::FamilyId, u32, u32);

impl<'a> TextLayout<'a> {
    pub fn new(
        font_cache: TextLayoutSystem<'a>,
        rich_text_styles: &'a RichTextStyles,
        max_width: f32,
    ) -> Self {
        Self {
            font_cache,
            rich_text_styles,
            max_width,
            container_scrolls_horizontally: false,
            lazy_capable: false,
            dehydrate_now: AtomicBool::new(false),
            mono_advance: Mutex::new(None),
        }
    }

    /// Declare that paragraphs laid out through this `TextLayout` may be dehydrated (the owner
    /// hydrates what it paints and queries). Ignored unless the layout is unwrapped.
    pub fn with_lazy_capable(mut self, flag: bool) -> Self {
        self.lazy_capable = flag && self.max_width == f32::MAX && !eager_layout_forced();
        self
    }

    /// Whether the owner supports dehydrated paragraphs.
    pub fn lazy_capable(&self) -> bool {
        self.lazy_capable
    }

    /// Switch dehydrated plain-text paragraph layout on or off for the current delta.
    pub(crate) fn set_dehydrate_paragraphs(&self, on: bool) {
        self.dehydrate_now
            .store(on && self.lazy_capable, Ordering::Relaxed);
    }

    /// Whether runs of dehydrated lines may be grouped into parked chunks (#1340 P5; off with
    /// `PHOSPHOR_EDITOR_P5=0`).
    pub(crate) fn parking_enabled(&self) -> bool {
        self.lazy_capable && crate::render::model::parked::p5_enabled()
    }

    /// Whether plain-text paragraphs should currently be built dehydrated.
    pub(crate) fn dehydrate_paragraphs(&self) -> bool {
        self.dehydrate_now.load(Ordering::Relaxed)
    }

    /// Advance, in pixels, of one cell of a monospace run in `style` (measured from the real
    /// font with a representative glyph, cached per font).
    pub(crate) fn monospace_advance(&self, style: &ParagraphStyles) -> f32 {
        let key = (
            style.font_family,
            style.font_size.to_bits(),
            style.font_weight as u32,
        );
        let mut cached = self.mono_advance.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((cached_key, advance)) = *cached {
            if cached_key == key {
                return advance;
            }
        }
        // 'M' x 16 averaged: robust to hinting/rounding of a single glyph.
        let sample = "M".repeat(16);
        let style_runs = [(
            0..16,
            self.style_and_font(style, &TextStylesWithMetadata::default()),
        )];
        let frame = self.layout_text_with_options(
            &sample,
            style,
            &style_runs,
            f32::MAX,
            TextAlignment::default(),
        );
        let advance = frame.max_width() / 16.;
        *cached = Some((key, advance));
        advance
    }

    /// Indicate that the surrounding container (for example, a code editor) already provides
    /// horizontal scrolling over its full content area. Blocks whose rendering would otherwise
    /// introduce a nested horizontal scroll (like wide Markdown tables) should render at their
    /// full intrinsic width instead and rely on the container's scroll.
    pub fn with_container_scrolls_horizontally(mut self, flag: bool) -> Self {
        self.container_scrolls_horizontally = flag;
        self
    }

    /// Whether the surrounding container already provides horizontal scrolling over its full
    /// content area. See [`Self::with_container_scrolls_horizontally`].
    pub fn container_scrolls_horizontally(&self) -> bool {
        self.container_scrolls_horizontally
    }

    /// Builds a [`TextLayout`] for an editor render state.
    pub fn for_render_state(app: &'a AppContext, model: &'a RenderState) -> Self {
        Self::new(
            app.font_cache().text_layout_system(),
            model.styles(),
            model.viewport().width().as_f32(),
        )
        .with_container_scrolls_horizontally(model.container_scrolls_horizontally())
    }

    /// Lay out a single frame of text. The caller is responsible for mapping rich text into
    /// the paragraph's styling and spacing, as well as the per-character styles.
    ///
    /// See [`Self::style_and_font`] for help constructing the `style_runs`.
    pub fn layout_text(
        &self,
        text: &str,
        paragraph_style: &ParagraphStyles,
        spacing: &BlockSpacing,
        style_runs: &[(Range<usize>, StyleAndFont)],
    ) -> Arc<TextFrame> {
        self.layout_text_with_options(
            text,
            paragraph_style,
            style_runs,
            self.content_width(spacing),
            Default::default(),
        )
    }

    pub fn layout_text_with_options(
        &self,
        text: &str,
        paragraph_style: &ParagraphStyles,
        style_runs: &[(Range<usize>, StyleAndFont)],
        max_width: f32,
        alignment: TextAlignment,
    ) -> Arc<TextFrame> {
        if text.is_empty() {
            return Arc::new(TextFrame::empty(
                paragraph_style.font_size,
                paragraph_style.line_height_ratio,
            ));
        }

        let shaped_text = truncate_text_for_layout(text);
        let clamped_style_runs = (shaped_text.len() < text.len()).then(|| {
            // Only pay for a new run list when the text actually lost characters.
            clamp_style_runs_for_layout(style_runs)
        });

        Arc::new(self.font_cache.layout_text_uncached(
            shaped_text,
            paragraph_style.line_style(),
            clamped_style_runs.as_deref().unwrap_or(style_runs),
            max_width,
            f32::MAX,
            alignment,
            None,
        ))
    }

    /// Lays out placeholder text for empty blocks.
    pub fn layout_placeholder(
        &self,
        layout_cache: &LayoutCache,
        text: &str,
        block_type: &BufferBlockStyle,
        spacing: &BlockSpacing,
    ) -> Arc<Line> {
        let paragraph_styles = self.paragraph_styles(block_type);
        let style_and_font = self.style_and_font(
            &paragraph_styles,
            &TextStylesWithMetadata::default().for_placeholder(),
        );
        let text = truncate_text_for_layout(text);
        let style_runs = &[(0..text.chars().count(), style_and_font)];
        layout_cache.layout_line(
            text,
            paragraph_styles.line_style(),
            style_runs,
            self.content_width(spacing),
            ClipConfig::end(),
            &self.font_cache,
        )
    }

    /// Returns the maximum width for text content laid out with the given spacing.
    fn content_width(&self, spacing: &BlockSpacing) -> f32 {
        self.max_width - spacing.x_axis_offset().as_f32()
    }

    /// The paragraph-level styling to use for blocks of the given kind.
    pub fn paragraph_styles(&self, block_type: &BufferBlockStyle) -> ParagraphStyles {
        self.rich_text_styles.paragraph_styles(block_type)
    }

    /// Given the [paragraph-level](ParagraphStyles) and [text-level](TextStyles) styles applicable
    /// to a range of text, build the [`StyleAndFont`] configuration for laying out that text.
    pub fn style_and_font(
        &self,
        paragraph_styles: &ParagraphStyles,
        text_styles: &TextStylesWithMetadata,
    ) -> StyleAndFont {
        let font_properties = text_styles.apply_properties(paragraph_styles.properties());
        let font_family = if text_styles.is_inline_code() {
            self.rich_text_styles.inline_code_style.font_family
        } else {
            paragraph_styles.font_family
        };

        let mut styling = TextStyle::default();
        if text_styles.is_placeholder() {
            styling = styling.with_foreground_color(self.rich_text_styles.placeholder_color);
        }

        if text_styles.is_strikethrough() {
            styling = styling.with_show_strikethrough(true);
        }

        if text_styles.is_underlined() {
            styling = styling.with_underline_color(self.rich_text_styles.base_text.text_color);
        }

        if text_styles.is_inline_code() {
            styling = styling
                .with_foreground_color(self.rich_text_styles.inline_code_style.font_color)
                .with_background_color(self.rich_text_styles.inline_code_style.background)
                .with_border(TextBorder {
                    color: self.rich_text_styles.inline_code_style.background,
                    radius: 4,
                    width: 1,
                    // Use the set 1.2 line height ratio for inline code backgrounds.
                    line_height_ratio_override: Some(120),
                });
        }

        if let Some(color) = text_styles.color() {
            styling = styling.with_syntax_color(color);
        }

        let style_and_font = StyleAndFont::new(font_family, font_properties, styling);

        if text_styles.is_link() {
            add_link_to_style_and_font(style_and_font)
        } else {
            style_and_font
        }
    }

    pub fn rich_text_styles(&self) -> &'a RichTextStyles {
        self.rich_text_styles
    }

    pub fn max_width(&self) -> Pixels {
        self.max_width.into_pixels()
    }
}

/// The line height for a line of text. In CSS terminology, this is the height of the
/// [line box](https://www.w3.org/TR/css-inline-3/#line-box). In typographic terms,
/// it should correspond to the distance between the [top](https://stackoverflow.com/questions/27631736/meaning-of-top-ascent-baseline-descent-bottom-and-leading-in-androids-font)
/// of one line and the top of the next.
///
/// Unlike [`Line::height`], this height does not depend on the specific text in the line. The
/// `height` field measures the line's actual bounds, so it depends on whether glyphs go below
/// the baseline or above the [cap line](https://www.canva.com/learn/typography-terms/).
pub(crate) fn line_height(line: &Line) -> f32 {
    line.font_size * line.line_height_ratio
}

pub(crate) fn add_link_to_style_and_font(mut style: StyleAndFont) -> StyleAndFont {
    let hyperlink_color = ColorU::from_u32(HYPERLINK_UNDERLINE_COLOR);
    style.style = style
        .style
        .with_underline_color(hyperlink_color)
        .with_foreground_color(hyperlink_color);
    style
}

#[cfg(test)]
pub(crate) fn markdown_inline_to_text_and_style_runs(
    inline: &FormattedTextInline,
    paragraph_style: &ParagraphStyles,
    link_color: Option<ColorU>,
    inline_code_background: Option<ColorU>,
) -> InlineTextLayoutInput {
    let mut text = String::new();
    let mut style_runs = Vec::new();
    let mut start = 0usize;

    for fragment in inline {
        if fragment.text.is_empty() {
            continue;
        }

        text.push_str(&fragment.text);
        let len = fragment.text.chars().count();
        let end = start + len;

        let mut properties = paragraph_style.properties();
        if let Some(custom_weight) = fragment.styles.weight {
            properties = properties.weight(Weight::from_custom_weight(Some(custom_weight)));
        }
        if fragment.styles.italic {
            properties = properties.style(Style::Italic);
        }

        let mut text_style = TextStyle::new();
        if fragment.styles.strikethrough {
            text_style = text_style.with_show_strikethrough(true);
        }
        if fragment.styles.underline {
            text_style = text_style.with_underline_color(paragraph_style.text_color);
        }
        if fragment.styles.inline_code
            && let Some(background) = inline_code_background
        {
            text_style = text_style.with_background_color(background);
        }
        if fragment.styles.hyperlink.is_some()
            && let Some(link_color) = link_color
        {
            text_style = text_style
                .with_foreground_color(link_color)
                .with_underline_color(link_color);
        }

        style_runs.push((
            start..end,
            StyleAndFont::new(paragraph_style.font_family, properties, text_style),
        ));
        start = end;
    }

    if text.is_empty() {
        style_runs.push((
            0..0,
            StyleAndFont::new(
                paragraph_style.font_family,
                paragraph_style.properties(),
                TextStyle::new(),
            ),
        ));
    }

    InlineTextLayoutInput { text, style_runs }
}

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
