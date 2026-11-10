# Unit failures, build/r12a (pipeline 3050758880)
All four are test faults or a policy collision; fixes are on integ/r13 (worked, unproven). See docs/design/r13-fuzz2.md and r13-pin2.md.

- ai::agent_providers::tools::web_runtime::websearch_tests::real_exa_anonymous_search #2022: policy collision, the no-real-endpoint lock refuses a test that deliberately called the live Exa endpoint; now an opt-in live check
- ai::blocklist::agent_fuzz::fuzz_tests::random_agent_conversations_hold_the_invariants #2010: oracle fault, a reply whose body closes with no End is committed whole by the app (I4); audited in r13-fuzz2
- ai::blocklist::agent_fuzz::fuzz_tests::random_agent_conversations_hold_the_invariants_with_the_allowlist_on #2010: same oracle fault as above
- terminal::view::tests::a_precmd_functions_question_sends_the_pin_and_its_enter_to_the_read #2038: test fault, the pty recorder was read inside the view update where emitted events have not run yet
