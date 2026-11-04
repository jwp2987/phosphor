#!/usr/bin/env bash
#
# macos.sh -- rebuild the macOS CI node (GitLab tag `Mac`, runner `MacBookPro-gitrunner`) on a fresh
# Apple-silicon Mac. Read script/ci/nodes/README.md first.
#
# DRY RUN BY DEFAULT: every step prints what it would do; --apply executes. Idempotent.
#
#   ./script/ci/nodes/macos.sh                                  # dry run
#   GITLAB_RUNNER_TOKEN=glrt-... ./script/ci/nodes/macos.sh --apply
#
# Run as the ADMIN user (the owner, `winters` on the observed Mac), NOT with sudo; steps that need root call
# sudo themselves. Run from a checkout of this repo. Written for the stock macOS bash 3.2.
#
# EVERY PIN BELOW WAS OBSERVED ON THE MAC TEST BOX ON 2026-11-04 (MacBook Pro M1 Pro, 8 cores, 16 GB,
# macOS 26.4.1 build 25E253, FileVault on). The runner user's home (/Users/gitrunner, mode 700) is NOT
# readable from the admin account and gitrunner has no sudo, so everything INSIDE it was not observed
# directly: those lines say TODO-VERIFY and take their value from the repo or a CI job log.
#
# Layout (why it looks like this): CI runs as `gitrunner`, a separate STANDARD user, so CI can never read the
# owner's files (docs/testing.md 3.3). macOS lets only a process inside a logged-in GUI (Aqua) session inject
# keys and record the screen, so the runner is a per-user LaunchAgent of gitrunner, not a LaunchDaemon. Mac CI
# jobs therefore run only while gitrunner is logged in at the console with the screen unlocked.
#
# The steps this script CANNOT do are printed at the end under "MANUAL" (TCC grants, login, lock screen).

set -euo pipefail

# ------------------------------------------------------------------ pins (observed 2026-11-04)
RUNNER_USER="${RUNNER_USER:-gitrunner}"
RUNNER_HOME="/Users/${RUNNER_USER}"
RUNNER_DESC="MacBookPro-gitrunner"            # GitLab runner description (API), tag `Mac`
RUNNER_BIN="/usr/local/bin/gitlab-runner"     # root:admin 755, Mach-O arm64; this path is what TCC grants name
GITLAB_RUNNER_VERSION="19.5.1"                # f411f264 (gitlab-runner --version)
GITLAB_URL="https://gitlab.com"
RUNNER_CONFIG="${RUNNER_HOME}/.gitlab-runner/config.toml"   # path from fix-gitrunner-login.sh's LaunchAgent; TODO-VERIFY
LABEL="com.gitlab.runner.${RUNNER_USER}"
AGENT_PLIST="${RUNNER_HOME}/Library/LaunchAgents/${LABEL}.plist"
XCODE_VERSION="26.6 (17F113)"                 # /Applications/Xcode.app, xcode-select -p -> Xcode.app/Contents/Developer
CLT_VERSION="26.4.1.0.1775747724"             # com.apple.pkg.CLTools_Executables
SWIFT_VERSION="6.3.3"                         # swiftc, builds the harness helper tools
METAL_VERSION="32023.883"                     # xcrun -sdk macosx metal --version (build-macos-* check it)
HOMEBREW_VERSION="7.0.10"                     # /opt/homebrew, owned by the admin user
PYTHON_FORMULA="python@3.11"                  # /opt/homebrew/bin/python3 -> Cellar/python@3.11/3.11.6 (the CI python3)
PILLOW_VERSION="13.0.0"                       # in python@3.11's global site-packages
RUST_TOOLCHAIN="1.99.1"                       # == rust-toolchain.toml (installed for gitrunner: TODO-VERIFY, builds pass)
CARGO_BINSTALL_VERSION="1.18.1"               # == script/install_cargo_binstall; present in gitrunner's ~/.cargo/bin (job log)
CARGO_ABOUT_VERSION="0.8.4"                   # == script/install_cargo_release_deps; present (job log)
CARGO_BUNDLE_VERSION="0.11.0"                 # == script/install_cargo_bundle; present (job log)
CREATE_DMG_VERSION="1.2.2"                    # == script/macos/install_create_dmg -> ~/.local/create-dmg-1.2.2 (job log)
# cargo-nextest / cargo-fuzz / cargo-mutants for gitrunner: versions TODO-VERIFY (installed on demand by
# script/ci/run_suite.sh via cargo binstall, unpinned). For parity with the other nodes pin them here:
CARGO_NEXTEST_VERSION="${CARGO_NEXTEST_VERSION:-0.9.143}"   # TODO-VERIFY on the Mac; box has 0.9.143, Windows 0.9.150
CARGO_FUZZ_VERSION="${CARGO_FUZZ_VERSION:-0.13.2}"          # TODO-VERIFY on the Mac; box value
CARGO_MUTANTS_VERSION="${CARGO_MUTANTS_VERSION:-27.1.0}"    # TODO-VERIFY on the Mac; box and Windows value

