#!/usr/bin/env bash
#
# fix-gitrunner-login.sh -- make the `gitrunner` CI account a real, loggable macOS
# user, and (optionally) move the GitLab runner into its desktop session so the
# macOS app tests can type into Phosphor and take screenshots.
#
# WHY: setup-mac-runner.sh (2026-10-26) created `gitrunner` as a background-only
# account: hidden, no login shell, and a password nobody knew. It could not log
# in and System Settings > Users & Groups did not list it.
# The runner also runs as a system LaunchDaemon, which has no desktop: macOS only
# lets a process inside a logged-in GUI session inject keys and record the screen.
#
# Isolation is unchanged: `gitrunner` stays a standard (non-admin) user, its home
# stays 700, and your home stays 700. Step 5 re-runs the isolation checks.
#
# Run as yourself (the admin user), NOT with sudo. Dry run by default.
#
#   ./fix-gitrunner-login.sh                       # dry run: print every step
#   ./fix-gitrunner-login.sh --apply               # fix the login (steps 1-3, 5)
#   ./fix-gitrunner-login.sh --apply --gui-runner  # also move the runner into
#                                                  # gitrunner's desktop (step 4)
#
# --gui-runner stops the background LaunchDaemon and installs a per-user
# LaunchAgent that only loads in gitrunner's own desktop session. From then on
# Mac CI jobs run only while gitrunner is logged in (fast user switching is fine;
# the screen must not be locked). Undo: ./fix-gitrunner-login.sh --apply --daemon-runner
#
# Every step prints what it changed and checks the result; the script stops at
# the first check that fails. Written for the stock macOS bash 3.2.

set -euo pipefail

RUNNER_USER="${RUNNER_USER:-gitrunner}"
RUNNER_HOME="/Users/${RUNNER_USER}"
RUNNER_BIN="${RUNNER_BIN:-/usr/local/bin/gitlab-runner}"
LABEL="com.gitlab.runner.${RUNNER_USER}"
DAEMON_PLIST="/Library/LaunchDaemons/${LABEL}.plist"
AGENT_DIR="${RUNNER_HOME}/Library/LaunchAgents"
AGENT_PLIST="${AGENT_DIR}/${LABEL}.plist"
AUTH_AUTHORITY=';ShadowHash;HASHLIST:<SALTED-SHA512-PBKDF2,SRP-RFC5054-4096-SHA512-PBKDF2>'
ADMIN_USER="$(id -un)"

APPLY=false
GUI_RUNNER=false
DAEMON_RUNNER=false
for arg in "$@"; do
  case "$arg" in
    --apply) APPLY=true ;;
    --gui-runner) GUI_RUNNER=true ;;
    --daemon-runner) DAEMON_RUNNER=true ;;
    -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
    *) echo "unknown option: $arg" >&2; exit 2 ;;
  esac
done
if $GUI_RUNNER && $DAEMON_RUNNER; then
  echo "--gui-runner and --daemon-runner are opposites; pick one" >&2; exit 2
fi

step() { printf '\n== %s\n' "$*"; }
note() { printf '   %s\n' "$*"; }
ok()   { printf '   ok:   %s\n' "$*"; }
die()  { printf '   FAIL: %s\n' "$*" >&2; printf '\nStopped. Nothing after this step was changed.\n' >&2; exit 1; }
# run CMD...: print it; execute only with --apply.
run() {
  printf '   + '; printf '%q ' "$@"; printf '\n'
  if $APPLY; then
    local rc=0
    "$@" || rc=$?
    [ "$rc" -eq 0 ] || die "the command above failed (exit $rc); its own error message is just above"
  fi
}
# Single-value attribute: "UserShell: /bin/zsh" or "dsAttrTypeNative:IsHidden: 1" -> last field.
attr() { dscl . -read "/Users/${RUNNER_USER}" "$1" 2>/dev/null | awk 'NR==1{print $NF}'; }
# dscl exits 0 and prints "No such key: X" for a missing attribute, so test the output.
has_attr() { dscl . -read "/Users/${RUNNER_USER}" "$1" 2>/dev/null | grep -Eq "^(dsAttrTypeNative:)?$1:"; }
as_runner() { ( cd / && sudo -u "$RUNNER_USER" -H "$@" ); }
runner_jobs() {
  # Child processes of the runner = a CI job in progress.
  local pid
  for pid in $(pgrep -u "$RUNNER_USER" -x gitlab-runner 2>/dev/null || true); do
    pgrep -P "$pid" 2>/dev/null || true
  done
}

# ---------------------------------------------------------------- 0. preflight
step "0. Preflight"
[ "$(uname -s)" = Darwin ] || die "this script is for macOS"
[ "$(id -u)" -ne 0 ] || die "run as your normal admin user, not root/sudo"
dscl . -read "/Users/${RUNNER_USER}" RecordName >/dev/null 2>&1 || die "no user '$RUNNER_USER'"
if dseditgroup -o checkmember -m "$RUNNER_USER" admin >/dev/null 2>&1; then
  die "$RUNNER_USER is an admin; refusing"
