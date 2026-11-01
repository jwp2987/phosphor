#!/bin/bash
# realapp/linux/torture: seeded expert-typist fuzzer and XTest key sequencer (kseq.c, rec.c) with exact-received-command verification, run against Phosphor and a reference terminal.
# Platform: Linux X11. Prereqs: cc -O2 kseq.c rec.c -lX11 -lXtst; PHOSPHOR_DEB_DIR extracted deb; config.example.sh sourced.
# Run: run.sh / all.sh (scenarios in scen.py); lib.sh builds the scratch profile (owner-config copy, agent mode, LAN provider).
# Restores: scratch profile only; the owner's config is only read (copied) and never printed.
# lib.sh: shared harness. Source it. Env: D (display, default :97), BIN (binary), T (work dir)
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
D=${D:-:97}; export DISPLAY=$D
T=${T:-$HERE/work}; mkdir -p $T
P=$T/prof; H=$P/home; R=/tmp/phosphor-realapp/xr-tsuggest
KSEQ=$HERE/kseq
LOG=$T/rec.log; export REC_LOG=$LOG

ensure_xvfb() { xdotool getdisplaygeometry >/dev/null 2>&1 || { Xvfb $D -screen 0 1600x1000x24 >$T/xvfb.log 2>&1 & echo $! >$T/xvfb.pid; sleep 1; }; }

# setup_profile [histlines]  -- fresh isolated HOME with rec on PATH, test dirs, seeded history
setup_profile() {
  local hl=${1:-0}
  rm -rf $P; mkdir -p $P/config/phosphor $P/state $P/data $H/bin $H/t
  cat > $P/config/phosphor/settings.toml <<EOT
[warpify.ssh]
ssh_extension_install_mode = "never_install"
[general]
default_session_mode = "terminal"
EOT
  if [ "${OWNER:-0}" = 1 ]; then
    # run it the way the OWNER does: scratch COPIES of config, state sqlite (read-only backup), bash rc/profile/history
    rm -rf $P/config/phosphor; cp -a $HOME/.config/phosphor $P/config/phosphor
    mkdir -p $P/state/phosphor
    python3 -I - "$P/state/phosphor/warp.sqlite" <<'PYE'
import sqlite3,sys
s=sqlite3.connect('file:$HOME/.local/state/phosphor/warp.sqlite?mode=ro',uri=True); d=sqlite3.connect(sys.argv[1]); s.backup(d); d.close(); s.close()
PYE
    cp $HOME/.bashrc $H/.bashrc; cp $HOME/.profile $H/.profile; cp $HOME/.bash_history $H/.bash_history
    rm -f $H/.bash_profile
    # remove the harness-written bashrc/profile lines below (they are appended after the owner's)
    OWNER_RC=1
  fi
  if [ "${AI:-0}" = 1 ] && [ "${OWNER:-0}" != 1 ]; then python3 - "$P/config/phosphor/settings.toml" <<'PYE'
# AI profile: LAN Ollama provider copied from the owner's settings (never "Local"), agent mode + NLD autodetect ON.
import re,sys,os
src=open(os.path.expanduser('~/.config/phosphor/settings.toml')).read().split('\n')
out=[];i=0
sect=None
keep=('agents',)
blk=[]
for l in src:
    m=re.match(r'^\[(.+)\]',l)
    if m: sect=m.group(1)
    if sect and sect.split('.')[0]=='agents' and sect not in ('agents.statusline','agents.third_party','agents.third_party.per_agent'): blk.append(l)
txt='\n'.join(blk)
# drop the "Local" provider entry (127.0.0.1:8787)
L=txt.split('\n')
k=[j for j,l in enumerate(L) if '52907771' in l][0]
a=k-1
while L[a].strip()!='{': a-=1
b=k
while L[b]!='  },': b+=1
txt='\n'.join(L[:a]+L[b+1:])
assert '8787' not in txt, 'Local provider not removed'
open(sys.argv[1],'w').write('[warpify.ssh]\nssh_extension_install_mode = "never_install"\n[general]\ndefault_session_mode = "agent"\n'+txt+'\n')
PYE
  fi
  cp $HERE/rec $H/bin/rec
  [ -z "$OWNER_RC" ] && echo '[ -f ~/.bashrc ] && . ~/.bashrc' > $H/.bash_profile
  [ -z "$OWNER_RC" ] && cat > $H/.bashrc <<'EOT'
export PS1="\w\$ "
export PATH=$HOME/bin:$PATH
export HISTSIZE=200000 HISTFILESIZE=200000
export REC_LOG
EOT
  echo "export REC_LOG=$LOG" >> $H/.bashrc
  echo 'export PATH=$HOME/bin:$PATH' >> $H/.bashrc
  cat >> $H/.bashrc <<'EOT'
__rl() { { echo "H $EPOCHREALTIME"; HISTTIMEFORMAT= history 1; } >> "$REC_LOG"; }
PROMPT_COMMAND="__rl${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
EOT
  # test tree
  mkdir -p "$H/t/with space" "$H/t/quo'te" 'dir$dollar' 2>/dev/null
  mkdir -p "$H/t/with space/inner dir" "$H/t/uni-é日本" "$H/t/\$HOME-lit" "$H/t/alpha" "$H/t/alphabet" "$H/t/alpine"
  touch "$H/t/with space/file one.txt" "$H/t/quo'te/a" "$H/t/uni-é日本/ñ.txt" "$H/t/small.txt" "$H/t/smaller.txt"
  mkdir -p $H/big; ( cd $H/big && seq -f 'f%05g.dat' 1 5000 | xargs touch )
  # history: prefix collisions
  {
    for i in $(seq 1 30); do echo "rec alpha $i"; done
    echo "rec alpha beta"; echo "rec alphabet soup"; echo "rec alpha"; echo "rec alpha-one two"
    echo "rec git status"; echo "rec git stash"; echo "rec echo hello world"; echo "rec héllo ünï"
    echo 'rec "quoted arg" and $dollar'; echo "rec long $(printf 'x%.0s' $(seq 1 120))"
    echo "cd t"; echo "ls -la"; echo "git checkout -"
  } >> $H/.bash_history
  if [ "$hl" -gt 0 ]; then
    python3 - $hl >> $H/.bash_history <<'PY'
import sys,random
random.seed(1)
for i in range(int(sys.argv[1])): print("rec filler %d %x"%(i,random.getrandbits(40)))
PY
  fi
  : > $LOG
}

