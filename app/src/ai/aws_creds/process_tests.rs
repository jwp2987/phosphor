use super::*;
use crate::ai::aws_creds::test_support::{Fixture, iso, run};

#[test]
fn splits_like_the_cli_for_quotes_and_plain_words() {
    assert_eq!(
        split_command("aws-vault exec --json  work").expect("splits"),
        ["aws-vault", "exec", "--json", "work"]
    );
    assert_eq!(
        split_command(r#"helper --name "two words""#).expect("splits"),
        ["helper", "--name", "two words"]
    );
    // A single quote groups under POSIX rules only: on Windows the AWS CLI (and the C runtime)
    // take it as an ordinary character.
    let single = split_command("helper 'single quoted'").expect("splits");
    if cfg!(windows) {
        assert_eq!(single, ["helper", "'single", "quoted'"]);
    } else {
        assert_eq!(single, ["helper", "single quoted"]);
    }
    assert_eq!(split_command(r#"helper "unterminated"#), None);
}

/// The Windows rules, checked on every platform.
#[test]
fn the_windows_split_groups_by_double_quotes_only() {
    let split = |line: &str| split_windows_command(line).expect("splits");
    assert_eq!(
        split("aws-vault exec --json  work"),
        ["aws-vault", "exec", "--json", "work"]
    );
    assert_eq!(
        split(r#"helper --name "two words" 'single quoted'"#),
        ["helper", "--name", "two words", "'single", "quoted'"]
    );
    // Backslashes are path separators, also inside quotes and at the end of a word.
    assert_eq!(
        split(r#""C:\Program Files\Helper\helper.exe" --dir C:\work\ x"#),
        [
            r"C:\Program Files\Helper\helper.exe",
            "--dir",
            r"C:\work\",
            "x"
        ]
    );
    // Before a double quote: 2n backslashes are n and the quote groups, 2n+1 are n and the
    // quote is literal.
    assert_eq!(
        split(r#"helper "say \"hi\" now" end"#),
        ["helper", r#"say "hi" now"#, "end"]
    );
    assert_eq!(
        split(r#"helper "C:\dir\\" next"#),
        ["helper", r"C:\dir\", "next"]
    );
    assert_eq!(split(r#"helper \"x"#), ["helper", r#""x"#]);
    // An empty quoted word is a word.
    assert_eq!(split(r#"helper "" x"#), ["helper", "", "x"]);
    assert_eq!(split_windows_command(r#"helper "unterminated"#), None);
    // An escaped quote does not close the word.
    assert_eq!(split_windows_command(r#"helper "open \""#), None);
}

#[cfg(unix)]
fn doc(version: u32, key: &str, token: &str, expiry: &str) -> String {
    format!(
        "{{\"Version\":{version},\"AccessKeyId\":\"{key}\",\"SecretAccessKey\":\"proc-secret\",\
         \"SessionToken\":\"{token}\",\"Expiration\":\"{expiry}\"}}"
    )
}

/// A `credential_process` value that prints `json` and exits 0.
#[cfg(unix)]
fn printing(json: &str) -> String {
    format!("printf '%s' '{json}'")
}

#[cfg(unix)]
#[test]
fn runs_the_command_and_reads_version_one_credentials() {
    let fx = Fixture::new();
    let command = format!(
        "sh -c \"{}\"",
        printing(&doc(1, "ASIAPROC", "proctoken", &iso(1800))).replace('"', "\\\"")
    );
    let creds = run(credentials(&fx.ctx(), &command)).expect("process creds");
    assert_eq!(creds.access_key_id, "ASIAPROC");
    assert_eq!(creds.secret_access_key, "proc-secret");
    assert_eq!(creds.session_token.as_deref(), Some("proctoken"));
    assert!(creds.expires_at.is_some());
}

#[cfg(unix)]
#[test]
fn a_nonzero_exit_reports_the_status_and_never_the_output() {
    let fx = Fixture::new();
    let err = run(credentials(
        &fx.ctx(),
        "sh -c 'echo SECRET-ON-STDERR >&2; echo SECRET-ON-STDOUT; exit 3'",
    ))
    .expect_err("exit 3");
    assert_eq!(err.kind(), "provider");
    assert!(err.detail().contains('3'));
    assert!(!err.detail().contains("SECRET"));
}

#[cfg(unix)]
#[test]
fn rejects_other_versions_bad_json_and_expired_credentials() {
    let fx = Fixture::new();
    let wrap = |json: String| format!("sh -c \"{}\"", printing(&json).replace('"', "\\\""));

    let err =
        run(credentials(&fx.ctx(), &wrap(doc(2, "A", "T", &iso(60))))).expect_err("version 2");
    assert_eq!(err.kind(), "invalid_config");

    let err = run(credentials(&fx.ctx(), &wrap("not json".to_string()))).expect_err("bad json");
    assert_eq!(err.kind(), "provider");

    let err = run(credentials(&fx.ctx(), &wrap(doc(1, "A", "T", &iso(-60))))).expect_err("expired");
    assert_eq!(err.kind(), "expired");
}

#[test]
fn a_missing_program_or_an_empty_command_is_an_error_not_a_panic() {
    let fx = Fixture::new();
    let err = run(credentials(
        &fx.ctx(),
        "definitely-not-a-real-program-xyz --flag",
    ))
    .expect_err("not found");
    assert_eq!(err.kind(), "provider");
    let err = run(credentials(&fx.ctx(), "   ")).expect_err("empty");
    assert_eq!(err.kind(), "invalid_config");
}