# brew leaves observed (2026-11-04) that CI uses, with the version brew had: Homebrew installs the CURRENT formula,
# so a rebuild gets newer builds of these. That is the one unpinnable layer; check the versions after install.
BREW_CI=(
  protobuf      # 36.2  -> protoc for every cargo job (NOTE: Linux and Windows pin protoc 25.1)
  bash          # 5.3.20 -> /opt/homebrew/bin/bash (the stock /bin/bash is 3.2)
  tesseract     # 5.3.3  (installed as a dependency on the observed Mac, not a leaf) -> OCR in UI cases
  fish          # 4.9.3  shell cases
  nushell       # 0.116.1 shell cases
  tmux          # 3.8
  htop          # 3.5.3
  "$PYTHON_FORMULA"
)
# Also leaves on the Mac, NOT needed by CI: cliclick 5.1 (the harness moved to htype2), virt-manager, youtube-dl.
# ffmpeg 6.0_1 is present as a dependency; the Mac harness screenshots with screencapture, not ffmpeg.

# -------------------------------------------------------------------------------- helpers
APPLY=false
for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=true ;;
    -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done
step() { printf '\n== %s\n' "$*"; }
note() { printf '   %s\n' "$*"; }
ok()   { printf '   ok:   %s\n' "$*"; }
die()  { printf '   FAIL: %s\n' "$*" >&2; exit 1; }
run() {
  printf '   + '; printf '%q ' "$@"; printf '\n'
  if $APPLY; then "$@"; fi
}
# as_runner CMD...: run as gitrunner from / (the admin's cwd is unreadable to it).
# shellcheck disable=SC2016  # "$@" is expanded by the inner sh
as_runner() { run sudo -u "$RUNNER_USER" -H /bin/sh -c 'cd / && exec "$@"' sh "$@"; }
BREW=/opt/homebrew/bin/brew
REPO="$(cd "$(dirname "$0")/../../.." && pwd)"

# ------------------------------------------------------------------------- 0. preflight
step "0. Preflight"
[ "$(uname -s)" = Darwin ] || die "macOS only"
[ "$(uname -m)" = arm64 ] || die "Apple silicon only (the Mac runner is darwin arm64; macOS Intel CI is dropped, #989)"
[ "$(id -u)" -ne 0 ] || die "run as your admin user, not root/sudo"
note "macOS $(sw_vers -productVersion) (observed node: 26.4.1)"
[ -f "$REPO/rust-toolchain.toml" ] || die "run from a checkout of the phosphor repo"
note "mode: $($APPLY && echo APPLY || echo 'DRY RUN (no changes)')"
if $APPLY; then sudo -v || die "sudo failed"; fi
# Never disturb a running job (the app tests drive this Mac's screen).
if pgrep -u "$RUNNER_USER" -f "gitlab-runner run" >/dev/null 2>&1 && pgrep -u "$RUNNER_USER" -f "appci|cargo|rustc" >/dev/null 2>&1; then
  die "a CI job is running under $RUNNER_USER; wait for it to finish"
