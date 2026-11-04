#!/usr/bin/env bash
#
# linux-box.sh -- rebuild the Linux CI node (`little-thing`, GitLab tag `little-thing`) on a fresh
# Ubuntu/Debian host. Read script/ci/nodes/README.md first: order of operations, the runner
# registration, the machine-local files to restore and the verification jobs.
#
# DRY RUN BY DEFAULT: every step prints what it would do. --apply executes. Idempotent: each step
# checks the current state first and does nothing when it is already right, so re-running is safe.
#
#   sudo ./script/ci/nodes/linux-box.sh                 # dry run
#   sudo GITLAB_RUNNER_TOKEN=glrt-... ./script/ci/nodes/linux-box.sh --apply
#
# Run as root (sudo) from a checkout of this repo. The maintainer account (OWNER_USER, default
# `winters`) must already exist: the CI toolchain lives in ITS home (see step 6).
#
# EVERY PIN BELOW WAS OBSERVED ON little-thing ON 2026-11-04 (Ubuntu 26.04.1 LTS "resolute",
# kernel 7.2.3-1-t2-resolute, 12 cores / 31 GiB, btrfs). Anything not observed says TODO-VERIFY.
# Do not "upgrade while you are at it": a rebuilt node should behave like the lost one, and a
# version bump is its own commit with its own CI run.
#
# What this script does NOT do (by design; printed again at the end):
#   - restore ~/.config/phosphor-ci/{ci.env,owner-settings.toml} (owner's backup; never in the repo);
#   - create the runner in GitLab (that gives you the token; README.md "Registering runners");
#   - give gitlab-runner sudo (it has none on the observed box, and jobs must not change the host).

set -euo pipefail

# ------------------------------------------------------------------ pins (observed 2026-11-04)
OWNER_USER="${OWNER_USER:-winters}"           # holds the rustup toolchain the runner uses (read-only)
RUNNER_USER="gitlab-runner"                   # created by the gitlab-runner .deb (uid 997 / gid 974 observed)
RUNNER_HOME="/home/${RUNNER_USER}"
RUNNER_NAME="little-thing"                    # config.toml [[runners]] name; GitLab tag is also `little-thing`
RUNNER_CONCURRENT="2"                         # config.toml `concurrent` observed
GITLAB_URL="https://gitlab.com"
GITLAB_RUNNER_DEB_VERSION="19.5.1-1"          # gitlab-runner + gitlab-runner-helper-images (packages.gitlab.com)
RUST_TOOLCHAIN="1.99.1"                       # == rust-toolchain.toml
RUSTUP_DEFAULT="stable"                       # owner's rustup default (observed); CI never relies on it
RUSTUP_EXTRA_TOOLCHAINS="1.92.0 nightly"      # also present on the box (owner's; not used by CI)
PROTOC_VERSION="25.1"                         # /usr/local/bin/protoc, same pin as script/linux/install_build_deps
# Owner's ~/.cargo/bin (on PATH for every box job, see .gitlab-ci.yml):
CARGO_BINSTALL_VERSION="1.18.1"               # == script/install_cargo_binstall
CARGO_NEXTEST_VERSION="0.9.143"
CARGO_FUZZ_VERSION="0.13.2"
SCCACHE_VERSION="0.17.0"                      # owner's; not used by CI
# Runner's own CARGO_HOME (/home/gitlab-runner/.cargo-ci/bin; from its .crates.toml). Jobs install
# these themselves when missing (pinned in the repo scripts), so pre-seeding only saves a first-run fetch.
CARGO_DENY_VERSION="0.20.2"                   # == script/ci/cargo_deny
CARGO_MUTANTS_VERSION="27.1.0"
CARGO_ABOUT_VERSION="0.8.4"                   # == script/install_cargo_release_deps
CARGO_ZIGBUILD_VERSION="0.19.8"               # cli-linux asks for ^0.19
GIT_CLIFF_VERSION="2.14.2"                    # release-meta
ZIG_VERSION="0.13.0"                          # cli-linux, ~/.local/zig-0.13.0
NU_VERSION="0.116.1"                          # ~gitlab-runner/.local/bin/nu (shell cases); source of the binary: TODO-VERIFY
# Optional Python modules: NOT installed on the observed box (2026-11-04: PIL, paramiko and Xlib all
# missing for gitlab-runner). Off by default so a rebuilt box matches; INSTALL_OPTIONAL_PY=1 adds them.
INSTALL_OPTIONAL_PY="${INSTALL_OPTIONAL_PY:-0}"
# The observed box does NOT hold the gitlab-runner package (unattended-upgrades is on). HOLD_RUNNER=1
# adds an apt hold so a rebuilt box keeps the pinned version; default 0 matches what was observed.
HOLD_RUNNER="${HOLD_RUNNER:-0}"

