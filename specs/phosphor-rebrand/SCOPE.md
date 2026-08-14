# Phosphor rebrand — work scope

Scoping doc for renaming this fork from **Zap** to **Phosphor**, an 80s-terminal-
themed identity (DEC VT-series / green-and-amber CRT phosphor).

Two independent workstreams:

1. **Phosphor themes** — a low-risk identity anchor that touches no storage or
   packaging. Do this first; it makes the project *feel* like Phosphor with zero
   migration risk.
2. **The rename** — layered, so most of the risk is optional. The visible brand
   can change without moving any user data.

## Naming / namespace (decided)

- Brand name: **Phosphor**.
- Bare namespaces are taken but dead/unrelated: crates.io `phosphor` (a 2020
  PBR renderer, ~2k downloads, abandoned) and the GitHub `phosphor` org both
  exist. Living cross-domain conflict: **Phosphor Icons** (`phosphor-icons`,
  phosphoricons.com) — an icon set, not a terminal, so coexistence is fine.
- Grabbable and recommended: GitHub org **`phosphorterm`** (or `phosphor-term`),
  crate prefix **`phosphor-term`**, domain `phosphor.sh` / `getphosphor.dev`
  (verify at execution time).
- Reverse-DNS app id (see layer 3): propose `sh.phosphor.Phosphor` (or reuse the
  existing `dev.*` qualifier as `dev.phosphor.Phosphor`).

---

# Part 1 — Phosphor themes

## What exists

Themes are plain YAML in `themes/` (`one_dark.yaml`, `tokyo_night.yaml`,
`vscode_2026_dark.yaml`). Schema (from `themes/one_dark.yaml`):

```yaml
name: One Dark
background: '#282c34'
foreground: '#abb2bf'
accent: '#74ade8'
details: darker            # light | darker
terminal_colors:
  normal:  { black, red, green, yellow, blue, magenta, cyan, white }
  bright:  { black, red, green, yellow, blue, magenta, cyan, white }
```

Users install by dropping the file into the data dir's `themes/` and picking it
under Settings → Appearance → Themes.

## Work

1. Add `themes/phosphor_green.yaml` and `themes/phosphor_amber.yaml`. Proposed
   starting palettes (tune against the renderer):
   - **Phosphor Green** (P1): `background '#0b0f0a'`, `foreground '#33ff66'`,
     `accent '#00ff88'`, `details darker`; ANSI ramp = green-tinted (reds/blues
     kept legible but desaturated toward the green monochrome look).
   - **Phosphor Amber** (P3): `background '#100a00'`, `foreground '#ffb000'`,
     `accent '#ffcf5c'`, `details darker`; ANSI ramp = amber-tinted.
2. Decide whether either ships as a **default/bundled** theme (vs. user-copied).
   Bundled themes live under `app/assets/bundled/` — trace how the existing
   defaults are registered and add Phosphor there so a fresh install already
   looks the part. (This is the one place Part 1 touches app code.)
3. Optional, later: CRT flourishes (scanline overlay / phosphor glow / slight
   bloom) if the GPU renderer supports a post-effect. Out of scope for v1 —
   the monochrome palettes alone read unmistakably "80s terminal."

## Risk

Near-zero. New files + one registration point. No storage, packaging, or
identity changes. Ship as the first commit under the new identity.

---

# Part 2 — The rename (layered)

Reference counts (case-insensitive `zap`, excluding vendored `lib/rust-genai`):
**~792 files** — but the vast majority are internal identifiers that must NOT
change (see layer 4). The user-facing surface is small and bounded.

## Layer 1 — GitHub repo

Rename `jwp2987/zap` → `jwp2987/phosphor` (GitHub auto-redirects old URLs).
Update remotes locally. **Changes nothing in code or the running app.** Trivial.

## Layer 2 — User-visible brand (the real rebrand; low risk)

This is what makes the app present as Phosphor. It does **not** touch storage.

- **Display name lever:** `app/build.rs` reads `WARP_APP_NAME` (default `"Zap"`)
  and `WARP_APP_PUBLISHER` (default `"Zap"`). Changing the defaults to
  `"Phosphor"` (or setting the env in the build/bundle scripts) flips the
  primary display name in one place.