fi
note "admin user:   $ADMIN_USER"
note "runner user:  $RUNNER_USER  (uid $(id -u "$RUNNER_USER"), home $RUNNER_HOME)"
note "password set: $(dscl . -read "/Users/${RUNNER_USER}" accountPolicyData 2>/dev/null | grep -q passwordLastSetTime && echo yes || echo never)"
note "IsHidden:     $(has_attr IsHidden && attr IsHidden || echo '(not set)')"
note "shell:        $(attr UserShell)"
note "FileVault:    $(fdesetup status 2>/dev/null | head -1)"
note "mode:         $($APPLY && echo APPLY || echo 'DRY RUN (no changes)')"
if $APPLY; then
  sudo -v || die "sudo failed"
  jobs="$(runner_jobs)"
  [ -z "$jobs" ] || die "a CI job is running under $RUNNER_USER (pids: $jobs); wait for it to finish"
  ok "no CI job running"
fi

# --------------------------------------------- 1. visible account, login shell
step "1. Visible account with a login shell"
for a in IsHidden IsHiden; do
  if has_attr "$a"; then run sudo dscl . -delete "/Users/${RUNNER_USER}" "$a"; else note "$a: not set"; fi
done
if [ "$(attr UserShell)" != /bin/zsh ]; then
  run sudo dscl . -create "/Users/${RUNNER_USER}" UserShell /bin/zsh
else
  note "shell is already /bin/zsh"
fi
if $APPLY; then
  has_attr IsHidden && die "IsHidden is still set"
  has_attr IsHiden && die "IsHiden is still set"
  [ "$(attr UserShell)" = /bin/zsh ] || die "shell is $(attr UserShell), not /bin/zsh"
  ok "not hidden; shell /bin/zsh"
fi

# ------------------------------------------------------- 2. the password
# Do NOT judge this by reading AuthenticationAuthority: macOS shows that attribute only to the
# account itself and root, so from your account it reads "No such key" even when it is fine
# (root's reads the same way). The only real test is a login: dscl . -authonly.
step "2. Password"
note "Set a password for $RUNNER_USER (a new one, or the one you set earlier) and keep it in your"
note "password manager. You type it twice; the script then test-logs in with it."
if $APPLY; then
  pw1=""; pw2=""
  printf '   New password for %s (hidden): ' "$RUNNER_USER"; read -rs pw1; echo
  printf '   Again (hidden): ' ; read -rs pw2; echo
  [ -n "$pw1" ] || die "empty password"
  [ "$pw1" = "$pw2" ] || die "the two passwords differ"
  unset pw2
  # dscl reads the password from argv; the value is visible in `ps` for a moment.
  # This Mac has one human user (you).
  sudo dscl . -passwd "/Users/${RUNNER_USER}" "$pw1" || die "sudo dscl . -passwd failed (message above)"
  if dscl . -authonly "$RUNNER_USER" "$pw1" >/dev/null 2>&1; then
    ok "test login with the password works"
  else
    note "test login failed; adding the standard password record and setting it again"
    sudo dscl . -create "/Users/${RUNNER_USER}" AuthenticationAuthority "$AUTH_AUTHORITY" \
      || die "could not add the password record (message above)"
    sudo dscl . -passwd "/Users/${RUNNER_USER}" "$pw1" || die "sudo dscl . -passwd failed (message above)"
    if dscl . -authonly "$RUNNER_USER" "$pw1" >/dev/null 2>&1; then
      ok "test login with the password works"
    else
      unset pw1
      die "the password is set but a test login still fails (dscl . -authonly)"
    fi
  fi
  unset pw1
else
  note "(dry run) would prompt for the password, set it with: sudo dscl . -passwd /Users/$RUNNER_USER <hidden>,"
  note "          and check it with a test login: dscl . -authonly $RUNNER_USER <hidden>"
fi

# ---------------------------------------------------- 3. Users & Groups listing
step "3. Listed as a user"
if $APPLY; then
  dscl . -list /Users UniqueID | awk -v u="$RUNNER_USER" '$1==u{f=1} END{exit !f}' || die "not in the user list"
  ok "$RUNNER_USER is a visible local user with a password."
  note "System Settings > Users & Groups caches its list: quit and reopen System Settings."
fi

