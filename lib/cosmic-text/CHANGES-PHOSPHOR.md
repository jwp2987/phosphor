# Changes made by the Phosphor fork

`lib/cosmic-text` is a **vendored, modified** copy of
[warpdotdev/cosmic-text](https://github.com/warpdotdev/cosmic-text) (a fork of
pop-os/cosmic-text by Jeremy Soller), version `0.12.0`, dual-licensed
**MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE` in this directory).

Apache-2.0 section 4(b) requires modified files to carry prominent notices.
Every modified file carries a three-line notice at the top pointing here.

## Pin

| | |
|---|---|
| upstream | `https://github.com/warpdotdev/cosmic-text.git` |
| commit | `a7c7b71497542758e08f77390b8efce543b3181f` (the rev the workspace used before vendoring) |
| fetched via | `gh api repos/warpdotdev/cosmic-text/tarball/a7c7b71497542758e08f77390b8efce543b3181f` |

Vendored: `src/`, `LICENSE-*`, `README.md`, `CHANGELOG.md`, `Cargo.toml`.
Omitted: benches, examples, tests, fonts, screenshots, CI scripts.
`Cargo.toml` changes: empty `[workspace]` table (so it is not absorbed into the
root workspace, as with `lib/rust-genai`); `[[bench]]`, `[dev-dependencies]`
and `[profile.test]` removed. Version and `[dependencies]`/`[features]` are
identical to upstream so the root `Cargo.lock` entry stays valid (only its
`source` line differs: path dependency, no git source).

## Modified files

| file | what changed |
|---|---|
| `src/shape.rs` | `ShapeWord::new_in_buffer`: skip shaping the empty run when `start_run == start_egc` (first grapheme has attrs incompatible with the defaults). Shaping an empty run dropped glyphs of long words (#864). Runs use absolute byte offsets, so no index bookkeeping changes for non-empty runs. `ShapeWord::new_in_buffer` (#940): a word longer than `MAX_SHAPED_WORD_BYTES` (128 KiB) is shaped only up to that cap, bounding a multi-megabyte paste's layout memory; one invisible placeholder glyph (the font's space, ~0 width) stands for the unshaped tail's last character so glyph-derived offsets still reach the true end. |

## Re-pin steps

1. Fetch the new rev tarball with `gh api` into a scratch dir (not /tmp if full).
2. Replace `src/`, licences, README, CHANGELOG; re-apply the `Cargo.toml` edits above.
3. Re-apply the `shape.rs` change (check whether upstream fixed it first) and the notice header.
4. Update the pin table here; keep `version` and dependency set in sync with `Cargo.lock`.
5. Verify `git ls-tree -r HEAD lib/cosmic-text` is non-empty (`/lib/*` is gitignored; `!/lib/cosmic-text/` exempts it).
