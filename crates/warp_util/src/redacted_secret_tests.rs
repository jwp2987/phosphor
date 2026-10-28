use super::RedactedSecret;

const SECRET: &str = "sk-do-not-log";

#[test]
fn debug_never_prints_the_value() {
    let secret = RedactedSecret::new(SECRET);
    assert_eq!(format!("{secret:?}"), "[REDACTED]");
    assert!(!format!("{secret:#?}").contains(SECRET));
    // Nested inside a derived `Debug`, as it is inside action payloads.
    let nested = vec![("x-api-key".to_owned(), secret)];
    let debug = format!("{nested:?}");
    assert!(!debug.contains(SECRET), "{debug}");
    assert!(debug.contains("x-api-key"), "{debug}");
}

#[test]
fn value_is_still_readable_through_expose() {
    let secret = RedactedSecret::new(SECRET);
    assert_eq!(secret.expose(), SECRET);
    assert_eq!(secret.into_inner(), SECRET);
}
