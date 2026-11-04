use std::{fs, path::Path};

use tempfile::{tempdir, TempDir};

use crate::{correct_command, rules::RuleId, Command, ExitCode, SessionMetadata};

/// Doesn't make use of any session metadata.
pub fn corrections_given_input_output(input: &str, output: &str) -> Vec<String> {
    let metadata = SessionMetadata::new();
    let command = Command::new(input, output, ExitCode(1));
    corrections(command, &metadata, std::iter::empty())
}

/// Same API as `correct_command` but simpler return type.
pub fn corrections_given_metadata(command: Command, metadata: &SessionMetadata) -> Vec<String> {
    corrections(command, metadata, std::iter::empty())
}

pub fn test_input_output_powershell(
    input: &str,
    output: &str,
    expected_chain: (&'static str, &'static str),
) {
    let command = Command::new(input, output, ExitCode(1));
    let mut metadata = SessionMetadata::new();
    metadata.set_shell(crate::Shell::PowerShell, Some("6.0.0"));
    let expected = format!("{}; if ($?) {{ {} }}", expected_chain.0, expected_chain.1);
    assert_eq!(
        corrections_given_metadata(command, &metadata),
        vec![expected]
    );

    let command = Command::new(input, output, ExitCode(1));
    let mut metadata = SessionMetadata::new();
    metadata.set_shell(crate::Shell::PowerShell, Some("7.0.0"));
    let expected = format!("{} && {}", expected_chain.0, expected_chain.1);
    assert_eq!(
        corrections_given_metadata(command, &metadata),
        vec![expected]
    );
}

pub fn corrections(
    command: Command,
    metadata: &SessionMetadata,
    ignored_rules: impl IntoIterator<Item = RuleId>,
) -> Vec<String> {
    correct_command(command, metadata, ignored_rules)
        .into_iter()
        .map(|correction| correction.command)
        .collect()
}

pub fn with_temp_directories(dir_paths: &[impl AsRef<Path>], test: impl Fn(TempDir)) {
    let tmpdir = tempdir().unwrap();
    for path in dir_paths {
        fs::create_dir_all(tmpdir.path().join(path.as_ref())).unwrap();
    }

    test(tmpdir)
}