# apt packages observed installed (dpkg-query, 2026-11-04). Versions are Ubuntu 26.04's; apt installs the
# archive's current build, which is the closest a rebuild can get. Grouped by who needs them.
APT_BUILD=(
  build-essential cmake pkg-config curl git unzip jq bc brotli binutils file acl
  python-is-python3 python3
  libssl-dev libfreetype-dev libexpat1-dev libgit2-dev libdbus-1-dev libfontconfig1-dev
  libasound2-dev libclang-dev clang-format liblzma-dev
  mold                       # .cargo/config.toml links x86_64-unknown-linux-gnu with -fuse-ld=mold
  fakeroot dpkg-dev          # build-linux-x86_64: dpkg-deb, fakeroot
)
APT_RUNTIME=(
  locales fontconfig zlib1g libx11-6 libxcb1 libxi6 libxcursor1 libxkbcommon-x11-0
  libwayland-client0 libwayland-egl1 libegl1 mesa-vulkan-drivers libvulkan1 yaru-theme-icon
)
APT_APPTEST=(
  xvfb xdotool xsel ffmpeg                       # x11 mode (wayland_session.sh --mode x11), integration-linux, usage-test-linux
  sway swaybg wtype grim wl-clipboard xwayland   # wayland/xwayland modes (owner-installed 2026-11-01)
  xterm foot                                     # reference terminals (foot 1.25.0, xterm 407)
  dbus dbus-x11 gnome-keyring                    # app launched under dbus-run-session + a private keyring
  openssh-server openssh-client                  # appci/sshd.py runs /usr/sbin/sshd as the CI user on 127.0.0.1
  tesseract-ocr tesseract-ocr-eng                # OCR in mode_proof / UI cases
  zsh fish tmux vim less htop                    # shell + alt-screen cases
  fonts-dejavu-core fonts-dejavu-mono fonts-liberation fonts-liberation-sans-narrow fonts-noto-core fonts-noto-mono
  fonts-droid-fallback fonts-lato fonts-urw-base35
)
APT_OPTIONAL_PY=(python3-pil python3-paramiko python3-xlib)   # NOT on the observed box; see INSTALL_OPTIONAL_PY
# Present on the box but NOT needed by CI (owner's tools): google-cloud-cli, tailscale, sccache. Not installed here.
# NOT present on the box although sometimes assumed: weston (not installed; the harness uses sway), clang
# (libclang-dev only), git-lfs (CI sets GIT_LFS_SKIP_SMUDGE), pwsh (integration-linux-pwsh installs it into
# the runner's home), imagemagick (`import` is the ffmpeg fallback only), dragon-drop (ui_drag_drop_file skips).

# -------------------------------------------------------------------------------- helpers
APPLY=false
for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=true ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done

step() { printf '\n== %s\n' "$*"; }
note() { printf '   %s\n' "$*"; }
ok()   { printf '   ok:   %s\n' "$*"; }
die()  { printf '   FAIL: %s\n' "$*" >&2; exit 1; }
# run CMD...: print it; execute only with --apply.
run() {
  printf '   + '; printf '%q ' "$@"; printf '\n'
  if $APPLY; then "$@"; fi
}
# as USER CMD...: run as another user with a login-like HOME, from / (a cwd in /root would be unreadable).
as() { local u="$1"; shift; run sudo -u "$u" -H env -C / "$@"; }
have_pkg() { dpkg-query -W -f='${db:Status-Abbrev}' "$1" 2>/dev/null | grep -q '^ii'; }

REPO="$(cd "$(dirname "$0")/../../.." && pwd)"