fi

# ------------------------------------------------------------------------- 1. Xcode + Metal
step "1. Xcode $XCODE_VERSION, Command Line Tools, Metal toolchain"
if [ -d /Applications/Xcode.app ]; then
  ok "Xcode: $(/usr/bin/xcodebuild -version 2>/dev/null | tr '\n' ' ')"
else
  note "MANUAL: install Xcode $XCODE_VERSION (App Store or developer.apple.com), open it once, then re-run."
fi
if [ "$(xcode-select -p 2>/dev/null)" != /Applications/Xcode.app/Contents/Developer ] && [ -d /Applications/Xcode.app ]; then
  run sudo xcode-select -s /Applications/Xcode.app/Contents/Developer
fi
run sudo xcodebuild -license accept
note "CLT observed: $CLT_VERSION (xcode-select --install if missing); swiftc observed $SWIFT_VERSION"
if xcrun -sdk macosx metal --version >/dev/null 2>&1; then ok "Metal toolchain present (observed $METAL_VERSION)"
else run xcodebuild -downloadComponent MetalToolchain; fi

# ------------------------------------------------------------------------- 2. Homebrew + packages
step "2. Homebrew in /opt/homebrew (observed $HOMEBREW_VERSION, owned by $(id -un))"
if [ -x "$BREW" ]; then ok "$($BREW --version | head -1)"
else
  # shellcheck disable=SC2016  # expanded by the inner bash, only with --apply
  run /bin/bash -c '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"'
fi
for f in "${BREW_CI[@]}"; do
  if [ -x "$BREW" ] && HOMEBREW_NO_AUTO_UPDATE=1 "$BREW" list --versions "$f" >/dev/null 2>&1; then ok "$f $(HOMEBREW_NO_AUTO_UPDATE=1 "$BREW" list --versions "$f" | cut -d' ' -f2-)"
  else run env HOMEBREW_NO_AUTO_UPDATE=1 "$BREW" install "$f"; fi
done
# The CI python3 is /opt/homebrew/bin/python3 -> python@3.11 (every Mac job puts /opt/homebrew/bin first).
if [ "$(readlink /opt/homebrew/bin/python3 2>/dev/null)" != "../Cellar/python@3.11/3.11.6/bin/python3" ]; then
  note "observed /opt/homebrew/bin/python3 -> ../Cellar/python@3.11/3.11.6/bin/python3; link python@3.11 as python3:"
  run env HOMEBREW_NO_AUTO_UPDATE=1 "$BREW" link --overwrite "$PYTHON_FORMULA"
fi
PY=/opt/homebrew/bin/python3.11
if [ -x "$PY" ] && "$PY" -c "import PIL, sys; sys.exit(PIL.__version__ != '$PILLOW_VERSION')" 2>/dev/null; then ok "Pillow $PILLOW_VERSION"
else run "$PY" -m pip install --break-system-packages "pillow==$PILLOW_VERSION"; fi
note "paramiko: NOT installed on the observed Mac (so the fakesshd password-prompt half of ssh_password_prompt cannot"
note "  run there). Optional: $PY -m pip install --break-system-packages paramiko"

# /usr/local: a STALE 2018 Intel Homebrew (1211 entries in /usr/local/bin, x86_64, owned by the admin user) is still
# on the Mac. CI puts /usr/local/bin AFTER /opt/homebrew/bin, so a tool missing from /opt/homebrew silently resolves to
# an Intel binary there (observed: jq -> /usr/local/bin/jq x86_64; protoc also exists there). Its git was renamed to
# /usr/local/bin/git.old-2.15 (2.15.1) so the system git (Apple Git 2.50.1, /usr/bin/git) wins; git-upload-pack etc.
# 2.15 are still in /usr/local/bin. A REBUILT Mac must NOT recreate /usr/local Homebrew: only gitlab-runner goes there.
step "3. /usr/local (legacy note)"
note "Only $RUNNER_BIN belongs in /usr/local/bin on a rebuilt Mac. Do not install Intel Homebrew."

