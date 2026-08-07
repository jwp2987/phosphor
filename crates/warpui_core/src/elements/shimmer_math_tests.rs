use super::*;

// -- shimmer_center ---------------------------------------------------------

#[test]
fn shimmer_center_is_zero_for_empty_or_single_glyph_text() {
    let config = ShimmerConfig::default();
    assert_eq!(shimmer_center(0, Duration::ZERO, &config), 0.0);
    assert_eq!(shimmer_center(1, Duration::from_secs(1), &config), 0.0);
}

#[test]
fn shimmer_center_sweeps_linearly_across_the_text() {
    // No padding, 5 glyphs: span is exactly 4 glyphs, swept over a 4s period,
    // so the center advances one glyph per second.
    let config = ShimmerConfig {
        period: Duration::from_secs(4),
        shimmer_radius: 2,
        padding: 0,
    };
    assert_eq!(shimmer_center(5, Duration::ZERO, &config), 0.0);
    assert_eq!(shimmer_center(5, Duration::from_secs(1), &config), 1.0);
    assert_eq!(shimmer_center(5, Duration::from_secs(2), &config), 2.0);
    assert_eq!(shimmer_center(5, Duration::from_secs(3), &config), 3.0);
}

#[test]
fn shimmer_center_loops_back_to_the_start_after_a_full_period() {
    let config = ShimmerConfig {
        period: Duration::from_secs(4),
        shimmer_radius: 2,
        padding: 0,
    };
    assert_eq!(
        shimmer_center(5, Duration::from_secs(5), &config),
        shimmer_center(5, Duration::from_secs(1), &config),
    );
    assert_eq!(
        shimmer_center(5, Duration::from_secs(4 * 3 + 2), &config),
        shimmer_center(5, Duration::from_secs(2), &config),
    );
}

#[test]
fn shimmer_center_starts_padding_glyphs_before_the_text() {
    let config = ShimmerConfig {
        period: Duration::from_secs(2),
        shimmer_radius: 6,
        padding: 8,
    };
    // At t=0 the band center sits exactly `padding` glyphs before glyph 0.
    assert_eq!(shimmer_center(3, Duration::ZERO, &config), -8.0);
}

// -- intensity_at -------------------------------------------------------

#[test]
fn intensity_at_is_full_strength_exactly_at_the_band_center() {
    let config = ShimmerConfig {
        period: Duration::from_secs(1),
        shimmer_radius: 4,
        padding: 0,
    };
    assert_eq!(intensity_at(2, 2.0, &config), 1.0);
}

#[test]
fn intensity_at_is_zero_at_and_beyond_the_band_radius() {
    let config = ShimmerConfig {
        period: Duration::from_secs(1),
        shimmer_radius: 4,
        padding: 0,
    };
    // Exactly at the radius (dist == radius) is defined as zero intensity.
    assert_eq!(intensity_at(6, 2.0, &config), 0.0);
    // Further out stays zero.
    assert_eq!(intensity_at(20, 2.0, &config), 0.0);
}

#[test]
fn intensity_at_falls_off_symmetrically_and_halfway_is_half_strength() {
    let config = ShimmerConfig {
        period: Duration::from_secs(1),
        shimmer_radius: 4,
        padding: 0,
    };
    let left = intensity_at(0, 2.0, &config); // 2 glyphs left of center
    let right = intensity_at(4, 2.0, &config); // 2 glyphs right of center
    assert_eq!(left, right);
    // Halfway to the radius, the cosine falloff is exactly 0.5.
    assert!((left - 0.5).abs() < 1e-6, "expected ~0.5, got {left}");
}

// -- shimmer_color_at -----------------------------------------------------

const BASE: ColorU = ColorU {
    r: 0,
    g: 100,
    b: 200,
    a: 255,
};
const SHIMMER: ColorU = ColorU {
    r: 200,
    g: 200,
    b: 0,
    a: 255,
};

#[test]
fn shimmer_color_at_zero_intensity_is_the_base_color() {
    assert_eq!(shimmer_color_at(BASE, SHIMMER, 0.0), BASE);
}

#[test]
fn shimmer_color_at_full_intensity_is_the_shimmer_color() {
    assert_eq!(shimmer_color_at(BASE, SHIMMER, 1.0), SHIMMER);
}

#[test]
fn shimmer_color_at_half_intensity_is_roughly_the_midpoint() {
    let mid = shimmer_color_at(BASE, SHIMMER, 0.5);
    // Allow +/-1 for u8 rounding through the f32 round trip.
    assert!((i32::from(mid.r) - 100).abs() <= 1, "r = {}", mid.r);
    assert!((i32::from(mid.g) - 150).abs() <= 1, "g = {}", mid.g);
    assert!((i32::from(mid.b) - 100).abs() <= 1, "b = {}", mid.b);
}

// -- additional coverage with a shared padded config -------------------------

fn config() -> ShimmerConfig {
    ShimmerConfig {
        period: Duration::from_secs(2),
        shimmer_radius: 6,
        padding: 8,
    }
}

#[test]
fn center_starts_before_the_text_and_sweeps_across_it() {
    // At t=0 the center sits `padding` glyphs before the first glyph.
    assert_eq!(shimmer_center(7, Duration::ZERO, &config()), -8.0);
    // Half a period in, the center is half way along the padded track:
    // 0.5 * ((7 - 1) + 2 * 8) - 8 = 3.
    assert_eq!(shimmer_center(7, Duration::from_secs(1), &config()), 3.0);
    // The animation loops every period.
    assert_eq!(shimmer_center(7, Duration::from_secs(2), &config()), -8.0);
}

#[test]
fn single_glyph_text_keeps_the_center_at_zero() {
    assert_eq!(
        shimmer_center(1, Duration::from_millis(500), &config()),
        0.0
    );
    assert_eq!(
        shimmer_center(0, Duration::from_millis(500), &config()),
        0.0
    );
}

#[test]
fn intensity_peaks_at_the_center_and_fades_to_zero_at_the_radius() {
    let config = config();
    assert_eq!(intensity_at(3, 3.0, &config), 1.0);
    // A glyph exactly `shimmer_radius` away has no intensity.
    assert_eq!(intensity_at(9, 3.0, &config), 0.0);
    // Intensity decreases monotonically with distance from the center.
    let near = intensity_at(4, 3.0, &config);
    let far = intensity_at(5, 3.0, &config);
    assert!(near > far);
    assert!(far > 0.0);
}

#[test]
fn color_lerp_hits_both_endpoints() {
    let base = ColorU::new(254, 253, 194, 255);
    let shimmer = ColorU::new(254, 255, 255, 255);
    assert_eq!(shimmer_color_at(base, shimmer, 0.0), base);
    assert_eq!(shimmer_color_at(base, shimmer, 1.0), shimmer);
}