# ------------------------------------------------------------------------- 0. preflight
step "0. Preflight"
[ "$(id -u)" -eq 0 ] || die "run with sudo (apt, users, /usr/local, systemd)"
# shellcheck disable=SC1091
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in *" debian "*|*" ubuntu "*) ok "distro: ${PRETTY_NAME:-$ID}" ;; *) die "Debian/Ubuntu only (got ${ID:-unknown})" ;; esac
[ "${VERSION_CODENAME:-}" = resolute ] || note "WARNING observed box is Ubuntu 26.04 (resolute); this is ${VERSION_CODENAME:-unknown}: package names may differ"
getent passwd "$OWNER_USER" >/dev/null || die "owner user '$OWNER_USER' does not exist (create it first; its home holds the toolchain)"
[ -f "$REPO/rust-toolchain.toml" ] || die "run from a checkout of the phosphor repo ($REPO has no rust-toolchain.toml)"
grep -q "channel = \"$RUST_TOOLCHAIN\"" "$REPO/rust-toolchain.toml" \
  || note "WARNING rust-toolchain.toml no longer pins $RUST_TOOLCHAIN; update RUST_TOOLCHAIN here (and re-observe the box)"
note "mode: $($APPLY && echo APPLY || echo 'DRY RUN (no changes)')"

# ------------------------------------------------------------------------- 1. apt packages
step "1. apt packages"
want=("${APT_BUILD[@]}" "${APT_RUNTIME[@]}" "${APT_APPTEST[@]}")
[ "$INSTALL_OPTIONAL_PY" = 1 ] && want+=("${APT_OPTIONAL_PY[@]}")
missing=()
for p in "${want[@]}"; do have_pkg "$p" || missing+=("$p"); done
if [ "${#missing[@]}" -eq 0 ]; then
  ok "all ${#want[@]} packages installed"
else
  note "missing: ${missing[*]}"
  run apt-get update
  run env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "${missing[@]}"
fi

# ------------------------------------------------------------------------- 2. protoc
step "2. protoc $PROTOC_VERSION in /usr/local"
if [ -x /usr/local/bin/protoc ] && /usr/local/bin/protoc --version 2>/dev/null | grep -qx "libprotoc $PROTOC_VERSION"; then
  ok "/usr/local/bin/protoc is libprotoc $PROTOC_VERSION"
else
  run curl -fsSL -o /tmp/protoc.zip "https://github.com/protocolbuffers/protobuf/releases/download/v${PROTOC_VERSION}/protoc-${PROTOC_VERSION}-linux-x86_64.zip"
  run unzip -o /tmp/protoc.zip -d /usr/local bin/protoc 'include/*'
  run rm -f /tmp/protoc.zip
fi

# ------------------------------------------------------------------------- 3. gitlab-runner package
step "3. gitlab-runner $GITLAB_RUNNER_DEB_VERSION (packages.gitlab.com apt repo)"
if [ ! -f /etc/apt/sources.list.d/runner_gitlab-runner.list ]; then
  note "adds /etc/apt/sources.list.d/runner_gitlab-runner.list via GitLab's repo script (observed file is generated by it)"
  run sh -c 'curl -fsSL https://packages.gitlab.com/install/repositories/runner/gitlab-runner/script.deb.sh | bash'
else
  ok "apt repo present"
fi
cur="$(dpkg-query -W -f='${Version}' gitlab-runner 2>/dev/null || true)"
if [ "$cur" = "$GITLAB_RUNNER_DEB_VERSION" ]; then
  ok "gitlab-runner $cur installed"
else
  note "installed: ${cur:-none}"
  run env DEBIAN_FRONTEND=noninteractive apt-get install -y \
    "gitlab-runner=$GITLAB_RUNNER_DEB_VERSION" "gitlab-runner-helper-images=$GITLAB_RUNNER_DEB_VERSION"
fi
if [ "$HOLD_RUNNER" = 1 ]; then run apt-mark hold gitlab-runner gitlab-runner-helper-images; else note "not held (matches the observed box; HOLD_RUNNER=1 to hold)"; fi

# ------------------------------------------------------------------------- 4. runner user, groups, dirs
step "4. $RUNNER_USER user, groups and directories"
if ! getent passwd "$RUNNER_USER" >/dev/null; then
  $APPLY && die "$RUNNER_USER does not exist; the gitlab-runner .deb creates it (step 3 must run first)"
  note "(dry run) $RUNNER_USER does not exist yet: the gitlab-runner .deb of step 3 creates it"
fi
for g in video render; do   # /dev/dri/renderD128 for headless sway (gles2); observed groups=gitlab-runner,video,render
  if id -nG "$RUNNER_USER" 2>/dev/null | tr ' ' '\n' | grep -qx "$g"; then ok "$RUNNER_USER in $g"; else run usermod -aG "$g" "$RUNNER_USER"; fi
