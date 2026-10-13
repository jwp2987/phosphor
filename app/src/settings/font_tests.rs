//! #857: `font_size` from settings.toml was used unchecked. These pin the
//! accepted range to the settings UI's (1..=120) and check that every
//! out-of-range or non-finite value falls back to the default.

use settings::Setting as _;

use super::*;

#[test]
fn monospace_font_size_accepts_the_ui_range() {
    for size in [FONT_SIZE_SETTING_MIN, 13.0, 48.5, FONT_SIZE_SETTING_MAX] {
        assert_eq!(*MonospaceFontSize::new(Some(size)).value(), size);
    }
}

#[test]
fn monospace_font_size_rejects_out_of_range_values_from_storage() {
    // The values from the torture run: 1000 and 10000 closed every window,
    // 1e308 (inf as f32) blanked the terminal, 0 and -5 cost ~2.7 GB.
    for size in [
        1000.0,
        10000.0,
        500.0,
        120.5,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
        0.0,
        -5.0,
        0.5,
    ] {
        assert_eq!(
            *MonospaceFontSize::new(Some(size)).value(),
            DEFAULT_MONOSPACE_FONT_SIZE,
            "font_size {size} must not be used"
        );
    }
}

#[test]
fn notebook_font_size_rejects_out_of_range_values_from_storage() {
    for size in [1000.0, f32::NAN, 0.0, -1.0] {
        assert_eq!(
            *NotebookFontSize::new(Some(size)).value(),
            DEFAULT_NOTEBOOK_FONT_SIZE,
            "notebook_font_size {size} must not be used"
        );
    }
    assert_eq!(*NotebookFontSize::new(Some(20.0)).value(), 20.0);
}

#[test]
fn ui_bounds_match_setting_bounds() {
    // `appearance_page` derives its integer bounds from these; a fractional
    // bound would silently make the UI and the loader disagree.
    assert_eq!(FONT_SIZE_SETTING_MIN.fract(), 0.0);
    assert_eq!(FONT_SIZE_SETTING_MAX.fract(), 0.0);
}
