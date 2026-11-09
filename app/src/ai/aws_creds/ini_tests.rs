use super::*;
use crate::ai::aws_creds::test_support::Fixture;

fn config_text() -> String {
    [
        "\u{feff}# a comment",
        "; another comment",
        "[default]",
        "region = us-west-2",
        "output = json   # inline comment",
        "",
        "[profile dev]",
        "region=eu-west-1",
        "role_arn = arn:aws:iam::123456789012:role/Dev",
        "source_profile = base",
        "s3 =",
        "    max_concurrent_requests = 10",
        "    addressing_style = path",
        "sso_session = corp",
        "",
        "[profile   spaced  ]",
        "region = ap-south-1",
        "",
        "[sso-session corp]",
        "sso_start_url = https://corp.awsapps.com/start",
        "sso_region = us-east-1",
        "",
        "[services mine]",
        "dynamodb =",
        "  endpoint_url = http://localhost:8000",
        "",
        "[notaprofile]",
        "region = nowhere",
    ]
    .join("\n")
}

#[test]
fn parses_profiles_sso_sessions_comments_and_nested_blocks() {
    let files = ParsedFiles::parse(&config_text(), "");
    assert_eq!(files.profiles["default"]["region"], "us-west-2");
    // Inline comments are stripped, the nested `s3 =` block is not mistaken for properties.
    assert_eq!(files.profiles["default"]["output"], "json");
    let dev = &files.profiles["dev"];
    assert_eq!(dev["region"], "eu-west-1");
    assert_eq!(dev["source_profile"], "base");
    assert!(!dev.contains_key("max_concurrent_requests"));
    assert!(!dev.contains_key("addressing_style"));
    assert_eq!(dev["sso_session"], "corp");
    assert_eq!(files.profiles["spaced"]["region"], "ap-south-1");
    assert_eq!(files.sso_sessions["corp"]["sso_region"], "us-east-1");
    // Non-profile config sections are not profiles.
    assert!(!files.profiles.contains_key("mine"));
    assert!(!files.profiles.contains_key("notaprofile"));
}

#[test]
fn credentials_file_names_profiles_bare_and_wins_over_config() {
    let config = "[profile dev]\nregion = us-east-2\naws_access_key_id = FROMCONFIG\n";
    let credentials = "[dev]\naws_access_key_id = FROMCREDS\naws_secret_access_key = s3cret\n[default]\naws_access_key_id = D\n";
    let files = ParsedFiles::parse(config, credentials);
    let dev = &files.profiles["dev"];
    assert_eq!(dev["aws_access_key_id"], "FROMCREDS");
    assert_eq!(dev["aws_secret_access_key"], "s3cret");
    assert_eq!(dev["region"], "us-east-2");
    assert_eq!(files.profiles["default"]["aws_access_key_id"], "D");
}

#[test]
fn keys_are_case_insensitive_and_values_keep_equals_signs() {
    let files = ParsedFiles::parse(
        "",
        "[x]\nAWS_Access_Key_Id = AK\naws_secret_access_key = a=b==\n",
    );
    assert_eq!(files.profiles["x"]["aws_access_key_id"], "AK");
    assert_eq!(files.profiles["x"]["aws_secret_access_key"], "a=b==");
}

#[test]
fn properties_before_any_header_and_garbage_lines_are_ignored() {
    let files = ParsedFiles::parse("orphan = 1\nnot a line\n[default]\nregion = r\n", "");
    assert_eq!(files.profiles.len(), 1);
    assert_eq!(files.profiles["default"]["region"], "r");
}

#[test]
fn a_hash_without_leading_space_is_part_of_the_value() {
    let files = ParsedFiles::parse("", "[x]\naws_secret_access_key = ab#cd\n");
    assert_eq!(files.profiles["x"]["aws_secret_access_key"], "ab#cd");
}

#[test]
fn load_reads_the_default_locations_and_missing_files_are_empty() {
    let fx = Fixture::new();
    assert!(ParsedFiles::load(&fx.env()).profiles.is_empty());

    fx.config("[default]\nregion = us-west-1\n");
    fx.credentials("[default]\naws_access_key_id = AK\naws_secret_access_key = SK\n");
    let files = ParsedFiles::load(&fx.env());
    assert_eq!(files.profiles["default"]["region"], "us-west-1");
    assert_eq!(files.profiles["default"]["aws_access_key_id"], "AK");
}

#[test]
fn load_honours_the_file_location_variables_including_tilde() {
    let other = tempfile::tempdir().expect("a second temp dir");
    let creds_path = other.path().join("creds");
    std::fs::write(&creds_path, "[other]\naws_access_key_id = ALT\n").expect("write creds");

    let fx = Fixture::new()
        .set("AWS_CONFIG_FILE", "~/alt/config")
        .set("AWS_SHARED_CREDENTIALS_FILE", &creds_path.to_string_lossy());
    fx.write("alt/config", "[profile other]\nregion = sa-east-1\n");
    let files = ParsedFiles::load(&fx.env());
    assert_eq!(files.profiles["other"]["region"], "sa-east-1");
    assert_eq!(files.profiles["other"]["aws_access_key_id"], "ALT");
}
