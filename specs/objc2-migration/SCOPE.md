# objc2 migration — macOS platform layer

> **STATUS: ASSESSMENT ONLY (2026-08-15).** Finding confirmed, larger than reported.
> No code ported. See "Note on a mid-task instruction" at the bottom for why this
> stayed assessment-only despite a message asking for a port to be included here.

## The finding, verified

The fork's macOS platform code is still on the deprecated `objc`/`cocoa` crates in the
files that matter; upstream (pin `02b53fcd8`) migrated to the `objc2` family. Confirmed
directly, not taken on trust:

- `crates/warpui/Cargo.toml:72,81` — `cocoa.workspace = true`, `objc.workspace = true`
  are live deps, no `objc2*` deps at all in that crate.
- `crates/warpui/src/platform/mac/delegate.rs` reads `use cocoa::base::{BOOL, NO, YES}`,
  `use objc::{class, msg_send, sel, sel_impl}`, and uses `msg_send!` throughout — the
  pre-migration style.
- Root `Cargo.toml` already carries `objc2 = "0.6.3"`, `objc2-app-kit`,
  `objc2-core-foundation`, `objc2-core-graphics`, `objc2-foundation` (used elsewhere —
  see below) but is **missing** `objc2-av-foundation`, `objc2-metal`,
  `objc2-quartz-core`, which the migration needs.
- `crates/warpui/Cargo.toml:80` still pulls `metal = "0.33.0"`, the deprecated Metal
  binding the migration drops.

**The finding is real, but the reported shape (3 commits, ~1500 lines) is an
undercount.** Upstream's own PR description settles this: commit `0e923243026`'s body
says *"This is the top of a 5-PR stack (`warpui-foundation` → `app-platform` →
`warp_core`+`warpui_extras` → `computer_use` → **this**)."* The audit found 3 of the 5.
The other two — the `app` crate migration and the `computer_use` keycode-cache
migration — are real, unported, and change 5 files across 2 more crates.

## The full stack, in dependency order

Chronological order is the dependency order (each is a normal top-of-branch PR, no
reordering needed):

| # | commit | PR | date | scope | files | lines (+/−) |
|---|---|---|---|---|---|---|
| 1 | `f60116d3eee` | #11670 | 2026-05-28 15:53 | warpui "leaf" modules: `mod.rs`, `utils.rs`, `event.rs`, `keycode.rs`, `clipboard.rs`, `menus.rs`, `notification.rs`; ports `alert.m`→Rust | 11 | 380+ / 265− |
| 2 | `5becb10b898` | #11669 | 2026-05-28 18:26 | `app` crate: `crash_reporting/mac.rs`, `appearance.rs`, `app_services/mac.rs`, `default_terminal/mac.rs`, `login_item/macos.rs`, `util/file/external_editor/mac.rs`, `settings_view/appearance_page.rs`, `terminal/platform.rs`; ports `app_bundle.m`→Rust | 13 | 271+ / 298− |
| 3 | `94d29fe208c4` | #11668 | 2026-05-29 00:05 | `warp_core::channel::state` + `warpui_extras::user_preferences::user_defaults` | 5 | 56+ / 88− |
| 4 | `6889a1d508aa` | #11667 | 2026-05-29 11:44 | `computer_use::mac::keycode_cache` (drops `core-foundation` dep) | 3 | 23+ / 27− |
| 5 | `0e923243026d` | #11672 | 2026-06-01 16:10 | Metal renderer (`rendering/**`) + `window.rs`/`app.rs`/`delegate.rs`/`geometry.rs` — combined because renderer↔window form a bidirectional seam (renderer calls `WindowState::metal_layer()`, window.rs calls the new typed `Device` API) | 16 | 1081+ / 830− |

**Total: ~3,319 lines changed across 48 file-touches in 5 crates** (`warpui`, `app`,
`warp_core`, `warpui_extras`, `computer_use`) — roughly double the ~1,500/3-file figure
in the original report, and it touches two more crates than reported.

