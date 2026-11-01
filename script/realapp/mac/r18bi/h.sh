# realapp/mac: scripts run ON the Mac test box (via ssh) driving a scratch Phosphor.app with cliclick/osascript/htype2 and ScreenCaptureKit keylat.
# Platform: macOS arm64. Prereqs: config vars exported on the Mac; cliclick, caffeinate; the htype2 typing tool (see ../../keylat and README); Screen Recording + Accessibility granted to sshd.
# Safety: guard() checks frontmost app/PID before every key burst; never Ctrl-D, Cmd-Q/W/Tab/Space, Globe, Ctrl-Cmd chords; back up and restore dev.phosphor.* plists with sha256; never run CI on the Mac during a measurement.
# Restores: kill only the exact app PID the harness recorded; kill the caffeinate PID it started.
# r18bi harness. source it.
export PATH=/opt/homebrew/bin:$HOME/.local/bin:$PATH
R=$HOME/phosphor-profile/r18bi/lat
H=$R/home
APP=$HOME/phosphor-profile/apps/${VAPP:-r18bi}/Phosphor.app
BOX=${PHOSPHOR_BOX_SSH}
LOG=$H/Library/Logs/phosphor.log
apppid(){ cat $R/app.pid 2>/dev/null; }
front(){ osascript -e 'tell application "System Events" to name of first process whose frontmost is true' 2>/dev/null; }
frontpid(){ osascript -e 'tell application "System Events" to unix id of first process whose frontmost is true' 2>/dev/null; }
guard(){
  kill -0 $(apppid) 2>/dev/null || { echo "GUARD: app dead" >&2; return 1; }
  [ "$(osascript -e 'tell application "System Events" to count of windows of process "phosphor-oss"' 2>/dev/null)" -ge 1 ] 2>/dev/null || { echo "GUARD: no window" >&2; return 1; }
  local f p; f=$(front); p=$(frontpid)
  if [ "$f" = phosphor-oss ] && [ "$p" = "$(apppid)" ]; then return 0; fi
  [ "$f" = loginwindow ] && { echo "GUARD: LOCK SCREEN? abort" >&2; return 1; }
  osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $(apppid)) to true" >/dev/null 2>&1
  sleep 0.6; f=$(front); p=$(frontpid)
  if [ "$f" = phosphor-oss ] && [ "$p" = "$(apppid)" ]; then return 0; fi
  echo "GUARD: not frontmost ($f $p) - NOT sending keys" >&2; return 1
}
shot(){ screencapture -x $R/shots/$1.png; }
ret(){ osascript -e 'tell application "System Events" to key code 36'; }
typ(){ guard || return 1; cliclick -w ${W:-20} t:"$1"; }
line(){ typ "$1" || return 1; sleep 0.3; guard && ret; }
ctrl(){ guard || return 1; cliclick kd:ctrl t:$1 ku:ctrl; }
cmd(){ guard || return 1; cliclick kd:cmd t:$1 ku:cmd; }
launch(){ : > $R/ctl/log; open -n -g --env HOME=$H --env ZDOTDIR=$R/zdot --env SHELL=${LSHELL:-/bin/zsh} --env RUST_LOG=${RL:-warp::input_order=debug,phosphor::routing=debug} $APP; sleep 6; pgrep -n -f "$APP/Contents/MacOS/phosphor-oss$" > $R/app.pid; echo "app pid $(apppid)"; osascript -e "tell application \"System Events\" to set frontmost of (first process whose unix id is $(apppid)) to true"; sleep 1; front; }
killapp(){ local p=$(apppid); [ -n "$p" ] && kill $p 2>/dev/null; sleep 2; kill -0 $p 2>/dev/null && echo "STILL ALIVE $p" || echo "app exited"; }
