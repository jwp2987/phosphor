# Unit failures, build/r10a (pipeline 3048440714)
All five are test faults; fixes are on integ/r11 (fix/r11-unit5, worked, unproven). See docs/design/r11-unit5.md.

- ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants #2010: oracle fault, a reply the user cut was read as a dropped call (I4); also #2022 (rig leaves web access on)
- ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants_with_the_allowlist_on #2010: same oracle fault as above
- terminal::view::tests::open_pane_mid_batch_cancel_sends_the_follow_up #2010: fixture fault, task did not exist (#1945 refuses the request)
- terminal::view::tests::a_line_typed_behind_a_deferred_enter_follows_that_enters_held_line_at_readiness #2010: recorder fault, did not listen for WriteHandoffUserBytesToPty
- terminal::view::tests::give_up_to_pty_tests::an_enter_that_finds_the_gate_gone_over_other_text_sends_only_the_held_line_and_then_acts #2010: wrong count, one ls counted twice