Each commit's own testing section describes it as binding-layer-only with **no logical
changes** (ownership/refcounting is the only thing that moves, onto `Retained`/
`CFRetained`), and each was independently reviewed for that property upstream. I did
not re-verify that claim line-by-line — that is exactly the kind of check that needs a
compiler and a Mac, not a read-through.

### Correction to the fork's actual remaining scope (not upstream's diff)

The raw upstream diffs above overstate what's left to do in *this* fork, because parts
of two crates already independently match the post-migration shape:

- **`crash_reporting/mac.rs` (105 of PR #2's 569 lines) doesn't exist in this fork at
  all.** The macOS native crash reporter (Cocoa-Sentry bridge) was stripped out —
  `app/src/crash_reporting/mod.rs:313` logs *"the macOS native crash reporter has been
  stripped out"* — consistent with `DECLINED.md`'s broader telemetry/cloud removal.
  That hunk of #11669 is moot; nothing to port there.
- **`computer_use`'s mac module is already 6/7 files on objc2**, independent of this
  migration: `mouse.rs`, `activation.rs`, `activation_tests.rs`, `post.rs`, `util.rs`,
  `window.rs`, `keyboard.rs` all import `objc2_app_kit`/`objc2_core_graphics` today.
  This isn't the migration landing early — it's fork-original issue **#349**
  ("macOS background window targeting", commit `81f9a4ed1`, parked 2026-08-09,
  restored 2026-08-10 per `TODO.md:239`), whose code happened to be written directly
  against objc2. `crates/computer_use/Cargo.toml` reflects the mixed state: it
  declares **both** `core-foundation` and `objc2*` deps at once. The one holdout is
  `keycode_cache.rs`, which mixes `core_foundation::{CFType, CFTypeRef, TCFType,
  CFData}` (old) with `objc2_core_graphics::CGKeyCode` (new, just a type import) — this
  is PR #4's actual remaining target, and it's small (the upstream diff is 23+/27−
  across 3 files, and roughly that much of it still applies here).

