# Triage: round 15 targeted run, pipeline 3054716634 (build/r15a b06800e98)

## Unit failures (FAIL)

- ai::blocklist::controller::tests::stop_while_preprocessing_2104::a_new_turn_settles_the_calls_of_an_earlier_batch_that_is_still_preprocessing: APP, #2136 (test-linux, test-macos, test-windows; fixed after the build in c2773fc98)
- terminal::cli_agent_sessions::plugin_manager::claude::tests::truncate_chars_cuts_by_characters_not_bytes: APP (jq -r on Windows appends "\r" to hook payload text), #2167
- shellcmd::git::write::tests::discard_validates_then_restores_and_removes: TEST (git checkout converts line endings on Windows), #2167

## Windows LEAK rows (a test left a child process behind; nextest marks it leaky, the test itself passed)

These are the leaky children that held the job's output pipe open after nextest returned, so test-windows hung for 3 h (#2148, fixed by script/ci/run_bounded.py). The leaks themselves are tracked under #2148 until each test is shown to end its children.

- ai::blocklist::agent_fuzz::matrix::pairs_failing_command::on #2148
- ai::blocklist::agent_fuzz::matrix::pairs_question::on #2148
- context_chips::current_prompt::tests::test_chips_rerun_when_the_blocks_session_registers #2148
- context_chips::current_prompt::tests::test_unchanged_chip_refetch_does_not_respawn_on_click_command #2148
- workspace::view::tests::test_closing_a_code_pane_drops_its_editor_and_buffer #2148
- workspace::view::tests::test_closing_a_code_pane_releases_a_retained_editors_heavy_state #2148
- workspace::view::tests::test_open_file_with_target_event_preserves_requested_line #2148

## App and lab rows

- Linux lab (106 pass / 148 fail / 93 NOREF): triaged into #2151-#2166.
- macOS lab: no cases ran (Local Network dialog in front, build predates the auto-answer); then #2149 (teardown crash) and #2150 (6 h hang).
- macOS app: 4 FAIL test faults fixed in #2137; 6 cases lost to the dialog.
- Wayland app: fixed in #2139-#2146.
- Windows app / lab: not played by the driver (#2168); the Windows app cases were played by hand on the same build afterwards (job 17936839264).
