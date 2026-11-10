# Triage: r18w8 (pipeline 3047198675)

## Unit failures
- model::escape_sequences::tests::test_ctrl_break_win32_input_sequence #2001 — TEST/parser gap: winit emits "pause" and Keystroke::parse did not know it; fixed in 9dc18d908. Does not explain #1906.
- backend::contract_tests::the_shell_arm_handles_directory_symlink_hardlink_and_tilde_targets #2001 — TEST: the shell arm runs under a scratch HOME, the test looked in the real one; fixed in 9dc18d908.