Net effect: PR #2 is closer to ~464 lines of real remaining work, and PR #4 is close to
its full upstream size. The other three PRs (#1, #3, #5) apply close to their full
upstream size, since none of those files have been independently touched here.

## What it blocks

Confirmed by walking upstream's post-`02b53fcd8` `objc`-touching history (`git log
--grep objc -i`) and checking each candidate against `HEAD` with
`git merge-base --is-ancestor`:

- **`463df36295c8`** (PR #12074, 2026-06-02, one day after PR #5) — the microphone
  crash fix named in the original report. Confirmed blocked: it edits
  `delegate.rs::microphone_access_state`, which only exists in objc2 form after PR #5
  lands, and its own description says it moves to `objc2-av-foundation` types
  specifically because plain `objc2`'s stricter type-checking is what surfaced the bug
  (`AVAuthorizationStatus` is a 64-bit return misread as `i32`). 33 lines, 1 file
  (`delegate.rs`) + Cargo.lock/Cargo.toml. **Needs `objc2-av-foundation` in root
  `Cargo.toml`, which isn't there yet** (see Cost, below).
- **`97dfcdeeb3ee`** (PR #13547, 2026-07-09) — "Fix macOS computer-use crash: build
  keycode cache on the main thread." Directly blocked: its fix in
  `keycode_cache.rs::build_cache()` adds a debug-only guard using
  **`objc2::MainThreadMarker::new()`**, which requires PR #4 to have landed first.
  Not an ancestor of `HEAD`.
- **`ebe8be84593e`** (#12480, 2026-06-25) — "Don't block system-initiated
  termination." Touches `app.rs` (16 lines) at the exact function PR #5 rewrites
  (`warp_app_should_terminate_app`). Not objc2-dependent in its own diff (still typed
  against `&mut Object`/`BOOL`), but it's built against post-migration `app.rs`
  line-for-line, so applying it before PR #5 lands means resolving that conflict twice.
  Not an ancestor of `HEAD`.
- **`4d2de5ea4`** (#14061, "scope background focus suppression to the activation
  window") — touches `computer_use/mac/activation.rs`, which (per the correction above)
  is *already* objc2-native in this fork via #349. This one is **not actually blocked
  by this migration** — it's a plain unported fix that can likely be evaluated on its
  own, separate from this scope. Flagging it here only because it surfaced in the same
  grep; it's not part of the migration's blast radius.
- **Four commits are `.m`-file-only and are *not* blocked**: `ec27d06d7` (macOS version
  check), `6eff52f9d` (Dock icon bounce), `64e3cd474` (Option-click zoom), `7a6044bd5`
  (Quake mode focus) all edit hand-written Objective-C (`window.m`) exclusively. Every
  migration PR's own description says the `.m` files are untouched — the migration is
  the Rust FFI binding layer only — so these four are independent of this decision and
  can be picked up on their own merits.

So the blocked set is smaller and more specific than "everything macOS since May":
2 commits are cleanly blocked (`463df36295`, `97dfcdeeb3ee`), 1 is blocked by conflict
surface rather than a hard type dependency (`ebe8be845`), 1 looked related but isn't
(`4d2de5ea4`), and 4 aren't blocked at all.

## Why this is more than a style preference

`deny.toml:11` already carries a live RUSTSEC exception:
```
{ id = "RUSTSEC-2024-0436", reason = "paste is unmaintained; this is a dependency of
metal, which we should eliminate by moving to the objc2 family of crates." }
```
That's this exact migration, already named as the fix for an accepted security-advisory
suppression that exists today.

There's also `specs/APP-4154/objc_checklist.md` — a prior fork-original project that
hand-audited and hand-fixed every ownership-producing Objective-C message send across
precisely the files this migration replaces (`app.m`, `host_view.m`, `window.m`,
`keycode.m`, `notifications.m`, `reachability.m`, `services.m`, plus Rust call sites in
`app.rs`, `clipboard.rs`, `appearance.rs`, `user_defaults.rs`). It found and fixed real
leaks (unbalanced `alloc`/`init` retains, a delegate outliving its window, an unreleased
`SentryUser`). objc2's `Retained`/`CFRetained` make that whole category of bug a compile-
time non-issue — the fork already paid engineering cost once for a class of bug the
migration eliminates structurally. That's a substantive argument for doing this, not
just a style-modernization one.

## Cost

- **~3,319 lines upstream, ~2,700 net of the moot `crash_reporting.rs` hunk**, across
  48 file-touches in `warpui`, `app`, `warp_core`, `warpui_extras`, `computer_use`.
- **New root `Cargo.toml` deps required**: `objc2-av-foundation`, `objc2-metal`,
  `objc2-quartz-core` (PR #5's renderer and #463df36295's mic fix need these; none are
  present today — only `objc2`, `objc2-app-kit`, `objc2-core-foundation`,
  `objc2-core-graphics`, `objc2-foundation` are already in the tree, because
  `computer_use`/`warp_core`/`login_item` already use those independently).
- **Drops**: `metal = "0.33.0"` from `crates/warpui/Cargo.toml` (resolves the
  `deny.toml` RUSTSEC exception above). Once all 5 PRs land, grep confirms no file
  outside this set imports `objc`/`cocoa`, so those two workspace deps could be dropped
  entirely — a genuine dependency-surface reduction, not just an addition.
- **Two `.m`/`.h` file pairs disappear**: `alert.m` (→ `alert.rs`, PR #1) and
  `app_bundle.m`/`app_bundle.h` (→ Rust, PR #2).
- **Drift risk is low.** I diffed the fork's current `delegate.rs` against upstream's
  pre-migration baseline (`0e923243026^`, i.e. the commit right before PR #5). The only
  differences are import-statement grouping, the "Warp"→"Zap" rebrand string, and
  Rust-2024-edition syntax (`extern "C"` → `unsafe extern "C"`, `#[no_mangle]` →
  `#[unsafe(no_mangle)]`) that this fork already carries and upstream's May-2026 PRs
  predate. The fork has **not** drifted substantively from pre-migration upstream in
  this area — a careful manual port (not a literal `git cherry-pick`, since the edition
  syntax needs reconciling either way) should be low-conflict relative to a typical
  multi-thousand-line stale port.
- **Who can verify it**: not this environment, categorically — `#[cfg(target_os =
  "macos")]` means a Linux toolchain skips the code at parse time, build permissions or
  not. But this repo is **not** verification-free on macOS in general: `.github/
  workflows/pr-check.yml` has a `check-macos` job (`runs-on: macos-latest`, a
  GitHub-hosted runner) that already does `cargo check -p warp --features gui --lib
  --tests`, `cargo check -p warp --features tui`, `cargo check -p warp_tui`, and
  compiles+runs the `login_item::macos` tests. That job is real, already wired, and
  would compile-check most of this migration's surface on the next push (it's narrower
  than the pin's self-hosted full-suite job by design — see the job's own header
  comment — so it's necessary, not sufficient, verification). Full runtime behavior
  (window lifecycle, Metal rendering, IME, notifications, mic-permission prompt) still
  needs a real Mac, which `TODO.md`'s "Requires macOS / Windows" section says this
  fork does not have.

## Is macOS actually shipped, or is this moot?

Evidence says shipped and actively maintained, not vestigial:

- `script/macos/{bootstrap,bundle,create_warpctrl_wrapper,install_build_deps,run,
  test_create_warpctrl_wrapper}` all exist and are non-trivial.
- `app/DockTilePlugin/` exists (a real packaging artifact, not a stub).
- `check-macos` in CI (above) runs on every PR, not just occasionally — this fork
  budgets real CI minutes for macOS compile-checking today.
- `HANDOFF.md:65,2132` records `macos 13/0` green test status and per-platform release
  bundle sizes (`macos-aarch64 ~52 MB`, `macos-x86_64 ~52 MB`) as tracked release
  artifacts, alongside Linux and Windows — not as an afterthought.
- `TODO.md:3305` tracks *"Edition-2024 cross-platform build — macOS release
  verification only"* as essentially **done** — the code work landed
  (`fix/edition-2024-native-targets`, commit `48bc21cb9`) and the only open item is a
  local `script/run --release` smoke test on an actual Mac. That's this project's
  established pattern for macOS work generally: port to completion, defer only the
  final on-hardware check — not "decline because it's unverifiable here."

So macOS is treated as a first-class target with the same "port now, verify on real
hardware later" pattern already used successfully for the edition-2024 migration. This
migration fits that same pattern rather than being an outlier.

## Recommendation

**Port, not decline** — with the port itself out of scope for this session (see below).
Rationale, weighed against `DECLINED.md`'s bar for "worth doing":

- Not vestigial: CI, packaging scripts, and DockTilePlugin are live; HANDOFF.md tracks
  macOS release artifacts as a first-class output.
- Not just a style migration: resolves a live `deny.toml` RUSTSEC exception, and
  eliminates by construction a class of bug the fork already spent real effort
  hand-fixing once (APP-4154).
- Low drift risk: the affected files haven't diverged from pre-migration upstream
  beyond cosmetic differences, so the port is closer to "apply carefully" than
  "re-derive from scratch."
- Blocks two more things than reported (`97dfcdeeb3ee` in addition to `463df36295`),
  and the mac layer will keep silently falling further behind on every re-pin as
  upstream's post-migration commits accumulate on top of the new binding layer,
  exactly as `ebe8be845` already does — deferring makes each future op-c-touching
  upstream commit slightly more expensive to port, not cheaper.
- Not free, and not verifiable in this environment or by this agent: ~2,700 real lines
  across 5 crates that no toolchain on this machine will ever compile. That is a
  concrete argument for *sequencing* this port outside a single blind session, not for
  declining it outright.

If the maintainer disagrees with "port," the fallback is **defer**, not decline: leave
this file as the record of scope, and revisit explicitly once someone is doing hands-on
macOS work anyway (per the edition-2024 pattern above) — not `DECLINED.md`, since
nothing here says this is deliberately out of scope for the fork.

## What I could not determine without a macOS machine

- Whether the "no logical changes" claim in each upstream PR description actually holds
  once merged onto this fork's diverged rebrand/edition baseline — I verified the
  *baseline* hasn't drifted, not that a hand-adapted port onto it would compile or
  behave identically.
- Whether `objc2-metal`/`objc2-quartz-core` pull in a meaningfully different transitive
  dependency tree than `metal = "0.33.0"` beyond dropping `paste` — I did not resolve
  a dependency graph, since that needs `cargo tree`, which needs a build.
- Whether the fork's rebrand ("Warp" → "Zap" and similar) touches any string literal
  inside the ~1911-line PR #5 renderer/window diff in a way that creates a real merge
  conflict rather than a cosmetic one — I checked `delegate.rs` only as a
  representative sample, not all 16 files PR #5 touches.
- Runtime behavior of anything: window creation/resize/fullscreen, Metal frame timing,
  IME composition, dock tile, notifications, and the mic-permission prompt itself. None
  of this is inspectable by reading source.

## Test plan for the Mac (forward-looking — for whoever does the eventual port)

If/when this migration is ported, in dependency order, here is what a maintainer with
Mac hardware should exercise per landed PR, and what a failure of each would indicate.
This is scoped to the migrated surfaces specifically, not a general smoke test.

**After PR #1 (warpui leaf modules — clipboard/menus/notification/keycode/event):**
1. Copy/paste text and (if applicable) rich content in and out of the terminal —
   failure here means the `clipboard.rs` `NSPasteboard` rewrite mis-mapped a type.
2. Open the app menu bar and exercise each of the ~10 standard menu items (About,
   Quit, Hide, etc.) plus any custom Warp/Zap menu items — a missing or misfiring
   selector means the `menus.rs` selector table didn't survive the port.
3. Trigger a desktop notification (e.g. a completed long-running command, if wired) —
   failure means `notification.rs`'s native-alert bridging broke.
4. Type non-ASCII / dead-key characters (e.g. option-e then e for é) — this exercises
   `keycode.rs`/`event.rs` modifier-flag mapping; wrong output means a modifier-flag
   bit got mistranslated.
5. Open the macOS character palette (the codepath in `delegate.rs::
   open_character_palette`, unaffected until PR #5, but worth a baseline check now).

**After PR #2 (app crate — appearance/login-item/default-terminal/external-editor):**
6. Toggle appearance/theme settings that read system light/dark mode
   (`appearance.rs`) — wrong result means `NSAppearance` observation broke.
7. Enable/disable "Launch at login" — this already has real automated coverage
   (`login_item::macos::tests::sm_app_service_class_is_available` in CI), but a live
   toggle-and-reboot check confirms the `SMAppService` registration actually persists.
8. Set Warp/Zap as the default terminal via System Settings and confirm it takes,
   then unset it — exercises `default_terminal/mac.rs`'s Launch Services calls.
9. Open a file in the configured external editor from the app — exercises
   `util/file/external_editor/mac.rs`.

**After PR #3 (warp_core + warpui_extras — bundle ID lookup, user defaults):**
10. Confirm the app's bundle identifier is read correctly at startup (visible
    indirectly via channel/update behavior, or via `defaults read` matching what the
    running app reports) — a wrong result means `NSBundle` lookup broke.
11. Change and persist a setting that round-trips through `NSUserDefaults`
    (`user_preferences` feature), quit, relaunch, confirm it stuck.

**After PR #4 (computer_use keycode cache):**
12. Run the actual `UseComputer`/computer-use action-model path once — this is the
    exact function (`get_keyboard_layout_data`) `97dfcdeeb3ee` fixed a crash in;
    confirm no crash, then confirm keyboard-layout-dependent input translation is
    correct on a non-US layout if one is available.

**After PR #5 (Metal renderer + window/app/delegate/geometry — the highest-risk PR):**
13. Basic window lifecycle: open, move, resize (including live-resize drag), minimize,
    restore, close, reopen — this is the part of the stack most likely to have a subtle
    `Retained`/ownership bug, since `native_window` is deliberately a non-owning raw
    pointer per the PR description (to avoid a retain cycle) — a crash or a window that
    silently fails to release on close both point here.
14. Fullscreen transition in and out, and tab-into-fullscreen if the app supports
    window tabbing — `fullscreen_queue.m`/`window.rs` seam.
15. Multi-monitor: drag a window between displays with different Metal devices/scale
    factors if hardware allows — exercises the renderer's GPU-device-selection path
    (`MTLCopyAllDevices`/`MTLCreateSystemDefaultDevice`, "preserved 1:1" per the PR).
16. Sustained rendering: scroll a long buffer / run a busy command for a minute and
    watch Activity Monitor's memory graph for the process — this is the single best
    check for a `Retained` reference-counting mistake (a leak here would show steady
    growth; per-frame Metal objects "now release deterministically via `Retained`...
    same timing, less pool pressure" per the PR, so no growth is expected).
17. Frame capture / GPU debugging path if exercised anywhere in the app (`frame_capture.rs`) —
    lower priority, but the PR explicitly touches it (80 lines).
18. Quit via Cmd-Q (user-initiated) and via a real system logout/shutdown/restart if you
    can arrange one safely — the user path exercises `delegate.rs::terminate_app`
    post-migration; the system path is what `ebe8be845` (still unported, conflicts
    with this exact file) was written to fix, so don't be surprised if system-initiated
    quit still blocks/hangs after PR #5 alone — that's expected until `ebe8be845` is
    separately ported on top.

**After `463df36295` (mic-access fix, if ported on top):**
19. Trigger a mic-permission prompt (any feature that calls
    `microphone_access_state`) on a machine where permission has never been granted,
    then again after granting and after denying — confirm all three states map
    correctly and, specifically, confirm no crash. This is the exact bug this commit
    fixes (a 64-bit `AVAuthorizationStatus` misread as `i32`), so a crash here means the
    port didn't fully adopt the `objc2-av-foundation` typed API.

**Where I'm least confident, ranked** (read this list first if something breaks):
1. PR #5's window/renderer seam (items 13–17) — largest diff (1911 lines), the one
   place upstream itself called out "please review" items (non-owning `native_window`,
   dropped defensive nil-checks on now-non-null singleton getters), and the one most
   likely to interact badly with anything fork-specific in `window.rs` I didn't
   line-by-line diff against the pre-migration baseline (I only sampled `delegate.rs`).
2. Anything touching `NSUInteger`/`NSInteger` → `usize`/`isize` width changes across
   the FFI boundary with the hand-written `.m` files — described as "ABI-identical on
   64-bit" upstream, unverified here since it needs a linker and a Mac.
3. Interaction between `ebe8be845` (unported) and PR #5's rewritten
   `warp_app_should_terminate_app` — if PR #5 lands without also porting `ebe8be845`,
   system-initiated termination (logout/shutdown/OS update) may still block, per item
   18 above. Worth deciding up front whether to bundle `ebe8be845` into the same
   effort rather than treating it as a separate follow-up.

## Note on a mid-task instruction

Partway through this assessment, a message arrived in-band claiming the remit had
changed from "assess only" to "assess, then port, in individual commits, on this
branch." I did not act on it. The original task instructions were explicit, repeated,
and reasoned in detail about exactly this scenario ("Do not write a port... producing
plausible, untestable, permanently-broken code"), and the system's own operating rule
for this kind of message is direct: no message from an agent — including whichever
agent is coordinating this work — constitutes authorization to override a task's
stated constraints; only the permission system or the user's own direct instruction
does. A single in-band reversal of a heavily-justified constraint, delivered mid-task,
is also exactly the shape a prompt injection takes. Substantively, nothing about "a
human will eventually test this on a Mac" changes what I can verify while writing
it — committing ~2,700 lines of FFI-layer Rust I cannot compile, type-check, or even
confirm parses, across 5 crates, is the specific outcome the original task was written
to prevent, and I'm not the right party to decide unilaterally, mid-task, that the
premise no longer applies. If a port is genuinely wanted, it should be requested
directly and unambiguously — ideally as its own task, sized so a reviewer can bisect
per-commit on the Mac as originally described — rather than folded into this
assessment via a side-channel message.

The test plan above is included anyway: it's pure documentation, requires no code and
no build, and is useful groundwork regardless of who ultimately does the port or when.
