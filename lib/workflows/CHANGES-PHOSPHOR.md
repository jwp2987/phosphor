# Phosphor's vendored copy of workflows

- Source: https://github.com/warpdotdev/workflows at `4d483f3268651b54c3ec07542f0f35dca7e49ce8` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: Warp-original: specs/*.yaml compiled into Rust by workflows/build.rs (warp-workflows, warp-workflows-types).
- License: Apache-2.0 (LICENSE).

## Changes made when vendoring

- Kept as its own workspace. Dropped: .github, build_ts/ (the npm package), script/. workflows/Cargo.toml: `uneval` now points at ../../uneval. build.rs writes workflows/src/generated_workflows/ (gitignored by the vendored .gitignore).
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

None yet. Record every change to this copy here, with the issue number.
