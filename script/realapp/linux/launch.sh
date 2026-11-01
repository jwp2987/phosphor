#!/bin/bash
# realapp/linux core: env.sh sets paths, launch.sh starts Xvfb + app on a private dbus-run-session, h.sh = key/screenshot/log helpers (source it), f.sh = fast-typing/Tab+Enter/paste/Ctrl-C trial functions.
# Platform: Linux X11. Prereqs: config.example.sh sourced, release .deb extracted to $PHOSPHOR_LINUX_RUN/dist, xdotool xsel xwininfo Xvfb dbus-run-session gnome-keyring.
# Run: source h.sh; then call a function from f.sh (fastrun, tabrun, pasterun, pe, ccrun, ...). Restores: nothing outside the run dir; stop the app with bin/stopapp.sh (exact PID).
# launch.sh : start Xvfb + app on private bus. usage: launch.sh [extra env...]
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"
if ! [ -S /tmp/.X11-unix/X${D#:} ]; then
  Xvfb $D -screen 0 1600x1000x24 >$R/raw/xvfb.log 2>&1 &
  echo $! > $R/xvfb.pid; sleep 1
fi
cat > $TH/inner.sh <<EOS
#!/bin/bash
unset GNOME_KEYRING_CONTROL SSH_AUTH_SOCK WAYLAND_DISPLAY SSH_ASKPASS SSH_ASKPASS_REQUIRE
export DISPLAY=$D HOME=$TH_HOME XDG_RUNTIME_DIR=$TH XDG_CONFIG_HOME=$TH_HOME/.config XDG_STATE_HOME=$TH_HOME/.local/state XDG_DATA_HOME=$TH_HOME/.local/share
export RUST_LOG=warp::input_order=debug,warp::terminal::model_lock_trace=debug,warpui::frame_cause=debug,phosphor::routing=debug
export $*
printf "" | gnome-keyring-daemon --unlock --components=secrets >/dev/null 2>&1
exec $BIN > $R/raw/app.stdout 2>&1
EOS
chmod +x $TH/inner.sh
cd $TH_HOME
nohup dbus-run-session -- $TH/inner.sh >/dev/null 2>&1 &
echo $! > $R/app-launcher.pid