# ------------------------------------------------------ 4. where the runner runs
if $GUI_RUNNER; then
  step "4. Move the runner into $RUNNER_USER's desktop session"
  note "A per-user LaunchAgent: launchd starts it when $RUNNER_USER logs in to the desktop,"
  note "and only then. The background LaunchDaemon is stopped and renamed, so the two never"
  note "run at once (both would poll the same registration)."
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
    <string>--config</string><string>${RUNNER_HOME}/.gitlab-runner/config.toml</string>
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
  if [ -f "$DAEMON_PLIST" ]; then
    if ! $APPLY || sudo launchctl print "system/${LABEL}" >/dev/null 2>&1; then
      run sudo launchctl bootout "system/${LABEL}"
    fi
    run sudo mv "$DAEMON_PLIST" "${DAEMON_PLIST}.disabled"
  else
    note "no LaunchDaemon plist at $DAEMON_PLIST"
  fi
  if $APPLY; then
    as_runner mkdir -p "$AGENT_DIR" "${RUNNER_HOME}/Library/Logs"
    printf '%s\n' "$AGENT_BODY" | as_runner tee "$AGENT_PLIST" >/dev/null
    as_runner chmod 644 "$AGENT_PLIST"
    as_runner plutil -lint "$AGENT_PLIST" >/dev/null || die "LaunchAgent plist does not lint"
    sudo launchctl print "system/${LABEL}" >/dev/null 2>&1 && die "the LaunchDaemon is still loaded"
    [ ! -f "$DAEMON_PLIST" ] || die "$DAEMON_PLIST still exists"
    ok "LaunchAgent installed at $AGENT_PLIST; LaunchDaemon stopped and disabled"
  else
    printf '%s\n' "$AGENT_BODY" | sed 's/^/   | /'
  fi
  note "Next, at the Mac:"
  note "  1. Log in as $RUNNER_USER (menu bar user switcher, or log out and pick it)."
  note "     Finish the first-login setup screens; skip Apple ID."
  note "  2. The runner starts by itself. Check from your own account:"
  note "       ps -axo user,pid,command | grep '[g]itlab-runner run'"
  note "  3. As $RUNNER_USER: System Settings > Privacy & Security >"
  note "       Accessibility:    add $RUNNER_BIN   (the + button, Cmd-Shift-G to type the path)"
  note "       Screen Recording: add $RUNNER_BIN"
  note "     A job that types or screenshots may raise one more prompt the first time; allow it."
  note "  4. Switch back to your own account. Leave $RUNNER_USER logged in and the Mac awake."
  note "  After a reboot: you unlock FileVault, then log $RUNNER_USER in again."
elif $DAEMON_RUNNER; then
  step "4. Put the runner back as a background LaunchDaemon"
  if [ -f "${DAEMON_PLIST}.disabled" ]; then
    if [ -f "$AGENT_PLIST" ] || $APPLY; then
      as_runner launchctl bootout "gui/$(id -u "$RUNNER_USER")/${LABEL}" >/dev/null 2>&1 || true
      run sudo rm -f "$AGENT_PLIST"
    fi
    run sudo mv "${DAEMON_PLIST}.disabled" "$DAEMON_PLIST"
    run sudo launchctl bootstrap system "$DAEMON_PLIST"
    run sudo launchctl kickstart -k "system/${LABEL}"
    if $APPLY; then
      sudo launchctl print "system/${LABEL}" >/dev/null 2>&1 || die "the LaunchDaemon did not load"
      ok "background LaunchDaemon restored"
    fi
  else
    note "no ${DAEMON_PLIST}.disabled; the daemon was never moved"
  fi
else
  step "4. Runner location (unchanged)"
  note "the runner stays a background LaunchDaemon (builds and unit tests work; app tests need --gui-runner)"
fi

# --------------------------------------------------------------- 5. isolation
step "5. Isolation checks (as $RUNNER_USER)"
if $APPLY; then
  failed=0
  for p in "/Users/${ADMIN_USER}/.ssh" "/Users/${ADMIN_USER}/.zsh_history" \
           "/Users/${ADMIN_USER}/Documents" "/Users/${ADMIN_USER}/Library/Keychains" \
           "/Users/${ADMIN_USER}/Library/Keychains/login.keychain-db"; do
    if [ ! -e "$p" ]; then
      note "skip: $p does not exist"
    elif as_runner /bin/test -r "$p" 2>/dev/null; then
      printf '   FAIL: %s can read %s\n' "$RUNNER_USER" "$p"; failed=1
    else
      ok "$RUNNER_USER cannot read $p"
    fi
  done
  if as_runner /usr/bin/sudo -n true >/dev/null 2>&1; then
    printf '   FAIL: %s has sudo\n' "$RUNNER_USER"; failed=1
  else
    ok "$RUNNER_USER has no sudo"
  fi
  [ "$(stat -f '%Lp' "/Users/${ADMIN_USER}")" = 700 ] && ok "/Users/$ADMIN_USER is 700" \
    || { printf '   FAIL: /Users/%s is not 700\n' "$ADMIN_USER"; failed=1; }
  [ "$failed" -eq 0 ] || die "isolation check failed"
else
  note "(dry run) would check, as $RUNNER_USER, that your .ssh, .zsh_history, Documents and"
  note "keychains are unreadable, that it has no sudo, and that your home is 700."
fi

note ""
note "Remote Login: if System Settings > General > Sharing > Remote Login allows all users,"
note "set it to 'Only these users: $ADMIN_USER' so $RUNNER_USER cannot ssh in with its password."
printf '\nDone (%s).\n' "$($APPLY && echo applied || echo 'dry run only')"
