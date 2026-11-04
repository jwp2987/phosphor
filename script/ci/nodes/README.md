# Rebuilding a CI node from scratch

Three machines run this project's GitLab CI. If one is lost, the scripts here rebuild it so that
it behaves like the one that was lost. Every version they install was **read off the live machine
on 2026-11-04**. A value that could not be read is marked `TODO-VERIFY` in the script, with the
reason. Do not bump versions during a rebuild. A version bump is a separate commit with its own
CI run.

| node | GitLab tag | runner (GitLab description / id) | script | runs as |
|---|---|---|---|---|
| Linux build box `little-thing` (12 cores / 31 GiB, Ubuntu 26.04.1) | `little-thing` | (none) / 57876346, group runner | [`linux-box.sh`](linux-box.sh) | `sudo` from the admin account |
| Mac test box (M1 Pro, 16 GB, macOS 26.4.1) | `Mac` | `MacBookPro-gitrunner` / 57878698, group runner | [`macos.sh`](macos.sh) | the admin account (it calls `sudo` itself) |
| Windows 10 VM (8 vCPU, 22H2 19045) | `windows` | `phosphor-windows-vm` / 57941687, project runner | [`windows.ps1`](windows.ps1) | `-Phase Admin` elevated, then `-Phase User` as the runner user |

Every script makes **no changes by default**: it prints each step it would take. Pass `--apply`
(or `-Apply` on Windows) to run the steps. The scripts are idempotent: each step checks the current
state first, so you can run a script again after fixing whatever stopped it. None of them reads or
writes a secret. The runner token comes from the environment variable `GITLAB_RUNNER_TOKEN`, and the
machine-local files are restored by hand (see below).

## Order of operations

1. **Install the OS and create the accounts.**
   - Linux: Ubuntu 26.04 with the disk layout in `linux-box.sh` step 10 (btrfs `@`/`@home`/`@swap`,
     20 GB swapfile, `/tmp` on tmpfs) and the maintainer account `winters`.
   - Mac: FileVault on, the owner's admin account. `macos.sh` creates the standard user `gitrunner`.
   - Windows: the admin account `phosphor-admin` and the standard user `Winters`, who is the console
     desktop user.
2. **Clone this repo** onto the machine: the scripts read pins from it, and they run the repo's
   installers, such as `script/install_cargo_binstall`.
3. **Create the runner in GitLab** (next section). This gives you a `glrt-...` token.
4. **Run the script dry**, read what it prints, then run it with `--apply` and
   `GITLAB_RUNNER_TOKEN=<token>`. On Windows, run `-Phase Admin` first (elevated) and then
   `-Phase User` (as `Winters`).
5. **Do the MANUAL steps** the script prints at the end. On the Mac these are the TCC grants, the
   gitrunner login, the lock screen and the Local Network prompt. On Windows they are the Autologon
   password and checking QuickEdit.
6. **Restore the machine-local files** from your backup (see below).
7. **Prove the node with CI** (see "Verification").

## Registering runners

The runners use GitLab's runner authentication tokens (`glrt-...`). With those tokens, the tags,
"run untagged", "protected" and "lock to project" settings belong to the runner object in GitLab.
You set them when you create the runner, not with `gitlab-runner register` flags. Create a runner
in the group (`winters-phosphor`) for the box and the Mac, or in the project
(`winters-phosphor/phosphor`, id 88085499) for Windows. The values observed through the API on
2026-11-04:

| runner | tags | run untagged | access level | locked | `config.toml` |
|---|---|---|---|---|---|
| box | `little-thing` | no | `ref_protected` | no | `/etc/gitlab-runner/config.toml`: `concurrent = 2`, `name = "little-thing"`, shell executor |
| Mac | `Mac` | no | `ref_protected` | no | `/Users/gitrunner/.gitlab-runner/config.toml` (path TODO-VERIFY, as noted in `macos.sh`), shell executor |
| Windows | `windows` | no | `not_protected` (see note) | yes | `C:\GitLab-Runner\config.toml`: `concurrent = 1`, shell `powershell`, `builds_dir = C:\gl\b`, `cache_dir = C:\gl\c` |

Every job in `.gitlab-ci.yml` carries one of those three tags. A runner without its tag picks up
nothing, and "run untagged" must stay off, or a job meant for one platform could land on another.
Pipelines run only for protected refs (`main`, `build/*`, `v*`). Set **"Protected"** on a new
runner. The Windows runner was observed as `not_protected`, unlike the other two and unlike
`docs/gitlab-migration.md`. A rebuild should make it protected too, unless the owner decides
otherwise.

The scripts run `gitlab-runner register --non-interactive --url https://gitlab.com --token
"$GITLAB_RUNNER_TOKEN" --executor shell ...`. They do not register again when `config.toml`
already names the runner. If you register the same token twice, GitLab sees two runner managers
polling for the same runner.

## Machine-local files: restore from your backup

These files are **owner-provided and per machine**. They never go into the repo or into GitLab
CI/CD variables (owner rule, `docs/testing.md` 2.5 / 2.8). Restore them as the runner user, with
mode 600:

| node | directory | files |
|---|---|---|
| box | `/home/gitlab-runner/.config/phosphor-ci/` | `ci.env`, `owner-settings.toml` |
| Mac | `/Users/gitrunner/.config/phosphor-ci/` | `ci.env`, `owner-settings.toml` |
| Windows | `%USERPROFILE%\.config\phosphor-ci\` (`C:\Users\Winters\...`) | `ci.env`, `owner-settings.toml` |

- `ci.env` holds `KEY=VALUE` lines. The variable names are `APPCI_OLLAMA_URL` and
  `APPCI_OLLAMA_MODEL`.
- `owner-settings.toml` is an exact copy of the owner's Phosphor `settings.toml`.

Without them the app tests still run, with a bare profile, and the agent-provider cases skip.

Other things that are **not** in these scripts and have to come from you:

- the runner tokens (from GitLab);
- the Mac `gitrunner` password and the Windows `Winters` password used by Autologon;
- the project CI variable `APPCI_API_TOKEN` (masked, `read_api`), which lives in GitLab and survives
  losing a machine;
- ssh keys for the admin logins.

## Verification

Push any commit as `build/<name>` (protected, so it gets a pipeline). Then run these jobs in
order, playing the manual ones with `glab api -X POST projects/88085499/jobs/<id>/play` or
`script/ci/ci_seq.py build/<name> --tier fix`. A node is back when every job in its row is green,
apart from the known-failure lists that each suite already reads:

| node | jobs, in order | what each one proves |
|---|---|---|
| box | `guards` (automatic) | `python3`, git, the shared rustfmt toolchain through `/home/winters/.rustup` |
| | `check-linux` (automatic) | rustup ACL, `CARGO_HOME=/home/gitlab-runner/.cargo-ci` is writable, the `x86_64-unknown-linux-gnu` target, mold, `script/agent-cargo` dirs under `~/.phosphor-build` |
| | `licenses`, `advisories` | cargo-deny 0.20.2 in the runner's CARGO_HOME, network |
| | `build-linux-x86_64` | protoc, cmake, dpkg-deb, fakeroot, linuxdeploy; AppImage `--version` |
| | `test-linux` (with `TEST_FILTER` for a quick run) | cargo-nextest from the owner's `~/.cargo/bin` |
| | `app-test-linux-wayland` (`APPCI_TIER=smoke`), then `app-test-linux-x11` | sway on `/dev/dri` (video/render groups), wtype, grim, Xwayland, foot/xterm reference, dbus-run-session, gnome-keyring, local sshd, tesseract |
| | `integration-linux` (one shard) | xvfb-run, sshd + ssh-keygen |
| | `cli-linux` (dry run on `main` only) | the `x86_64-unknown-linux-musl` target **for 1.99.1**, zig 0.13.0, cargo-zigbuild |
| Mac | `diag-mac` | runner user, PATH, toolchain, `~/phosphor-profile` |
| | `check-macos` | Xcode, protoc, Rust 1.99.1 for gitrunner |
| | `build-macos-arm64` | Metal toolchain, cargo-bundle/cargo-about/create-dmg, codesign, hdiutil |
| | `test-warpui-core-macos` | cargo-nextest |
| | `app-test-macos` (`APPCI_TIER=smoke`) | Aqua session (not a daemon), TCC Accessibility + Screen Recording, swiftc-built helper tools, `screencapture`, Local Network answered. Exit 3 means `NEEDS_OWNER.txt` lists the missing grant |
| Windows | `diag-windows` | runner user, VS, git/perl/cmake/ISCC, `PROTOC` |
| | `guards-windows` | the VM's own PowerShell 5.1 parses every harness/CI script |
| | `check-windows` | VS Build Tools + SDK, Strawberry Perl, CMake |
| | `build-windows-x86_64` | Inno Setup (ISCC), cargo-about 0.8.4 |
| | `test-warpui-core-windows` | Git Bash runs `run_suite.sh`, cargo-nextest |
| | `app-test-windows` (`APPCI_TIER=smoke`) | the runner is in the console session (keys and screenshots work), the private Python 3.12 with paramiko and Pillow, side-by-side install and restore |

For a release, run the full matrix of `docs/testing.md` 2.9 ("Test tiers") on the rebuilt node
before trusting it.

## What the scripts deliberately do not do

- **Give the CI user sudo or admin rights.** None of the three runner users has them, and the jobs
  are written never to change their host. A missing tool fails the job with a message.
- **Install a newer version.** Homebrew on the Mac is the one layer that cannot be pinned (it
  installs the current formula). `macos.sh` lists what the lost Mac had, so you can compare.
- **Partition disks or set up FileVault/BitLocker.** These are notes in the scripts.
- **Recreate the stale 2018 Intel Homebrew in the Mac's `/usr/local`.** A rebuilt Mac has only
  `/usr/local/bin/gitlab-runner` there (see `macos.sh` step 3).
