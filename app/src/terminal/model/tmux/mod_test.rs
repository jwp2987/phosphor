use super::*;

#[test]
fn test_parse_generator_output() {
    let input = b"^^^cmd1|||output|||0$$$";
    let result = parse_generator_output(input);
    assert!(result.is_some());
    let event = result.unwrap();
    assert_eq!(event.command_id, "cmd1");
    assert_eq!(event.output, b"output");
    assert_eq!(event.exit_code, 0);

    let input = b"^^^cmd2|||multi\nline\noutput|||255$$$";
    let result = parse_generator_output(input);
    assert!(result.is_some());
    let event = result.unwrap();
    assert_eq!(event.command_id, "cmd2");
    assert_eq!(event.output, b"multi\nline\noutput");
    assert_eq!(event.exit_code, 255);

    let input = b"invalid input";
    let result = parse_generator_output(input);
    assert!(result.is_none());
}

#[test]
fn control_mode_command_detection() {
    assert!(command_starts_tmux_control_mode("tmux -CC"));
    assert!(command_starts_tmux_control_mode("tmux -CC attach -t x"));
    assert!(command_starts_tmux_control_mode("/usr/bin/tmux -u -CC new"));
    assert!(command_starts_tmux_control_mode(
        "ssh -t host \"tmux -CC attach\""
    ));
    assert!(!command_starts_tmux_control_mode("cat evil.txt"));
    assert!(!command_starts_tmux_control_mode("tmux attach"));
    assert!(!command_starts_tmux_control_mode("echo -CC tmux"));
    assert!(!command_starts_tmux_control_mode(""));
}