# ------------------------------------------------------------------------- 4. the runner user
step "4. Standard user $RUNNER_USER (not admin, home 700, shell /bin/zsh)"
if dscl . -read "/Users/$RUNNER_USER" RecordName >/dev/null 2>&1; then
  ok "$RUNNER_USER exists (uid $(id -u "$RUNNER_USER"); observed uid 501, gid 20 staff)"
  dseditgroup -o checkmember -m "$RUNNER_USER" admin >/dev/null 2>&1 && die "$RUNNER_USER is an admin; it must be a standard user"
else
  note "creates a visible, loggable standard user; you type its password (keep it in your password manager)"
  run sudo sysadminctl -addUser "$RUNNER_USER" -fullName "GitLab Runner" -shell /bin/zsh -password -
fi
run sudo chmod 700 "$RUNNER_HOME"
note "if the account exists but cannot log in / is hidden: script/ci/mac/fix-gitrunner-login.sh --apply"
note "after the runner is set up, fix-gitrunner-login.sh's step 5 re-checks isolation (owner's files unreadable, no sudo)"

# ------------------------------------------------------------------------- 5. gitlab-runner binary
step "5. gitlab-runner $GITLAB_RUNNER_VERSION at $RUNNER_BIN"
if [ -x "$RUNNER_BIN" ] && "$RUNNER_BIN" --version 2>/dev/null | grep -q "Version: *$GITLAB_RUNNER_VERSION\$"; then ok "$RUNNER_BIN $GITLAB_RUNNER_VERSION"
else
  run sudo mkdir -p /usr/local/bin
  run sudo curl -fsSL -o "$RUNNER_BIN" "https://gitlab-runner-downloads.s3.amazonaws.com/v${GITLAB_RUNNER_VERSION}/binaries/gitlab-runner-darwin-arm64"
  run sudo chown root:admin "$RUNNER_BIN"
  run sudo chmod 755 "$RUNNER_BIN"
  note "REPLACING this binary invalidates its TCC grants: re-grant Accessibility + Screen Recording (MANUAL list below)"
fi

# ------------------------------------------------------------------------- 6. Rust for gitrunner
step "6. Rust $RUST_TOOLCHAIN + cargo tools in $RUNNER_HOME/.cargo (gitrunner's own; CI PATH has \$HOME/.cargo/bin)"
as_runner /bin/sh -c "test -x \$HOME/.cargo/bin/rustup || curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain $RUST_TOOLCHAIN --no-modify-path"
as_runner /bin/sh -c "\$HOME/.cargo/bin/rustup toolchain install $RUST_TOOLCHAIN --profile minimal --component rustfmt,clippy,rust-analyzer"
# fuzz-smoke-macos passes --target <host>: the aarch64-apple-darwin std must be installed (it is the host target).
as_runner /bin/sh -c "\$HOME/.cargo/bin/rustup target add --toolchain $RUST_TOOLCHAIN aarch64-apple-darwin"
# The admin's checkout is in a 700 home gitrunner cannot read, so hand it a copy of the (self-contained) installer.
BINSTALL_SH="/tmp/phosphor-install_cargo_binstall.sh"
run install -m 755 "$REPO/script/install_cargo_binstall" "$BINSTALL_SH"
as_runner /bin/sh -c "PATH=\$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin $BINSTALL_SH"
for spec in "cargo-nextest@$CARGO_NEXTEST_VERSION" "cargo-fuzz@$CARGO_FUZZ_VERSION" "cargo-mutants@$CARGO_MUTANTS_VERSION" \
            "cargo-about@$CARGO_ABOUT_VERSION" "cargo-bundle@$CARGO_BUNDLE_VERSION"; do
  as_runner /bin/sh -c "PATH=\$HOME/.cargo/bin:/opt/homebrew/bin:/usr/bin:/bin cargo binstall --no-confirm --locked $spec"
