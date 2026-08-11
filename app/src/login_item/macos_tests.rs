//! Runtime tests for `login_item::macos`.
//!
//! ## Why these tests look different from `windows_tests.rs`
//!
//! The Windows backend stores its login-item state in the registry, under a
//! `(hive, subkey, value name)` address it fully controls -- tests redirect
//! that address to a scratch subkey, write to it, read it back, and delete
//! it. That pattern doesn't carry over to macOS.
//!
//! `SMAppService.mainAppService` has no equivalent address to redirect. Its
//! identity is derived from the calling process's code signature / bundle
//! identifier, which is a macOS security property, not a parameter this
//! code -- or any refactor of it -- controls. Concretely, that rules out a
//! `register`/`overwrite`/`idempotent-unregister` trio that touches real
//! `SMAppService` state:
//!
//!   - A `cargo nextest` binary is not a signed `.app` bundle, so calling
//!     `registerAndReturnError` from a test would either no-op (the
//!     `bundle_is_present` guard -- or the equally plausible bundle-but-no-
//!     identifier case documented below -- would likely skip it, giving zero
//!     real coverage of the register/unregister call itself) or, if it did
//!     NOT no-op, would attempt to mutate real system login-item state under
//!     whatever identity the test binary presents -- exactly the kind of
//!     uncontrolled, non-scratch side effect this task requires tests to
//!     avoid.
//!   - There is no "scratch bundle identifier" constructor on `SMAppService`
//!     analogous to `ScratchSubkey` in `windows_tests.rs`: `mainAppService`
//!     is a singleton tied to the current process, not a name you pass in.
//!
//! So instead these tests cover the two real, side-effect-free FFI calls
//! this module actually makes before it would ever reach `register`/
//! `unregister`: the bundle-presence guard and the `SMAppService`
//! class-availability check. Both are genuine `objc2` runtime calls (not
//! mocks) and both had zero coverage before this file. See
//! `docs/sweep/outcome-macos-login-item.md` for the proposed follow-up that
//! WOULD give the register/unregister path real coverage (a disposable,
//! ad-hoc-signed helper `.app` bundle with its own scratch bundle
//! identifier) and why it isn't attempted here.

use super::*;

/// `SMAppService` ships in `ServiceManagement.framework` and requires macOS
/// 13 (Ventura) or later. `build.rs` now links that framework explicitly
/// (previously nothing in this crate referenced it, so `AnyClass::get`'s
/// lookup depended entirely on some other framework transitively mapping it
/// in -- unverified, and the kind of thing that can silently regress). This
/// test exercises the exact `objc2::runtime::AnyClass::get` call the
/// production path uses and asserts the class resolves on the CI runner
/// (`macos-latest`, which is well past Ventura).
///
/// This is a real runtime check, not a mock: if `ServiceManagement.framework`
/// stops being linked, or the class is renamed/removed upstream, this fails
/// for the same reason login-item registration would silently stop working
/// in production.
#[test]
fn sm_app_service_class_is_available() {
    assert!(
        sm_app_service_class().is_some(),
        "SMAppService should be resolvable on macOS 13+; if this fails, \
         check that ServiceManagement.framework is still linked in build.rs \
         and that `SMAppService` hasn't been renamed upstream"
    );
}

/// Real, non-mocked call into `[NSBundle mainBundle]`, exercising the exact
/// `msg_send!` machinery the production registration path depends on.
///
/// This deliberately does NOT assert a specific true/false value. The
/// module's own comment claims `[NSBundle mainBundle]` is nil outside an
/// app bundle, but a different NSBundle nil-check idiom already lives in
/// this codebase (`default_terminal::mac::can_become_default_terminal`
/// checks `bundleIdentifier`, not `mainBundle` itself, for exactly this
/// case) -- which one is actually nil for a bare `cargo nextest` binary is
/// genuinely unverified from this (Linux) environment. Asserting a specific
/// direction here without macOS hardware to check it against would risk
/// committing a test that passes for the wrong reason, or fails outright.
///
/// What this test DOES verify for real: the call completes without
/// crashing / triggering undefined behavior in the `msg_send!` signature,
/// which is the actual regression class most likely to hit unreviewed objc2
/// FFI code. A maintainer with real macOS hardware should tighten this to a
/// specific assertion once the true behavior is confirmed.
#[test]
fn bundle_presence_check_does_not_crash() {
    let is_present = bundle_is_present();
    log::debug!("bundle_is_present() reported {is_present} under cargo nextest");
}