- **Hardcoded display strings** to sweep to "Phosphor":
  - `app/src/autoupdate/windows.rs:361,369` (`Channel::Stable/Oss => "Zap"`).
  - `app/src/ai/agent_providers/tools/web_runtime.rs:48` (`FALLBACK_UA = "Zap"`)
    and any other User-Agent construction (grep `Zap` UA sites).
  - `app/src/root_view.rs` window-title fallback (`"Zap"`), Task-Manager label.
  - Desktop entry `Name=`/`GenericName=` (see layer 3 for the *filename*).
- **Docs / web / assets:** `README.md`, `README.zh-CN.md`, `README.ja.md`,
  `docs/**`, `website/**` (22 files), `SECURITY.md`, `about.hbs`, `logo.svg`,
  `cliff.toml` changelog groups. These are prose/branding — safe to change; the
  translated READMEs/docs are the exception the i18n rule already carves out.
- **Prompt/UA identity strings** the agent sends (grep `"Zap"` in
  `prompt_renderer.rs`, `web_runtime.rs`) — decide whether the model should now
  identify the client as "Phosphor".

Effort: ~half a day of careful find/replace on a *reviewed* list (do NOT
blanket sed — see layer 4). Risk: low; worst case a stray display string.

## Layer 3 — Storage / system identity (the sharp edge; OPTIONAL, deferrable)

**Key finding: display (layer 2) and storage identity are separable.** You can
ship Phosphor-branded with layer 2 alone and leave every path as `zap`, so no
existing user loses data. Only touch this layer when you deliberately choose to,
and pair it with a migration.

What derives from the storage identity today:
- `crates/warp_core/src/channel/state.rs:86` `app_id()` and
  `crates/warp_core/src/app_id.rs` (`application_name` / `qualifier` /
  `organization`). Set at `state.rs:40` (`AppId::new("dev", "zap", "Zap")`) and
  `app/src/bin/zap_oss.rs:29` (same triple, for the OSS channel config); on
  macOS, `app_id_from_bundle()` (`state.rs:275`) overrides both from the
  bundle's `CFBundleIdentifier` at runtime.
- `crates/warp_core/src/paths.rs:254` `project_dirs()` →
  `project_dirs_for_app_id()` (`paths.rs:265`) →
  `directories::ProjectDirs::from(qualifier, organization, app_name)`
  (`paths.rs:287`). Linux maps `"Zap"` → dir `zap` (and `Zap*` → `zap-*`,
  `paths.rs:274`). This is the root of `~/.config/zap`, `~/.local/share/zap`,
  `~/.local/state/zap`, and the macOS/Windows equivalents
  (`%APPDATA%\zap\Zap\...`).
- **Secret storage service name — `ChannelState::data_domain()`
  (`crates/warp_core/src/channel/state.rs:122`), NOT
  `app/src/app_services/linux/mod.rs:163`.** `data_domain()` is `app_id().to_string()`
  (plus a `WARP_DATA_PROFILE` suffix in debug builds); `app/src/lib.rs:1278`
  passes it into `warpui_extras::secure_storage::register*`, which becomes the
  Secret Service `service` attribute on Linux (`secure_storage/linux.rs:26`)
  and the macOS Keychain service name (`secure_storage/mac.rs:14`).
  **Renaming this orphans stored API keys.**
  `app_services/linux/mod.rs:163`'s `default_service = "dev.zap.Zap"` is a
  **different mechanism**: it is the `org.freedesktop.Application` D-Bus
  well-known name used for single-instance activation. Both strings happen to
  read `dev.zap.Zap` today, so both change under a rename, but they fail
  independently — getting the D-Bus name wrong breaks single-instance
  activation, getting the keyring service wrong silently orphans every stored
  API key. (Corrected 2026-08-13 — this row previously conflated the two; see
  `LAYER3-PLAN.md` §1.)
- Channel / binary name — `crates/warp_core/src/channel/mod.rs:45,70`
  `Channel::Oss => "zap-oss"`; the `zap` symlink; `zap-tui-oss`
  (`crates/warp_tui/Cargo.toml`).
