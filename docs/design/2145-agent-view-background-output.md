# #2145: the agent view shows background-job output

Owner decision 2026-11-13: the agent view shows background-job output, the same as the terminal view.

## Root cause

Output between a Precmd and the next command start goes to a lazily created background block
(`model/early_output.rs` -> `Block::start_background`). It carries no conversation id, and
`Block::should_hide_block` hid it in `TranscriptScope::Conversation` unless the conversation was in its
sets or `shown_to_tty_reader()` (#2053). The text was in the grid, only hidden (render
`block_list_element.rs`, height `block.rs`).

## Change (simplest variant)

- Do NOT attach background blocks to the conversation: that is the agent-context path.
- In `should_hide_block`'s Conversation arm add `&& !self.shows_background_output()`, where
  `shows_background_output()` is `is_background()` and not an empty / typeahead-only output grid. Empty
  background blocks stay hidden by the existing `is_empty_background_block` rule.
- Display only: background blocks are never sent as agent context or captured as a requested command's output.
- Unchanged: `TranscriptScope::Unfiltered` and `Terminal`; the #2209 hand-off code.
- Heights: live background blocks already refresh their height on output (`update_live_block_height`); scope
  switches recompute through `set_transcript_scope` / `set_agent_view_state` -> `update_blocks_and_sumtree`.
- #2053 tests that asserted the tty-reader question hides again after the reader ends now assert it stays
  shown (owner decision, not a loosening).

## Tests

Unit (`blocks_test.rs`): content shown / empty hidden; unrelated terminal block still hidden; height on first
content, next command separate, no conversation id on the block, scope switches keep heights.
appci: `agent_bg_job_output`; `scroll_keys_position` judges the job's output in agent mode.
