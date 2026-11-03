# Triage: r18d4 (pipeline 3020340107)

## Unit failures
- invalid_diagram_outcomes_are_snapshotted #1496 — crates/editor/tests/snapshots/ was never committed (insta-snapshots never played); not a code regression.
- valid_diagram_summaries_are_stable #1496 — same cause.

## App tests
- windows: every pass crashed with FileNotFoundError before any case ran (harness: .refzdot parent dir never created), fixed in 7d3eac9e1 after this build.
- macos, linux-wayland: failed; non-Windows runs broken by the local `import winmap` (UnboundLocalError), fixed in 566bb0479 after this build.