- Packaging identifiers — `app/channels/oss/dev.zap.Zap.desktop` (filename +
  `StartupWMClass=dev.zap.Zap`, `Icon=dev.zap.Zap`, `MimeType=x-scheme-handler/zap`;
  `Name=`/`Exec=` are already Phosphor — layer 2 landed),
  `script/windows/bundle.ps1` (`$WARP_BIN='zap-oss'`, `$BINARY_NAME`, InnoAppId),
  `resources/linux/{debian,rpm,arch}` package names, icon asset ids under
  `app/channels/oss/icon/`.

Two ways to do layer 3 when the time comes:
- **(A) Decouple and defer (recommended default).** Keep `app_id()`/paths/service
  names as `zap` / `dev.zap.Zap`; only the display brand changes. Zero migration,
  zero data loss. The internal "zap" identity becomes an invisible implementation
  detail (like the `warp_*` crate names already are).
- **(B) Full deep rename + migration.** Change `app_id()` → Phosphor, then write a
  one-time startup migration that, if the new data/secret dirs are absent and the
  old `zap` ones exist, copies/moves config + `data/` + keyring entries across,
  guarded so it runs once. Also rename the desktop file, package names, binary,
  bundle ids, and icons. This is the bulk of the risk and effort and should be
  its own milestone with real installs tested on all three OSes.

**Status 2026-08-13: option B is underway**, split across four parallel
agent branches (Rust identity/migration, Linux packaging, Windows+macOS
packaging, and this documentation/inventory pass). See `LAYER3-PLAN.md` for
the phasing and `RENAME-INVENTORY.md` for the completeness checklist — none
of the four branches has been compiled yet (builds were prohibited for this
round).

## Layer 4 — Internal code identifiers (DO NOT rename)

Crate names (`warp_core`, `warpui`, `zap_sftp`, `zap_sync`, …), module paths,
Rust symbols, and the `WARP_*` build env var names. Renaming these is pure churn
with no user benefit and would touch hundreds of files. They are the Warp/Zap
lineage internals and should stay, exactly as the code already keeps `warp_*`
crate names under the "Zap" brand. **Explicitly out of scope.**

## Recommended sequencing

1. Part 1 (themes) — immediate, safe, high-signal.
2. Layer 1 (repo rename) + Layer 2 (display brand) — the visible rebrand; ship
   together. Keep storage as `zap` (Layer 3 option A).
3. Grab the namespace (`phosphorterm` org, domain) so links in docs are stable.
4. Layer 3 option B (deep rename + data migration) — only if/when you commit to
   it; separate milestone, tested against real installs. Never do it twice.

## Open decisions

- ~~App-id qualifier: new `sh.phosphor.Phosphor` vs. reuse `dev.phosphor.Phosphor`.~~
  **DECIDED 2026-08-13 (maintainer): `dev.phosphor.Phosphor`** — keeps the
  existing `dev` qualifier so nothing about signing/notarization assumptions
  changes shape. See `LAYER3-PLAN.md` §3.
- ~~Binary/package name for the OSS channel.~~ **DECIDED 2026-08-13 (maintainer):
  `phosphor-oss`** (`Channel::Oss => "phosphor-oss"`). Keeping the `-oss` suffix
  preserves the channel distinction and avoids a symlink collision: `/usr/bin/phosphor`
  stays a symlink to the `phosphor-oss` binary exactly as it is a symlink to
  `zap-oss` today, so the deb/rpm rename is a package rename only. See
  `LAYER3-PLAN.md` §3.
- Does the agent's self-identity / User-Agent become "Phosphor" now (layer 2), or
  stay "Zap" until layer 3? (Recommend: change with layer 2 — it's display, not
  storage.)
- Ship Phosphor Green/Amber as bundled defaults, or user-installable only?
- **Still open (layer 3 execution, see `LAYER3-PLAN.md` §3):** copy vs. move the
  user's data during migration (recommendation: copy), and whether to keep a
  `zap://` scheme handler alongside `phosphor://` for existing links.

For the full layer-3 execution plan — migration ordering, the secure-storage
key inventory, packaging traps, phasing, and rollback — see
`specs/phosphor-rebrand/LAYER3-PLAN.md`.
