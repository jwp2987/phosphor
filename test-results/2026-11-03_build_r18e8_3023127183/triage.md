# Triage: r18e8 (pipeline 3023127183, commit 6188e6fc1; release tier)

check-linux failed: E0599 `no method named prompt_fact` at app/src/terminal/model/typeahead_handoff.rs:2318, in the #1609 route-decision log line (merged e15abb3a9, first compiled here; r18e6, its unit run, was cancelled for a superseding head). Fixed in 76ac1c3d1. No tests ran; the pipeline was cancelled (Mac/Windows builds would fail the same way).

## Unit failures
- none ran (compile failure above)
