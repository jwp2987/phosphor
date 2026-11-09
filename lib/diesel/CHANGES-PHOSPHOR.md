# Phosphor's vendored copy of diesel

- Source: crates.io `diesel` 2.3.14 (the published tarball, minus Cargo.toml.orig / .cargo_vcs_info.json / Cargo.lock).
- Why: diesel 2.3.x caps `libsqlite3-sys` at `<0.39.0`, but RUSTSEC-2026-0459 (bundled SQLite < 3.54.0)
  is only fixed from libsqlite3-sys 0.39 (GitLab #1822). Upstream already lifted the cap on its main
  branch (diesel-rs/diesel#5339, "Extended libsqlite3-sys support to include 0.39") but has not
  released it. Drop this copy and the `[patch.crates-io]` entry once a diesel release carries it.
- License: MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE).

## Changes made when vendoring

- `[workspace]` added. Applied through `[patch.crates-io]` (root and fuzz/ if it resolves diesel from the registry).
- Not formatted with Phosphor's rustfmt config (lib/ is excluded from script/check_rustfmt).

## Source changes

- `Cargo.toml`: `libsqlite3-sys` requirement `>=0.17.2, <0.39.0` -> `>=0.17.2, <0.40.0` (same one-line change as diesel#5339). No .rs changes.