done
# Observed modes: home 750; .cargo-ci, .phosphor-build/*, .local 775; .config, .config/phosphor-ci, .ssh 700.
for d in "$RUNNER_HOME/.cargo-ci" "$RUNNER_HOME/.phosphor-build/targets" "$RUNNER_HOME/.phosphor-build/locks" \
         "$RUNNER_HOME/.phosphor-build/logs" "$RUNNER_HOME/.local/bin" "$RUNNER_HOME/builds"; do
  if [ -d "$d" ]; then ok "$d"; else as "$RUNNER_USER" mkdir -p "$d"; fi
done
for d in "$RUNNER_HOME/.config/phosphor-ci" "$RUNNER_HOME/.ssh"; do
  if [ -d "$d" ]; then ok "$d"; else as "$RUNNER_USER" mkdir -p "$d"; as "$RUNNER_USER" chmod 700 "$d"; fi
done
run chmod 750 "$RUNNER_HOME"
note "no ~/.phosphor-build/slots override file (observed absent: agent-cargo uses its default of 3 slots)"

# ------------------------------------------------------------------------- 5. the owner's rustup (shared, read-only)
# .gitlab-ci.yml: RUSTUP_HOME=/home/winters/.rustup, PATH starts with /home/winters/.cargo/bin, and CARGO_HOME is the
# runner's own /home/gitlab-runner/.cargo-ci (the registry cache must be writable). The runner only READS the owner's
# toolchains, so the owner's home grants it traverse-only (ACL user:gitlab-runner:--x on /home/winters, observed) and
# ~/.rustup and ~/.cargo are 775 (others r-x, observed).
step "5. Rust $RUST_TOOLCHAIN in /home/$OWNER_USER (owner's rustup; read by the runner)"
OWNER_HOME="$(getent passwd "$OWNER_USER" | cut -d: -f6)"
RUSTUP="$OWNER_HOME/.cargo/bin/rustup"
if [ ! -x "$RUSTUP" ]; then
  note "rustup-init (observed rustup 1.29.3, profile minimal)"
  as "$OWNER_USER" sh -c "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $RUSTUP_DEFAULT --no-modify-path"
else
  ok "rustup at $RUSTUP ($("$RUSTUP" --version 2>/dev/null | head -1))"
fi
# Components == rust-toolchain.toml (rustfmt, clippy, rust-analyzer; the `guards` job runs check_rustfmt with it).
as "$OWNER_USER" "$RUSTUP" toolchain install "$RUST_TOOLCHAIN" --profile minimal --component rustfmt,clippy,rust-analyzer
as "$OWNER_USER" "$RUSTUP" target add --toolchain "$RUST_TOOLCHAIN" x86_64-unknown-linux-gnu
# x86_64-unknown-linux-musl: cli-linux and bench-remote-server REQUIRE it. Observed 2026-11-04: installed for the
# `stable` toolchain only, NOT for 1.99.1 (so those jobs would fail today; reported, not fixed on the box). Installed
# here so a rebuilt box can run the release pipeline.
as "$OWNER_USER" "$RUSTUP" target add --toolchain "$RUST_TOOLCHAIN" x86_64-unknown-linux-musl
for t in $RUSTUP_EXTRA_TOOLCHAINS; do as "$OWNER_USER" "$RUSTUP" toolchain install "$t" --profile minimal; done
run setfacl -m "u:${RUNNER_USER}:--x" "$OWNER_HOME"
run chmod 775 "$OWNER_HOME/.rustup" "$OWNER_HOME/.cargo"

# ------------------------------------------------------------------------- 6. cargo tools in the owner's ~/.cargo/bin
step "6. cargo tools in /home/$OWNER_USER/.cargo/bin"
OWNER_PATH="$OWNER_HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin"
# cargo-binstall from the repo's SHA-verified installer (it reads BINSTALL_VERSION itself).
as "$OWNER_USER" env PATH="$OWNER_PATH" "$REPO/script/install_cargo_binstall"
binst() { as "$OWNER_USER" env PATH="$OWNER_PATH" RUSTUP_TOOLCHAIN="$RUST_TOOLCHAIN" cargo binstall --no-confirm --locked "$@"; }
binst "cargo-nextest@$CARGO_NEXTEST_VERSION"
binst "cargo-fuzz@$CARGO_FUZZ_VERSION"            # fuzz-smoke uses it with RUSTC_BOOTSTRAP=1 (no nightly needed)
binst "sccache@$SCCACHE_VERSION"                  # owner's; harmless
note "also in the owner's ~/.cargo/bin, not needed by CI: wgslfmt (script/install_cargo_test_deps), diesel"
note "check: cargo-binstall $CARGO_BINSTALL_VERSION, cargo-nextest $CARGO_NEXTEST_VERSION, cargo-fuzz $CARGO_FUZZ_VERSION"

