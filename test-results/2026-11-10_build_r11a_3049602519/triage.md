# Unit failures, build/r11a (pipeline 3049602519)
All three are fixed on integ/r12 (worked, unproven). See docs/design/r12-keyfacts.md, r12-ps1case.md, r12-fuzz.md.

- terminal::model::key_facts::tests::a_question_printed_after_the_precmd_is_seen_on_the_grid_the_block_writes_to #1923: APP, the #1923 fact read the active block; what a shell prints between a prompt's Precmd and the command start is early output in a background block (fixed: block_receiving_output)
- ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants #2010: oracle fault, a broken reply (disconnect_after) and a user-cut reply were judged as finished replies (I4); fixed in the oracle
- ai::blocklist::agent_fuzz::fuzz_tests::paced_random_sequences_hold_the_invariants_with_the_allowlist_on #2010: same oracle fault as above
