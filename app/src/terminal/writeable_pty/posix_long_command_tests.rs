use super::*;

fn decoded_script(line: &str) -> String {
    let start = line.find("printf %s '").unwrap() + "printf %s '".len();
    let end = start + line[start..].find('\'').unwrap();
    // A folded wrapper has newlines inside the quotes, which `base64 -d` ignores.
    let encoded: String = line[start..end].chars().filter(|c| *c != '\n').collect();
    String::from_utf8(STANDARD.decode(encoded).unwrap()).unwrap()
}

#[test]
fn ordinary_commands_are_typed() {
    assert!(!needs_wrapper(&"a".repeat(MIN_WRAPPED_BYTES - 1)));
    assert!(needs_wrapper(&"a".repeat(MIN_WRAPPED_BYTES)));
    assert!(!needs_wrapper("cat > f <<'EOF'\nx\nEOF"));
}

#[test]
fn bracketed_paste_wraps_only_from_the_larger_size() {
    // Between the two limits: typed as pasted with bracketed paste, wrapped without it.
    let between = "a".repeat(MIN_WRAPPED_BYTES_BRACKETED - 1);
    assert!(!needs_wrapper_for(&between, true));
    assert!(needs_wrapper_for(&between, false));
    // A multi-line command of the larger size is wrapped.
    let multi_line = format!("{}\nb", "a".repeat(MIN_WRAPPED_BYTES_BRACKETED));
    assert!(needs_wrapper_for(&multi_line, true));
    // A here-doc below the larger size keeps going in as pasted.
    assert!(!needs_wrapper_for(&"x\n".repeat(100_000), true));
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

#[test]
fn a_single_line_command_is_never_wrapped_with_bracketed_paste() {
    // #1191: the shell takes a pasted single line of any size quickly; the wrapper made it slower.
    for len in [MIN_WRAPPED_BYTES_BRACKETED, 1024 * 1024, 3 * 1024 * 1024] {
        assert!(!needs_wrapper_for(&"a".repeat(len), true), "{len}");
    }
    // Without bracketed paste the size alone still decides (#1129).
    assert!(needs_wrapper_for(&"a".repeat(MIN_WRAPPED_BYTES), false));
    // A CR also makes a multi-line paste.
    let cr = format!("{}\rb", "a".repeat(MIN_WRAPPED_BYTES_BRACKETED));
    assert!(needs_wrapper_for(&cr, true));
}

#[test]
fn the_typed_input_wrapper_folds_the_base64_and_carries_the_command_byte_for_byte() {
    // #1129: a shell without bracketed paste gets the keys one by one; no line may be long.
    let command = format!(
        "cat > f <<'EOF'\n{}\nEOF\necho \"it's\" `x` $y \\\\ \r\n caf\u{e9} \u{1f600}",
        "line with 'quotes' and $vars\n".repeat(20_000)
    );
    let text = wrapper_lines_for_typed_input(&command);
    assert!(
        text.lines().all(|l| l.len() <= FOLD_WIDTH + 400),
        "a line is too long"
    );
    assert!(!text.contains('\r'));
    assert_eq!(
        decoded_script(&text),
        format!("rm -f \"$__pc\"; unset __pc\n{command}\n")
    );
    // Quote parity: the only single quotes are the two around the base64.
    assert_eq!(
        text.matches('\'').count(),
        4,
        "{}",
        text.matches('\'').count()
    );
}

#[test]
fn the_typed_input_wrapper_has_the_same_decode_and_cleanup_shell_as_the_one_line_form() {
    let one = wrapper_line("exit 3");
    let folded = wrapper_lines_for_typed_input("exit 3");
    assert_eq!(one, folded, "a short command folds to a single line");
    assert!(!folded.contains('\n'));
}

/// The cost a line editor pays redisplaying a typed line grows with the square of its length, so
/// the work of typing a wrapper is the sum of its lines' squares. Folding must make that grow
/// linearly with the command: 10x the size is about 10x the work, not 100x (#1129, #1191).
#[test]
fn the_typed_input_wrapper_redisplay_work_scales_linearly() {
    fn work(text: &str) -> u128 {
        text.lines().map(|l| (l.len() as u128).pow(2)).sum()
    }
    let small = work(&wrapper_lines_for_typed_input(&"x".repeat(100 * 1024)));
    let large = work(&wrapper_lines_for_typed_input(&"x".repeat(1024 * 1024)));
    let ratio = large as f64 / small as f64;
    assert!(
        (8.0..13.0).contains(&ratio),
        "work ratio {ratio} for 10x the size"
    );
    // The one-line form is the quadratic one this replaces.
    let one = work(&wrapper_line(&"x".repeat(1024 * 1024))) as f64
        / work(&wrapper_line(&"x".repeat(100 * 1024))) as f64;
    assert!(one > 80.0, "{one}");
    // And the widest folded line stays under macOS's MAX_CANON.
    let widest = wrapper_lines_for_typed_input(&"x".repeat(1024 * 1024))
        .lines()
        .map(str::len)
        .max()
        .unwrap();
    assert!(widest < 4096, "{widest}");
}
