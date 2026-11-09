use super::*;

#[test]
fn sh_quote_wraps_and_escapes_single_quotes() {
    assert_eq!(sh_quote("abc"), "'abc'");
    assert_eq!(sh_quote(""), "''");
    assert_eq!(sh_quote("it's"), "'it'\\''s'");
    assert_eq!(sh_quote("a b;$(x)"), "'a b;$(x)'");
}

#[test]
fn the_output_cap_is_ten_megabytes() {
    assert_eq!(MAX_OUTPUT_BYTES, 10_485_760);
}

#[test]
fn sh_quote_path_expands_only_a_leading_tilde_on_the_host() {
    assert_eq!(sh_quote_path("~"), "\"$HOME\"");
    assert_eq!(sh_quote_path("~/it's"), "\"$HOME\"'/it'\\''s'");
    assert_eq!(sh_quote_path("/a/~/b"), "'/a/~/b'");
    assert_eq!(sh_quote_path("~user/x"), "'~user/x'");
}
