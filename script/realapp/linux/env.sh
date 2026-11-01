# realapp/linux core: env.sh sets paths, launch.sh starts Xvfb + app on a private dbus-run-session, h.sh = key/screenshot/log helpers (source it), f.sh = fast-typing/Tab+Enter/paste/Ctrl-C trial functions.
# Platform: Linux X11. Prereqs: config.example.sh sourced, release .deb extracted to $PHOSPHOR_LINUX_RUN/dist, xdotool xsel xwininfo Xvfb dbus-run-session gnome-keyring.
# Run: source h.sh; then call a function from f.sh (fastrun, tabrun, pasterun, pe, ccrun, ...). Restores: nothing outside the run dir; stop the app with bin/stopapp.sh (exact PID).
export REALAPP="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export D=${D:-:75}
export R=${PHOSPHOR_LINUX_RUN}
export TH=/tmp/phosphor-realapp/xr-r18bi
export TH_HOME=${TH_HOME_OVERRIDE:-$R/home}
export BIN=${BIN_OVERRIDE:-$R/dist/x/squashfs-root/opt/phosphor/phosphor-oss}