launch_app() {
  MARK=tsm$$_$RANDOM; echo $MARK > $P/mark
  rm -rf $R; mkdir -p $R; chmod 700 $R
  env -u WAYLAND_DISPLAY -u GNOME_KEYRING_CONTROL -u SSH_AUTH_SOCK DISPLAY=$D XDG_RUNTIME_DIR=$R XDG_CONFIG_HOME=$P/config XDG_DATA_HOME=$P/data XDG_STATE_HOME=$P/state SHELL=/bin/bash HOME=$H REC_LOG=$LOG TSMARK=$MARK \
    setsid -f dbus-run-session -- sh -c 'printf "" | gnome-keyring-daemon --unlock --components=secrets >/dev/null; exec "$0"' "$BIN" > $P/stdout.log 2>&1
  echo $MARK > $P/pid
  for i in $(seq 1 200); do
    M=$(pids_marked | while read q; do [ "$(readlink /proc/$q/exe)" = "$(readlink -f $BIN)" ] && [ "$(tr '\0' ' ' </proc/$q/cmdline | wc -w)" = 1 ] && echo $q; done | head -1)
    W=""; [ -n "$M" ] && W=$(xdotool search --pid $M 2>/dev/null | head -1)
    [ -n "$W" ] && break; sleep 0.25; done
  echo "$M" > $P/mainpid; echo "$W" > $P/win
  sleep 6
  xdotool windowfocus --sync $W; xdotool key Escape; sleep 0.8
  if [ "${OWNER:-0}" = 1 ]; then
    xdotool windowsize $W 1590 990; xdotool windowmove $W 0 0; sleep 2
    xdotool mousemove 140 419 click 1; sleep 2; xdotool key Escape; sleep 0.5
    xdotool mousemove 800 915 click 1; sleep 0.5; xdotool mousemove 1100 300
  else
    xdotool mousemove 700 724 click 1; sleep 0.5; xdotool mousemove 1100 300
  fi
}
shot() { import -window root "$1" 2>/dev/null || xwd -root -silent | convert xwd:- "$1" 2>/dev/null || ffmpeg -loglevel error -y -f x11grab -video_size 1600x1000 -i $D -frames:v 1 "$1"; }
ocr() { shot $T/_o.png; tesseract $T/_o.png - 2>/dev/null; }
pids_marked() { local m; m=$(cat $P/mark 2>/dev/null); [ -n "$m" ] && grep -la "TSMARK=$m" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3; }
stop_app() {
  local q; for q in $(pids_marked); do kill $q 2>/dev/null; done; sleep 1.5
  for q in $(pids_marked); do kill -9 $q 2>/dev/null; done
  fusermount3 -u -z $R/doc 2>/dev/null; true
}
# clear the line + screen between reps (Ctrl-C, then Enter-less clear)
reset_line() { $KSEQ d20 '<C-c>' w150 '<C-u>' w150; }
nowns() { date +%s%N; }

# ---- reference terminals ----
launch_xterm() {  # same HOME/rc/history as the app; xterm + bash readline
  XT_MARK=tsx$$_$RANDOM; echo $XT_MARK > $P/mark
  env DISPLAY=$D HOME=$H REC_LOG=$LOG TSMARK=$XT_MARK HISTFILE=$H/.bash_history setsid -f xterm -geometry 140x40+0+0 -fa Monospace -fs 11 -e bash --rcfile $H/.bashrc -i
  sleep 2
  W=$(xdotool search --class xterm | head -1); echo $W > $P/win
  xdotool windowfocus $W; xdotool mousemove 400 300 click 1; sleep 0.5
  M=$(pids_marked | while read q; do [ "$(basename $(readlink /proc/$q/exe))" = xterm ] && echo $q; done | head -1); echo $M > $P/mainpid
}
launch_konsole() {
  XT_MARK=tsk$$_$RANDOM; echo $XT_MARK > $P/mark
  env -u WAYLAND_DISPLAY QT_QPA_PLATFORM=xcb DISPLAY=$D HOME=$H XDG_CONFIG_HOME=$P/config XDG_DATA_HOME=$P/data XDG_RUNTIME_DIR=$R REC_LOG=$LOG TSMARK=$XT_MARK setsid -f dbus-run-session -- konsole --separate -e bash --rcfile $H/.bashrc -i
  sleep 6
  W=$(xdotool search --class konsole | head -1); echo $W > $P/win
  xdotool windowfocus $W; xdotool mousemove 400 300 click 1; sleep 0.5
  M=$(pids_marked | while read q; do [ "$(basename $(readlink /proc/$q/exe))" = konsole ] && echo $q; done | head -1); echo $M > $P/mainpid
}
