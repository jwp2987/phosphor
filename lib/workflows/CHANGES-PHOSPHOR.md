# Phosphor's vendored copy of workflows

- Source: https://github.com/warpdotdev/workflows at `4d483f3268651b54c3ec07542f0f35dca7e49ce8` -- copied in 2026-11-03 so Phosphor no longer depends on
  Warp's GitHub. The source files are byte-identical to that rev.
- What it is: Warp-original: specs/*.yaml compiled into Rust by workflows/build.rs (warp-workflows, warp-workflows-types).
- License: Apache-2.0 (LICENSE).

## Changes made when vendoring

- Kept as its own workspace. Dropped: .github, build_ts/ (the npm package), script/. workflows/Cargo.toml: `uneval` now points at ../../uneval. build.rs writes workflows/src/generated_workflows/ (gitignored by the vendored .gitignore).
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

- 2026-11-04, workflows/build.rs + workflows/src/lib.rs: the generated workflows go to
  `$OUT_DIR/generated_workflows/` instead of `src/generated_workflows/`, and lib.rs includes
  them from there (each `pub mod x { include!(...) }`). Writing into the source tree broke a
  cached rebuild of a fresh checkout: CI's r18u4 retry failed with E0583 (`generated_workflows`
  not found) because cargo considered the build script up to date while the checkout's clean
  had removed its output. Upstream's `cargo fmt -p warp-workflows` step inside build.rs is
  removed: it reformatted the vendored tree as a side effect of building. Checked with
  `cargo check -p warp-workflows` on the box. Re-pin: re-apply both.
