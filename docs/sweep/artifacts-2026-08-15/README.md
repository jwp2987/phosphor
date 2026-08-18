# Recovered sweep artifacts — 2026-08-15 round

Rescued from a session scratchpad on 2026-08-17. They were living in `/tmp`,
unversioned, and `triage_out.txt` had **already been clobbered once** by another
agent writing the same filename. Nothing here is regenerated cheaply: the
commit lists come from walks over the pinned oracle.

## Authoritative — these back numbers quoted in `TODO.md`

| file | what it is |
|---|---|
| `all_commits.txt` | full upstream commit walk for the round |
| `fix_candidates.txt` | the fix-flavoured subset (the `116` figure) |
| `partials_clean.txt` | the PARTIAL candidates — **54 `===` blocks, not 34** |
| `triage_out.txt` | per-commit triage output |

**Read `partials_clean.txt` before quoting a PARTIAL count.** `TODO.md` said
"34 PARTIAL candidates, ~25 unverified" for two rounds. That 34 was a
`grep -B1 "PARTIAL"` artifact, which only catches commits whose *first* file was
PARTIAL; the correct extractor in the same session printed 54. Real remainder is
~44 unverified. Two other numbers in that section were similarly wrong — `~82`
excluded commits was never computed by any command (the real figure is
612 − 116 = **496**), and `~37` unaudited `default` entries was a count
difference standing in for a set difference (the real figure is **50**, all of
them cloud/account `FeatureFlag`s and out of scope).

The pattern is the same each time: an estimate written from a prose summary
rather than from a command. If a number here disagrees with a document, the file
is right.

## Incidental

The `*.sh` files are throwaway scripts from several different sessions, swept in
wholesale rather than risk dropping one that mattered. Most are unrelated to the
2026-08-15 round. Treat them as history, not as tooling — nothing in the repo
calls them, and they encode paths from other machines.

## Caveat on the clone

This checkout is **shallow, grafted at `02b53fcd8`**. Only ~232 upstream commits
exist locally, so any claim of the form "N upstream commits touch X" cannot be
verified here beyond that horizon without `git fetch --unshallow`. The
"~1,240 commits touch `warpui_core`/`warpui`" figure in `TODO.md` is in exactly
that category — unverifiable as the tree stands, not disproven.
