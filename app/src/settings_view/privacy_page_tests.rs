//! Tests for the Privacy settings page.
//!
//! Scope, deliberately narrow: the gate that decides whether the "Send crash reports" row is on
//! the page at all (issue #633). `CrashReportsWidget::should_render` also reads
//! `PrivacySettings`, whose `register_singleton` pulls in `AuthStateProvider`,
//! `WarpDrivePrivacySettings` and the user-preferences store; standing all of that up would test
//! the harness, not the gate. So the gate is asserted through
//! [`super::crash_reports_row_is_live`], which is the whole of the decision except the
//! enterprise `is_telemetry_force_enabled` override, and which `should_render` calls before it
//! touches any singleton.

use super::crash_reports_row_is_live;

/// The regression guard for issue #633.
///
/// `FeatureFlag::CrashReporting` is in `RELEASE_FLAGS` (`crates/warp_features/src/lib.rs`), so
/// the runtime flag is on in every release build. The `crash_reporting` **cargo** feature is
/// not: it is absent from `app/Cargo.toml`'s `default` list, and both `script/linux/bundle` and
/// `script/windows/bundle.ps1` reset `FEATURES` on their `oss` arm without it. Before the fix
/// the row was gated on the flag alone, so every shipped OSS build drew a privacy switch over a
/// subsystem that had been compiled out.
///
/// The assertion is written against `cfg!` rather than a hard-coded `false` so it holds in an
/// explicit `--features crash_reporting` build too. Note the asymmetry that follows: with the
/// feature ON this test passes either way, so it only bites in feature-OFF builds -- which is
/// the default, and what CI runs (`--features warp/gui`), and the configuration every shipped
/// OSS bundle uses.
#[test]
fn crash_reports_row_is_hidden_unless_the_subsystem_is_compiled_in() {
    assert_eq!(
        crash_reports_row_is_live(true),
        cfg!(feature = "crash_reporting"),
        "the crash-reports row must appear exactly when the crash_reporting cargo feature is \
         compiled in -- with the runtime flag on and the feature off it controls nothing (#633)"
    );
}

/// The other half of the same gate, unchanged by #633: `crash_reporting::init` returns early
/// when `FeatureFlag::CrashReporting` is off (`app/src/crash_reporting/mod.rs`), so the setting
/// is just as inert then, even in a build that compiled the subsystem in.
#[test]
fn crash_reports_row_is_hidden_when_the_runtime_flag_is_off() {
    assert!(!crash_reports_row_is_live(false));
}
