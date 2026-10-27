use super::*;

fn constant(name: &str, value: &str) -> EnvVar {
    EnvVar::new(name.to_owned(), value.to_owned(), None)
}

/// nushell has no `export NAME=value`: a variable is `$env.NAME = <string>;`, and the value is
/// ALWAYS a quoted string, because a bare `1` or `true` would be stored as an int or a bool.
#[test]
fn nu_env_var_initialization_is_an_env_assignment_with_a_quoted_string() {
    assert_eq!(
        constant("FOO", "bar").get_initialization_string(ShellType::Nu),
        "$env.FOO = \"bar\";"
    );
    assert_eq!(
        constant("N", "1").get_initialization_string(ShellType::Nu),
        "$env.N = \"1\";"
    );
    assert_eq!(
        constant("MY VAR", "a \"b\" \\ \n c").get_initialization_string(ShellType::Nu),
        "$env.\"MY VAR\" = \"a \\\"b\\\" \\\\ \\n c\";"
    );
    // A command value runs as a nushell subexpression; the others keep their own syntax.
    let command = EnvVar {
        name: "TOKEN".to_owned(),
        value: EnvVarValue::Command(EnvVarSecretCommand {
            name: "t".to_owned(),
            command: "^gcloud auth print-access-token".to_owned(),
        }),
        description: None,
    };
    assert_eq!(
        command.get_initialization_string(ShellType::Nu),
        "$env.TOKEN = (^gcloud auth print-access-token);"
    );
    assert_eq!(
        constant("FOO", "bar").get_initialization_string(ShellType::Bash),
        "export FOO=bar;"
    );
}

#[test]
fn nu_env_var_collection_serializes_without_posix_syntax() {
    let collection = EnvVarCollection::new(
        None,
        None,
        vec![constant("A", "x y"), constant("B", "it's")],
    );
    assert_eq!(
        collection.export_variables_for_shell(ShellType::Nu),
        "$env.A = \"x y\"; $env.B = \"it's\";"
    );
}

/// What the app types must set the variables in a real nushell, whatever is in the values.
#[cfg(unix)]
#[test]
fn nu_env_var_initialization_sets_the_values_in_a_real_nushell() {
    let Some(nu) = [
        "/home/linuxbrew/.linuxbrew/bin/nu",
        "/opt/homebrew/bin/nu",
        "/usr/local/bin/nu",
    ]
    .into_iter()
    .map(std::path::PathBuf::from)
    .chain(
        std::env::var_os("PATH")
            .map(|path| {
                std::env::split_paths(&path)
                    .map(|d| d.join("nu"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default(),
    )
    .find(|candidate| candidate.is_file()) else {
        eprintln!("skipping: nu is not installed");
        return;
    };
    let values = [
        ("PLAIN", "value"),
        ("NUMBER", "1"),
        ("BOOL", "true"),
        ("SPACES", "a b  c"),
        ("QUOTES", "it's \"x\" `y` $z (w) ; | #"),
        ("BACKSLASH", "C:\\dir\\file \\"),
        ("MULTILINE", "line one\nline two\r\n"),
        ("UNICODE", "ünï ☃"),
        ("EMPTY", ""),
    ];
    for (name, value) in values {
        let init = constant(name, value).get_initialization_string(ShellType::Nu);
        let output = std::process::Command::new(&nu)
            .args(["--no-config-file", "-c"])
            .arg(format!("{init} print -n $env.{name}"))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&output.stdout),
            value,
            "{init}; stderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        // And it reaches child processes as the same string.
        let child = std::process::Command::new(&nu)
            .args(["--no-config-file", "-c"])
            .arg(format!("{init} ^printenv {name}"))
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&child.stdout).trim_end_matches('\n'),
            value.trim_end_matches('\n'),
            "{init}"
        );
    }
}
