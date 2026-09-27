use cfg_aliases::cfg_aliases;

fn main() {
    // This sets the same cfg aliases as the `warp` crate, used to gate crash-recovery flags.
    cfg_aliases! {
        linux_or_windows: { any(target_os = "linux", windows) },
        enable_crash_recovery: { linux_or_windows },
    }

    inject_app_version();
}

/// Injects `PHOSPHOR_APP_VERSION`, read from the **app** crate's `Cargo.toml`,
/// for `version_string()`'s (in `src/lib.rs`) untagged-build fallback
/// (issue #640). Not an intra-doc link: this doc comment lives in the build
/// script, which is its own separate crate root with no `version_string`.
///
/// Why a build script has to do this at all: `app/Cargo.toml`'s `version` is
/// the decided single source of truth for Phosphor's release number, but this
/// crate is a *dependency* of `app` (not the other way around), and Cargo
/// builds dependencies before dependents. By the time `app`'s own build
/// script could run, this crate has already been compiled, so `app` cannot
/// push a value into this crate's compilation -- this crate has to reach out
/// and read `app/Cargo.toml` itself instead. That keeps exactly one place a
/// maintainer edits the version (this doesn't add a second copy anywhere);
/// it does add a relative-path dependency on the two crates' relative
/// locations in the workspace, which `cargo:rerun-if-changed` at least makes
/// visible the moment it goes stale (a missing/unparseable file fails the
/// build immediately, loudly, rather than silently falling back to a wrong
/// version).
///
/// A tiny hand-rolled parse rather than pulling in a TOML crate as a
/// build-dependency for one line: this only ever needs to find `version =
/// "<value>"` inside `[package]`; a `[dependencies]` entry shaped like `foo =
/// { version = "1.2" }` never *starts* a line with `version`, so this can't
/// misfire on one of those.
fn inject_app_version() {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is set by cargo");
    let app_cargo_toml = std::path::Path::new(&manifest_dir).join("../../app/Cargo.toml");

    println!("cargo:rerun-if-changed={}", app_cargo_toml.display());

    let contents = std::fs::read_to_string(&app_cargo_toml).unwrap_or_else(|err| {
        panic!(
            "warp_cli/build.rs: could not read {} (needed for the app's release version, issue #640): {err}",
            app_cargo_toml.display()
        )
    });

    let version = contents
        .lines()
        .find_map(|line| {
            let rest = line.trim().strip_prefix("version")?;
            let rest = rest.trim_start().strip_prefix('=')?;
            let rest = rest.trim_start().strip_prefix('"')?;
            rest.split('"').next()
        })
        .unwrap_or_else(|| {
            panic!(
                "warp_cli/build.rs: no 'version = \"...\"' line found in {}",
                app_cargo_toml.display()
            )
        });

    println!("cargo:rustc-env=PHOSPHOR_APP_VERSION={version}");
}
