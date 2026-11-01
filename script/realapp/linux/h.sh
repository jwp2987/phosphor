# realapp/linux core: env.sh sets paths, launch.sh starts Xvfb + app on a private dbus-run-session, h.sh = key/screenshot/log helpers (source it), f.sh = fast-typing/Tab+Enter/paste/Ctrl-C trial functions.
# Platform: Linux X11. Prereqs: config.example.sh sourced, release .deb extracted to $PHOSPHOR_LINUX_RUN/dist, xdotool xsel xwininfo Xvfb dbus-run-session gnome-keyring.
# Run: source h.sh; then call a function from f.sh (fastrun, tabrun, pasterun, pe, ccrun, ...). Restores: nothing outside the run dir; stop the app with bin/stopapp.sh (exact PID).
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"
export DISPLAY=$D
export W=$(xdotool search --name "." 2>/dev/null | while read w; do xwininfo -id $w 2>/dev/null | grep -q "Map State: IsViewable" && xwininfo -id $w | grep -q "Width: 1[0-9][0-9][0-9]" && echo $w; done | head -1)
t(){ xdotool type --delay ${DL:-15} -- "$1"; }
k(){ xdotool key --delay ${DL:-15} "$@"; }
ent(){ xdotool key Return; }
s(){ $REALAPP/bin/shot.sh "$1"; }
alive(){ ps -p $(pgrep -f "squashfs-root/opt/phosphor/phosphor-oss$" | head -1) -o pid,etime,rss,pcpu,cmd | cut -c1-100; }
lg(){ tail -n ${1:-20} $R/raw/app.stdout; }
LOG=${LOGHOME:-$R/home}/.local/state/phosphor/phosphor.log
lgm(){ wc -l < $LOG; }
lgs(){ tail -n +$(($1+1)) $LOG | grep -E "keystroke:|routing|pty_write|submit buf|Preexec |ExecuteCommand|AI query|agent" | cut -c1-230 | head -${2:-12}; }
trial(){ # text  tag
  k ctrl+shift+t; sleep 1.8; printf %s "$1" | xsel -b -i; m=$(lgm)
  xdotool key ctrl+shift+v Return; sleep 1.5; s $2; echo "== $2 [$1]"; lgs $m 8; }
