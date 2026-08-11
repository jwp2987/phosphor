# Outcome: macOS `login_item` runtime tests + CI job

Task: `app/src/login_item/macos.rs` had zero `#[test]` functions (unlike the
Windows side, which had 5 real registry tests that had simply never been
*executed* in CI). Mirror the Windows fix: add real runtime tests, and a
macOS job in `.github/workflows/pr-check.yml` that runs them, not just
compiles them.

Status: **written, unverified**. This environment is Linux and the hard
rules for this task forbid `cargo`/`rustc`/`nextest`/`script/precheck` —
nothing below has been compiled, let alone run. Everything is "should work
by construction," cross-checked by reading, not by execution.

## First finding: the task's own assumption was wrong

The brief guessed the macOS artefact would be "a `LaunchAgents` plist, most
likely — verify, do not assume." It verified false. `macos.rs` registers via
`SMAppService.mainAppService` (the modern `ServiceManagement.framework` API,
macOS 13+), called through raw `objc2` `msg_send!`s — there is no plist file
this code writes itself, and no legacy fallback path in this file for older
macOS versions (the module doc says registration just no-ops if the class
isn't found).

This matters because it changes what "hermetic, scratch-cleanable test" can
even mean here. The Windows registry path is addressable:
`register_in(hive, subkey, value_name, exe)` takes the subkey as a
parameter, so `windows_tests.rs` redirects it to a scratch key under
`HKCU\Software\Phosphor\LoginItemTests\...` and deletes it on `Drop`.
`SMAppService.mainAppService` has **no equivalent parameter**. Its identity
is derived from the calling process's code signature / bundle identifier —
a macOS security property, not something this Rust code (or any refactor of
it) controls. There is no `SMAppService::scratch("test-name")` constructor.

Concretely, that rules out a register/overwrite/idempotent-unregister trio
that touches real `SMAppService` state, for two independent reasons:

1. A `cargo nextest` test binary is not a signed `.app` bundle. Calling
   `registerAndReturnError`/`unregisterAndReturnError` from a test would
   most likely no-op via the existing `bundle_is_present()` guard (see
   below for why even that's not 100% certain) — giving **zero** real
   coverage of the mutation itself.
2. If it did *not* no-op, it would attempt to mutate real system-level
   login-item state under whatever identity the CI runner's test binary
   presents. That is exactly the class of side effect this task requires
   avoiding ("never touch the user's real login item entry"), and unlike
   Windows there is no scratch namespace to redirect it into instead.

## What was built

### `app/src/login_item/macos.rs` — minimal, behavior-preserving extraction

Pulled the two side-effect-free FFI calls the registration path makes
*before* it would ever reach register/unregister out into their own
functions, so they're independently callable from a test:

- `fn bundle_is_present() -> bool` — wraps `[NSBundle mainBundle]` and
  returns whether it's non-null. Previously inlined in the async closure.
- `fn sm_app_service_class() -> Option<&'static AnyClass>` — wraps
  `AnyClass::get(c"SMAppService")`. Previously inlined as an `if let Some`.

The production closure now calls these instead of inlining the same
`msg_send!`s — behavior is unchanged (same calls, same order, same
control flow), just named and callable from outside the closure. Also
hoisted the `objc2` imports (`class`, `msg_send`, `AnyClass`, `AnyObject`)
to file scope instead of the function-local `use` the original code had,
since they're now used in three places instead of one.

### `app/src/login_item/macos_tests.rs` — new, `#[cfg(target_os = "macos")]` via the module's own gate

Two tests, both calling real `objc2` runtime functions (no mocks):

- `sm_app_service_class_is_available` — asserts `sm_app_service_class()`
  is `Some`. This is the class-availability check the production code
  depends on to decide whether to attempt registration at all; it requires
  `ServiceManagement.framework` to actually be mapped into the process.
- `bundle_presence_check_does_not_crash` — calls `bundle_is_present()` and
  logs the result, without asserting a direction. See below for why.

Neither test writes, mutates, or reads any login-item state — they're
read-only capability/FFI-signature checks. Both are gated implicitly by
`mod.rs`'s existing `#[cfg(target_os = "macos")]` on the `macos` module
(mirrored from how `windows_tests.rs` rides `#[cfg(target_os = "windows")]`
on its parent), so they never build on Linux.

**Why `bundle_presence_check_does_not_crash` doesn't assert a value**: the
module's own comment claims `[NSBundle mainBundle]` is nil outside an app
bundle. But this codebase already has a *different* NSBundle nil-check idiom
elsewhere for the identical "are we running from a bundle" question —
`app/src/default_terminal/mac.rs`'s `can_become_default_terminal` checks
`bundleIdentifier != nil`, not `mainBundle` itself. That's a well-known
Apple gotcha: `[NSBundle mainBundle]` for a bare (non-bundled) executable
typically returns a *non-nil* bundle object representing the enclosing
directory, with a nil `bundleIdentifier` — not a nil bundle. Which of these
two behaviors this specific file's comment is actually right about is
**genuinely unverifiable from this (Linux) environment**, and guessing wrong
would mean committing a test that either passes for the wrong reason or
fails outright — both against this task's explicit rules. The test instead
verifies the one thing that's true either way: the `msg_send!` call
completes without crashing / triggering UB in the FFI signature, which is
the actual regression class most likely to hit unreviewed objc2 code. A
maintainer with real macOS hardware should run this, observe which way it
resolves, and tighten it to a real assertion.

### `app/build.rs` — explicit `ServiceManagement.framework` link

While tracing whether `sm_app_service_class_is_available` could even be
expected to pass, found that nothing in this crate explicitly links
`ServiceManagement.framework` — unlike `MetalKit` and `UserNotifications`,
which `build.rs` already links by name for the same reason (`objc2`'s
`AnyClass::get` only finds classes from frameworks actually mapped into the
process; it does not `dlopen` anything on your behalf). Whether
`ServiceManagement` happened to be pulled in transitively by something else
linked (e.g. via `cocoa`/AppKit) was not verifiable from here either. Added
the explicit link as a defensive, behavior-preserving fix: a no-op if it was
already transitively linked, and a real fix for a **previously-undetectable
"login-item registration silently never works"** class of bug if it wasn't.
This is exactly the kind of gap that "compile-only, never runtime-verified"
was hiding.

### `.github/workflows/pr-check.yml` — new `check-macos` job

Modeled on `check-windows` (this fork's existing convention) for the
three `cargo check` steps, and on the pinned oracle's CI shape
(`02b53fcd8:.github/workflows/ci.yml`, the `tests` job) for the nextest
steps specifically, per the coordinator's mid-task correction:

- Two-step compile-then-run (`--no-run` first, then a real run), matching
  the pin, so a compile break reads as a compile failure and not a
  confusing test failure.
- `taiki-e/install-action` pinned to the same SHA the Windows job already
  uses.
- Filtered to `-E 'test(/login_item/)'` rather than the pin's full
  `--workspace` run — the Linux jobs already cover everything portable, and
  this fork doesn't have the pin's self-hosted-runner budget. Commented
  explicitly as a deliberate narrowing versus the pin, as asked.
- `timeout-minutes: 120`, matching `check-windows` rather than the pin's 25
  minutes — the pin's 25-minute budget covers a full-workspace build on a
  larger self-hosted Mac runner; this job runs three `cargo check`
  invocations plus a nextest compile+run on generic `macos-latest`, which
  is closer in shape to `check-windows`'s existing budget than to the pin's
  narrower, differently-resourced one.

## Verification status

- **Not run, cannot run from here**: `cargo check`, `cargo nextest`,
  `rustc` — Linux + hard rule. Nothing about whether this compiles, whether
  `sm_app_service_class_is_available` passes, or which way
  `bundle_presence_check_does_not_crash` resolves has been confirmed.
- **Run, passed**: `rustfmt --check --edition 2024` on all three touched
  Rust files (`macos.rs`, `macos_tests.rs`, `build.rs`) — clean, no diff.
- **Run, passed**: `python3 -c "yaml.safe_load(...)"` on the full
  `pr-check.yml` after the edit — parses as valid YAML.
- **Checked by reading, not by running**: the `objc2::runtime::AnyClass::get`
  call signature and `c"..."` literal argument are unchanged from the
  original (working, shipped) code — only moved into a named function, not
  altered.

## Unfinished / explicitly out of scope

- **The register/overwrite/idempotent-unregister trio the brief asked to
  match from `windows_tests.rs` was not attempted**, for the architectural
  reason above — not skipped for lack of effort. The concrete way to close
  this for real: a disposable, ad-hoc-codesigned helper `.app` bundle built
  at test time with its own throwaway `CFBundleIdentifier`
  (`com.phosphor.logintest.<uuid>`), which would give `SMAppService` a
  genuine scratch identity to register/unregister against — the bundle-ID
  equivalent of `windows_tests.rs`'s scratch registry subkey, with cleanup
  (delete the bundle, unregister) mirroring `ScratchSubkey`'s `Drop`. This
  was not attempted here because it needs real macOS hardware to iterate on
  (bundle/Info.plist structure, ad-hoc codesigning, and whether
  `SMAppService.register()` behaves the same on a headless-but-logged-in CI
  runner as on a desktop session are all things I cannot check from Linux),
  and building it blind risks landing a CI job that hangs or flakes rather
  than one that's merely incomplete.
- **`bundle_presence_check_does_not_crash` doesn't assert a direction** —
  see the explanation above. Tightening it needs a maintainer running it on
  real hardware once.
- **The `ServiceManagement.framework` link in `build.rs` is unverified** —
  it's a defensive addition based on reading, not on having seen the class
  lookup fail without it.
- Did not edit `TODO.md` or `DECLINED.md`, per instructions.