# ------------------------------------------------------------------------- 7. runner's own CARGO_HOME + ~/.local
step "7. Pre-seed the runner's CARGO_HOME (/home/gitlab-runner/.cargo-ci) and ~/.local"
RUNNER_ENV=(env HOME="$RUNNER_HOME" CARGO_HOME="$RUNNER_HOME/.cargo-ci" RUSTUP_HOME="$OWNER_HOME/.rustup"
            RUSTUP_TOOLCHAIN="$RUST_TOOLCHAIN" PATH="$RUNNER_HOME/.cargo-ci/bin:$OWNER_HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin")
rbinst() { as "$RUNNER_USER" "${RUNNER_ENV[@]}" cargo binstall --no-confirm --locked "$@"; }
rbinst "cargo-deny@$CARGO_DENY_VERSION"
rbinst "cargo-mutants@$CARGO_MUTANTS_VERSION"
rbinst "cargo-about@$CARGO_ABOUT_VERSION"
rbinst "cargo-zigbuild@$CARGO_ZIGBUILD_VERSION"
rbinst "git-cliff@$GIT_CLIFF_VERSION"
if [ -x "$RUNNER_HOME/.local/zig-$ZIG_VERSION/zig" ]; then ok "zig $ZIG_VERSION"; else
  as "$RUNNER_USER" mkdir -p "$RUNNER_HOME/.local/zig-$ZIG_VERSION"
  as "$RUNNER_USER" sh -c "curl -fsSL https://ziglang.org/download/${ZIG_VERSION}/zig-linux-x86_64-${ZIG_VERSION}.tar.xz | tar -xJ -C $RUNNER_HOME/.local/zig-${ZIG_VERSION} --strip-components=1"
fi
# linuxdeploy: script/linux/install_linuxdeploy (called by build-linux-x86_64) puts it in ~/.local/bin; let the first build do it.
note "linuxdeploy: installed by the first build-linux-x86_64 run (script/linux/install_linuxdeploy), observed in ~/.local/bin"
if [ -x "$RUNNER_HOME/.local/bin/nu" ]; then ok "nu"; else
  note "nu $NU_VERSION -> $RUNNER_HOME/.local/bin/nu. How it was installed is TODO-VERIFY (release tarball assumed):"
  as "$RUNNER_USER" sh -c "curl -fsSL https://github.com/nushell/nushell/releases/download/${NU_VERSION}/nu-${NU_VERSION}-x86_64-unknown-linux-gnu.tar.gz | tar -xz -C /tmp && install -m 755 /tmp/nu-${NU_VERSION}-x86_64-unknown-linux-gnu/nu $RUNNER_HOME/.local/bin/nu"
fi

# ------------------------------------------------------------------------- 8. register the runner
# New-style runner authentication token (glrt-...). Tags, run_untagged=false and "protected" are properties of the
# runner in GitLab (set where you CREATE it), not register flags. README.md "Registering runners".
step "8. Register the runner (shell executor, name $RUNNER_NAME)"
RUNNER_CHANGED=false
if grep -q "^  name = \"$RUNNER_NAME\"" /etc/gitlab-runner/config.toml 2>/dev/null; then
  ok "/etc/gitlab-runner/config.toml already has runner '$RUNNER_NAME'"
elif [ -z "${GITLAB_RUNNER_TOKEN:-}" ]; then
  note "GITLAB_RUNNER_TOKEN is not set: skipping. Create the runner in GitLab (README.md), then re-run with it."
  $APPLY && note "(nothing registered)"
else
  run gitlab-runner register --non-interactive --url "$GITLAB_URL" --token "$GITLAB_RUNNER_TOKEN" \
      --executor shell --name "$RUNNER_NAME"
  RUNNER_CHANGED=true
fi
if [ -f /etc/gitlab-runner/config.toml ]; then
  if grep -q "^concurrent = $RUNNER_CONCURRENT\$" /etc/gitlab-runner/config.toml; then ok "concurrent = $RUNNER_CONCURRENT"
  else run sed -i "s/^concurrent = .*/concurrent = $RUNNER_CONCURRENT/" /etc/gitlab-runner/config.toml; RUNNER_CHANGED=true; fi
