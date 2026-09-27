//! #637: `whoami` must not present the auth facade's placeholder as an identity.

use warp_cli::agent::OutputFormat;

use super::write_whoami;
use crate::auth::{TEST_USER_EMAIL, TEST_USER_UID};

fn render(output_format: OutputFormat) -> String {
    let mut out = Vec::new();
    write_whoami(output_format, &mut out).expect("whoami should render");
    String::from_utf8(out).unwrap()
}

#[test]
fn whoami_never_prints_the_placeholder_identity() {
    for format in [OutputFormat::Pretty, OutputFormat::Text, OutputFormat::Json] {
        let rendered = render(format);
        assert!(!rendered.contains(TEST_USER_EMAIL), "{format}: {rendered}");
        assert!(!rendered.contains(TEST_USER_UID), "{format}: {rendered}");
        assert!(!rendered.contains("warp.dev"), "{format}: {rendered}");
        assert!(rendered.ends_with('\n'), "{format}: {rendered:?}");
    }
}

#[test]
fn whoami_reports_a_local_profile_without_an_account() {
    let pretty = render(OutputFormat::Pretty);
    assert!(pretty.contains("Local profile"), "{pretty}");
    assert!(pretty.contains("no account"), "{pretty}");

    assert_eq!(render(OutputFormat::Text), "local\n");

    let json: serde_json::Value = serde_json::from_str(&render(OutputFormat::Json)).unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "type": "local", "account": null })
    );
}

#[test]
fn whoami_rejects_ndjson() {
    let err = write_whoami(OutputFormat::Ndjson, &mut Vec::new()).unwrap_err();
    assert!(err.to_string().contains("ndjson"), "{err}");
}