done
note "create-dmg $CREATE_DMG_VERSION: installed by the first build-macos-arm64 (script/macos/install_create_dmg, SHA-pinned)"
note "never 'npm install -g create-dmg' (docs/testing.md 3.3)"

# ------------------------------------------------------------------------- 7. register + LaunchAgent
step "7. Register the runner and install the gitrunner LaunchAgent (Aqua session only)"
config_exists() { sudo -n -u "$RUNNER_USER" -H /bin/test -f "$RUNNER_CONFIG" 2>/dev/null; }
if config_exists; then
  ok "$RUNNER_CONFIG exists (not touched; it holds the runner token)"
elif [ -z "${GITLAB_RUNNER_TOKEN:-}" ]; then
  note "GITLAB_RUNNER_TOKEN not set: skipping registration (create the runner in GitLab first, README.md)"
else
  as_runner /bin/mkdir -p "$(dirname "$RUNNER_CONFIG")"
  as_runner "$RUNNER_BIN" register --non-interactive --config "$RUNNER_CONFIG" --url "$GITLAB_URL" \
      --token "$GITLAB_RUNNER_TOKEN" --executor shell --name "$RUNNER_DESC"
fi
# Same body as script/ci/mac/fix-gitrunner-login.sh --gui-runner (the one that is running on the observed Mac:
# /Library/LaunchDaemons/com.gitlab.runner.gitrunner.plist.disabled is the old daemon, kept disabled).
AGENT_BODY="<?xml version=\"1.0\" encoding=\"UTF-8\"?>
<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">
<plist version=\"1.0\">
<dict>
  <key>Label</key><string>${LABEL}</string>
  <key>LimitLoadToSessionType</key><string>Aqua</string>
  <key>ProgramArguments</key>
  <array>
    <string>${RUNNER_BIN}</string>
    <string>run</string>
    <string>--config</string><string>${RUNNER_CONFIG}</string>
    <string>--working-directory</string><string>${RUNNER_HOME}</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>HOME</key><string>${RUNNER_HOME}</string>
    <key>PATH</key><string>${RUNNER_HOME}/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ProcessType</key><string>Interactive</string>
  <key>StandardOutPath</key><string>${RUNNER_HOME}/Library/Logs/gitlab-runner.log</string>
  <key>StandardErrorPath</key><string>${RUNNER_HOME}/Library/Logs/gitlab-runner.log</string>
</dict>
</plist>"
as_runner /bin/mkdir -p "${RUNNER_HOME}/Library/LaunchAgents" "${RUNNER_HOME}/Library/Logs"
if $APPLY; then
  printf '%s\n' "$AGENT_BODY" | sudo -u "$RUNNER_USER" -H /usr/bin/tee "$AGENT_PLIST" >/dev/null
  sudo -u "$RUNNER_USER" -H /bin/chmod 644 "$AGENT_PLIST"
  sudo -u "$RUNNER_USER" -H /usr/bin/plutil -lint "$AGENT_PLIST" >/dev/null || die "LaunchAgent plist does not lint"
  ok "LaunchAgent written: $AGENT_PLIST (loads at $RUNNER_USER's next GUI login)"
else
  note "would write $AGENT_PLIST:"; printf '%s\n' "$AGENT_BODY" | sed 's/^/   | /'
fi
note "No LaunchDaemon: a daemon has no WindowServer session, so app-test-macos would fail with exit 3."