fi
# Observed unit (installed by the .deb): ExecStart=/usr/bin/gitlab-runner run --config /etc/gitlab-runner/config.toml
#   --working-directory /home/gitlab-runner --service gitlab-runner --user gitlab-runner; Restart=always, RestartSec=120.
if systemctl is-enabled gitlab-runner >/dev/null 2>&1 && systemctl is-active gitlab-runner >/dev/null 2>&1; then
  ok "gitlab-runner.service enabled and active"
else
  run systemctl enable --now gitlab-runner
fi
# A restart kills a running job, so only when this run changed the config (the runner also reloads config.toml itself).
if $RUNNER_CHANGED; then run systemctl restart gitlab-runner; fi

# ------------------------------------------------------------------------- 9. host settings the harness depends on
step "9. Host settings"
# #1567: XWayland refuses /tmp/.X11-unix unless root-owned (then no xterm reference). Observed root:root 1777.
if [ -d /tmp/.X11-unix ] && [ "$(stat -c '%U %a' /tmp/.X11-unix)" != "root 1777" ]; then
  run chown root:root /tmp/.X11-unix; run chmod 1777 /tmp/.X11-unix
else ok "/tmp/.X11-unix root 1777 (or not created yet)"; fi
note "/tmp is tmpfs (fstab, observed): the appci session falls back to short /tmp/pa-* runtime dirs there"
note "sudoers: NONE for $RUNNER_USER (observed: 'not allowed to run sudo'). Do not add any."
note "cron / systemd timers: none for $RUNNER_USER or $OWNER_USER (observed); linger off"
note "sshd: the system ssh.service is only for admin access; the harness starts its OWN /usr/sbin/sshd per case"
note "      (appci/sshd.py: UsePAM no, pubkey only, 127.0.0.1) - no sshd_config change is needed"

# ------------------------------------------------------------------------- 10. disk layout (notes only)
step "10. Disk layout (observed 2026-11-04; recreate at install time, this script does not partition)"
note "nvme0n1 466 GB: p1 300 MB vfat /boot/efi; p2 btrfs, subvolumes @ -> /, @home -> /home, @swap -> /swap"
note "fstab: / and /home 'noatime,autodefrag'; /swap/swapfile 20 GB; /tmp tmpfs mode=1777"
note "cargo targets: /home/gitlab-runner/.phosphor-build/targets/<label> (ci-test, ci-check-linux, ci-fuzz, ci-loom,"
note "  ci-shuttle, ci-mutants, ci-mutants-core, ci-bench), bounded by script/ci/prune_target.sh (caps 120/60 GB,"
note "  wipes below 80 GB free). Plan >= 300 GB free for /home. Observed: 342 of 466 GB used, 119 GB free."

# ------------------------------------------------------------------------- 11. restore + verify
step "11. MANUAL: restore machine-local files from your backup (never in the repo or GitLab variables)"
note "$RUNNER_HOME/.config/phosphor-ci/ci.env              mode 600, owner $RUNNER_USER"
note "    KEY=VALUE lines; variable names: APPCI_OLLAMA_URL, APPCI_OLLAMA_MODEL"
note "$RUNNER_HOME/.config/phosphor-ci/owner-settings.toml mode 600, owner $RUNNER_USER (copy of the owner's settings.toml)"
note "Without them the app tests still run: bare profile, and the agent-provider cases skip."

step "Quick self-check (as $RUNNER_USER, the way a job sees the box)"
if $APPLY; then
  # shellcheck disable=SC2016  # expanded by the inner sh
  sudo -u "$RUNNER_USER" -H env -C /tmp "${RUNNER_ENV[@]:1}" sh -c '
    for t in cargo rustc cargo-nextest cargo-fuzz cargo-deny protoc cmake pkg-config dpkg-deb fakeroot mold \
             xvfb-run sway wtype grim wl-copy Xwayland xdotool xsel foot xterm dbus-run-session gnome-keyring-daemon \
             tesseract ssh-keygen python3; do
      printf "   %-22s %s\n" "$t" "$(command -v "$t" || echo MISSING)"; done
    test -x /usr/sbin/sshd && echo "   sshd                   /usr/sbin/sshd" || echo "   sshd                   MISSING"
    rustc --version; rustup target list --installed'
else
  note "(dry run) would list every tool a box job checks for, as $RUNNER_USER"
fi
note "Then prove the node with CI: script/ci/nodes/README.md 'Verification' (guards, check-linux, build-linux-x86_64,"
note "test-linux, app-test-linux-wayland on a build/* push)."
printf '\nDone (%s).\n' "$($APPLY && echo applied || echo 'dry run only')"
