use super::*;

fn decoded_script(line: &str) -> String {
    let start = line.find("printf %s '").unwrap() + "printf %s '".len();
    let end = start + line[start..].find('\'').unwrap();
    String::from_utf8(STANDARD.decode(&line[start..end]).unwrap()).unwrap()
}

#[test]
fn ordinary_commands_are_typed() {
    assert!(!needs_wrapper(&"a".repeat(MIN_WRAPPED_BYTES - 1)));
    assert!(needs_wrapper(&"a".repeat(MIN_WRAPPED_BYTES)));
    assert!(!needs_wrapper("cat > f <<'EOF'\nx\nEOF"));
}

#[test]
fn the_wrapper_is_one_line_and_carries_the_command_byte_for_byte() {
    // Quotes, here-docs, backslashes, `$`, backticks, CRLF and non-ASCII must all survive.
    let command = format!(
        "cat > f <<'EOF'\n{}\nEOF\necho \"it's\" `x` $y \\\\ \r\n caf\u{e9} \u{1f600}",
        "line with 'quotes' and $vars\n".repeat(10_000)
    );
    let line = wrapper_line(&command);
    assert!(!line.contains('\n') && !line.contains('\r'));
    assert_eq!(
        decoded_script(&line),
        format!("rm -f \"$__pc\"; unset __pc\n{command}\n")
    );
}

#[test]
fn the_file_removes_itself_first_and_the_status_is_the_commands() {
    let line = wrapper_line("exit 3");
    assert!(decoded_script(&line).starts_with("rm -f \"$__pc\"; unset __pc\n"));
    // The sourced file is the last thing that runs on success, so `$?` is its status.
    assert!(line.contains("then . \"$__pc\";"));
    // A failed decode removes the file and reports a failure, never runs a partial file.
    assert!(line.contains("else rm -f \"$__pc\"; unset __pc;"));
    assert!(line.contains("false; fi; }"));
}

#[test]
fn the_line_grows_by_a_third_only() {
    let command = "x".repeat(300_000);
    let line = wrapper_line(&command);
    assert!(line.len() < command.len() * 4 / 3 + 600, "{}", line.len());
}