# ------------------------------------------------------------------------- 8. helper tools (optional pre-build)
step "8. Harness helper tools (built by the job; optional warm-up)"
note "script/realapp/mac/ci/app_test.sh compiles htype2, kl3, ocr, winrect, tis, ktap from script/realapp/mac/tools/*.swift"
note "with 'swiftc -O' at job start and caches them by source hash in $RUNNER_HOME/.cache/phosphor-appci-tools."
note "Nothing to install: swiftc (Xcode) is the prerequisite. The first app-test-macos run fills the cache."

# ------------------------------------------------------------------------- 9. power (scriptable part)
step "9. Power settings (observed: sleep 0, displaysleep 0, disksleep 10, standby 1, powernap 1, womp 1, on AC)"
run sudo pmset -a sleep 0 displaysleep 0
note "jobs also hold 'caffeinate -dimsu' for their whole run (app_test.sh)"

# ------------------------------------------------------------------------- 10. MANUAL
step "10. MANUAL steps (the script cannot do these)"
cat <<EOF
   a. Log in as $RUNNER_USER at the console once (fast user switching is fine). Finish the setup screens, skip Apple ID.
      The runner starts by itself; check from the admin account: ps -axo user,pid,command | grep '[g]itlab-runner run'
   b. TCC, in $RUNNER_USER's session: System Settings > Privacy & Security >
        Accessibility:    add $RUNNER_BIN   (+ button, Cmd-Shift-G to type the path)
        Screen Recording: add $RUNNER_BIN
      If a job raises a prompt for a child of the runner (/bin/bash, /opt/homebrew/bin/bash, /opt/homebrew/bin/python3.11,
      osascript, screencapture, or the compiled htype2/kl3/ocr/winrect/tis/ktap), allow it. app-test-macos probes this
      first and fails with exit 3 + NEEDS_OWNER.txt naming what is missing.
      For hand-run GUI work over ssh (docs/testing.md 3.3), the admin account's sshd also holds both grants
      (/usr/libexec/sshd-keygen-wrapper): TODO-VERIFY, the TCC database is not readable without Full Disk Access.
   c. Local Network: the first agent case with the CI Ollama raises "Allow Phosphor to find devices on local networks?"
      in $RUNNER_USER's session. Click Allow (Screen Sharing or at the console). A new ad-hoc signature may ask again.
   d. Lock screen: as $RUNNER_USER, System Settings > Lock Screen > "Require password after screen saver begins or display
      is turned off" = Never; screen saver = Never. A locked screen takes the harness's keys into the password field.
      Observed values for $RUNNER_USER: TODO-VERIFY (its preferences are in its 700 home).
   e. Auto-login: NOT possible while FileVault is on (observed: FileVault On, no autoLoginUser). After every reboot the owner
      unlocks FileVault, then logs $RUNNER_USER in, then switches back. Leave $RUNNER_USER logged in.
   f. Remote Login (System Settings > General > Sharing): "Only these users: <admin>" so $RUNNER_USER cannot ssh in.
   g. QuickType / autocorrect / press-and-hold: the harness does NOT depend on them (htype2 posts CGEvents with real key
      codes; nothing in script/realapp reads those settings). Leave them at the defaults.
   h. Restore the machine-local CI inputs from your backup (never in the repo or GitLab variables), as $RUNNER_USER:
        $RUNNER_HOME/.config/phosphor-ci/ci.env              (mode 600; variable names APPCI_OLLAMA_URL, APPCI_OLLAMA_MODEL)
        $RUNNER_HOME/.config/phosphor-ci/owner-settings.toml (mode 600; copy of the owner's settings.toml)
      (Present on the observed Mac: TODO-VERIFY, gitrunner's home is not readable from the admin account; docs say yes.)
   i. Verify with CI: script/ci/nodes/README.md "Verification" (diag-mac, check-macos, build-macos-arm64,
      test-warpui-core-macos, app-test-macos on a build/* push).
EOF
printf '\nDone (%s).\n' "$($APPLY && echo applied || echo 'dry run only')"
