//! #857: values read from storage go through `validate` via `Setting::new`.
//! These two validators existed before #857 but were skipped at startup.

use settings::Setting as _;

use super::*;

#[test]
fn background_opacity_is_validated_on_load() {
    assert_eq!(
        *BackgroundOpacity::new(Some(0)).value(),
        BackgroundOpacity::MIN
    );
    assert_eq!(
        *BackgroundOpacity::new(Some(255)).value(),
        BackgroundOpacity::MAX
    );
    assert_eq!(*BackgroundOpacity::new(Some(42)).value(), 42);
    assert_eq!(*BackgroundOpacity::new(Some(1)).value(), 1);
    assert_eq!(*BackgroundOpacity::new(Some(100)).value(), 100);
}

#[test]
fn background_blur_radius_is_validated_on_load() {
    assert_eq!(
        *BackgroundBlurRadius::new(Some(0)).value(),
        BackgroundBlurRadius::MIN
    );
    assert_eq!(
        *BackgroundBlurRadius::new(Some(200)).value(),
        BackgroundBlurRadius::MAX
    );
    assert_eq!(*BackgroundBlurRadius::new(Some(10)).value(), 10);
    assert_eq!(*BackgroundBlurRadius::new(Some(64)).value(), 64);
}
