# realapp/linux core: env.sh sets paths, launch.sh starts Xvfb + app on a private dbus-run-session, h.sh = key/screenshot/log helpers (source it), f.sh = fast-typing/Tab+Enter/paste/Ctrl-C trial functions.
# Platform: Linux X11. Prereqs: config.example.sh sourced, release .deb extracted to $PHOSPHOR_LINUX_RUN/dist, xdotool xsel xwininfo Xvfb dbus-run-session gnome-keyring.
# Run: source h.sh; then call a function from f.sh (fastrun, tabrun, pasterun, pe, ccrun, ...). Restores: nothing outside the run dir; stop the app with bin/stopapp.sh (exact PID).
. "$(dirname "${BASH_SOURCE[0]}")/h.sh"
export DISPLAY=$D
rnd(){ head -c 12 /dev/urandom | base64 | tr -dc 'a-zA-Z0-9' | head -c ${1:-12}; }
# fastrun DELAY N GAP TAG : types N commands, Enter immediately, compares r file
fastrun(){ local dl=$1 n=$2 gap=$3 tag=$4 f=$R/out/r_$4.txt; : > $f; : > $f.exp
 local m=$(lgm)
 for i in $(seq 1 $n); do tok="T${tag}_${i}_$(rnd 10)"; echo $tok >> $f.exp
  xdotool type --delay $dl -- "echo $tok >> r_$tag.txt"; xdotool key --delay 0 Return; [ "$gap" != 0 ] && sleep $gap; done
 sleep 3
 local ok=$(grep -cxFf $f.exp $f); local ex=$(wc -l < $f.exp)
 if diff -q $f.exp $f >/dev/null; then echo "PASS $tag dl=$dl n=$n gap=$gap exact $ok/$ex"; else echo "FAIL $tag dl=$dl n=$n gap=$gap got $(wc -l < $f) lines, exact-match $ok/$ex"; diff $f.exp $f | head -6; fi
}
tabrun(){ # delay_between_tab_and_enter n tag
 local g=$1 n=$2 tag=$3; mkdir -p $R/out/tc; rm -f $R/out/tc/zzcompl_*; : > $R/out/tc/zzcompl_alpha_unique_$tag.txt; : > $R/out/tc/exp_$tag
 for i in $(seq 1 $n); do tok="TB${tag}_${i}_$(rnd 8)"; echo $tok >> $R/out/tc/exp_$tag
  xdotool type --delay 15 -- "echo $tok >> tc/zzcompl_alpha_u"; xdotool key Tab; [ "$g" != 0 ] && sleep $g; xdotool key Return; sleep 0.5; done
 sleep 2
 if diff -q $R/out/tc/exp_$tag $R/out/tc/zzcompl_alpha_unique_$tag.txt >/dev/null; then echo "PASS tab $tag gap=$g n=$n"; else echo "FAIL tab $tag gap=$g: file has $(wc -l < $R/out/tc/zzcompl_alpha_unique_$tag.txt)/$n; stray: $(ls $R/out/tc | grep -v $tag | tr '\n' ' ')"; fi; }
pasterun(){ # tag n size(chars of paste)  (md5 compare)
 local tag=$1 n=$2 sz=$3 f=$R/out/rp_$1.txt; : > $f; : > $f.exp
 for i in $(seq 1 $n); do tok="P${tag}$i$(rnd 6)"; pad=$(head -c $sz /dev/zero | tr '\0' 'q'); pl="$tok$pad"
  printf %s "$pl" | xsel -b -i
  printf '%s\n' "${pl}xzy" | md5sum >> $f.exp
  xdotool key ctrl+c; sleep 0.6; xdotool type --delay 12 -- "echo "; xdotool key ctrl+shift+v; xdotool key --delay 8 x y Left z End; xdotool type --delay 8 -- " | md5sum >> rp_$tag.txt"; xdotool key Return; sleep $((1+sz/150000)); done
 sleep 2; if diff -q $f.exp $f >/dev/null; then echo "PASS paste+arrow $tag n=$n size=$sz"; else echo "FAIL paste+arrow $tag size=$sz: $(wc -l < $f)/$n lines"; diff $f.exp $f | head; fi; }
pe(){ # paste + instant Enter. tag n size
 local tag=$1 n=$2 sz=$3 f=$R/out/pe_$1.txt; : > $f; : > $f.exp
 for i in $(seq 1 $n); do xdotool key ctrl+c; sleep 0.5; pad=$(head -c $sz /dev/zero | tr '\0' 'w'); tok="E${tag}$i$(rnd 6)"
  printf %s "echo $tok$pad | md5sum >> pe_$tag.txt" | xsel -b -i; printf '%s\n' "$tok$pad" | md5sum >> $f.exp
  xdotool key ctrl+shift+v; xdotool key Return; sleep $((2+sz/100000)); done
 sleep 2; if diff -q $f.exp $f >/dev/null; then echo "PASS paste+Enter $tag n=$n size=$sz"; else echo "FAIL paste+Enter $tag size=$sz: $(wc -l < $f)/$n"; fi; }
ccrun(){ # tag n : ctrl-c delivery latency
 local tag=$1 n=$2 f=$R/out/cc_$1.txt; : > $f; : > $f.sent
 for i in $(seq 1 $n); do
  xdotool type --delay 10 -- "bash -c 'trap \"date +%s.%N >> cc_$tag.txt; exit\" INT; sleep 30' ; echo AFTER$i >> cc_$tag.txt"; xdotool key Return; sleep 1.2
  date +%s.%N >> $f.sent; xdotool key ctrl+c; sleep 1.2; done
 python3 - $f $f.sent <<'PY'
import sys
r=[float(l) for l in open(sys.argv[1]) if l[0].isdigit()]; s=[float(l) for l in open(sys.argv[2])]
a=open(sys.argv[1]).read()
d=sorted((x-y)*1000 for x,y in zip(r,s))
print('ctrlc n=%d got=%d  ms median %.1f max %.1f  AFTER=%d'%(len(s),len(r),d[len(d)//2] if d else -1,d[-1] if d else -1,a.count('AFTER')))
PY
}
cdrun(){ tag=$1; rm -f $R/out/cd_$tag.txt
 xdotool type --delay 12 -- "cat > cd_$tag.txt"; ent; sleep 1; xdotool type --delay 12 -- "hello"; ent; sleep .5; xdotool key ctrl+d; sleep 1.5
 xdotool type --delay 12 -- "echo done1 >> cd_$tag.txt"; ent; sleep 1
 xdotool type --delay 12 -- "bash"; ent; sleep 2; xdotool type --delay 12 -- 'echo L$SHLVL >> cd_'$tag'.txt'; ent; sleep 1; xdotool key ctrl+d; sleep 1.5
 xdotool type --delay 12 -- 'echo L$SHLVL >> cd_'$tag'.txt'; ent; sleep 1
 xdotool type --delay 12 -- "ab"; xdotool key ctrl+c; sleep .5; xdotool type --delay 12 -- 'echo c3 >> cd_'$tag'.txt'; ent; sleep 1
 echo "--- cd_$tag"; cat $R/out/cd_$tag.txt | tr '\n' '|'; echo; }
subrun(){ # shellcmd gapsec tag
 local sh=$1 g=$2 tag=$3 f=$R/out/sub_$3.txt; rm -f $f; tok="S${tag}$(rnd 8)"
 xdotool type --delay 12 -- "$sh"; xdotool key Return; sleep $g
 xdotool type --delay 15 -- "echo $tok >> sub_$tag.txt; sh -c 'ps -o comm= -p \$PPID' >> sub_$tag.txt"; xdotool key Return; sleep 4
 xdotool type --delay 15 -- "exit"; xdotool key Return; sleep 2
 xdotool type --delay 15 -- "echo back_$tok >> sub_$tag.txt"; xdotool key Return; sleep 1.5
 r=$(tr '\n' ' ' < $f 2>/dev/null); echo "sub '$sh' gap=$g $tag -> $r"
 case "$r" in "$tok "*"back_$tok ") echo "  PASS";; *) echo "  FAIL (expected $tok <shell> back_$tok)";; esac; }
nest(){ # tag gap
 local tag=$1 g=$2 f=$R/out/nest_$1.txt; rm -f $f; tok="N${tag}$(rnd 6)"
 for sh in bash zsh fish; do xdotool type --delay 12 -- "$sh"; xdotool key Return; sleep $g; done
 xdotool type --delay 15 -- "echo $tok >> nest_$tag.txt; sh -c 'ps -o comm= -p \$PPID' >> nest_$tag.txt"; xdotool key Return; sleep 3
 for i in 1 2 3; do xdotool type --delay 15 -- "exit"; xdotool key Return; sleep $g; sleep 1; done
 xdotool type --delay 15 -- "echo back_$tok >> nest_$tag.txt; sh -c 'ps -o comm= -p \$PPID' >> nest_$tag.txt"; xdotool key Return; sleep 2
 r=$(tr '\n' ' ' < $f); echo "nest $tag gap=$g -> $r"; }
readrun(){ tag=$1 g=$2; f=$R/out/rd_$tag.txt; rm -f $f; tok="R${tag}$(rnd 8)"
 xdotool type --delay 10 -- "read x"; xdotool key Return; sleep $g
 xdotool type --delay 12 -- "$tok with  spaces ; \$HOME 'q'"; xdotool key Return; sleep 1
 xdotool type --delay 12 -- 'printf "%s\n" "$x" >> rd_'$tag'.txt'; xdotool key Return; sleep 1.5
 echo "read $tag gap=$g: $(cat $f 2>&1)  expect: $tok with  spaces ; \$HOME 'q'"; }
sshfail(){ host=$1; g=$2; tag=$3; rm -f $R/out/sf_$tag.txt; xdotool key ctrl+u; sleep .3; xdotool type --delay 12 -- "ssh -o ConnectTimeout=4 -o BatchMode=yes $host"; xdotool key Return; sleep $g; xdotool type --delay 15 -- "echo MUSTNOT_$tag >> sf_$tag.txt"; xdotool key Return; sleep ${4:-3.4}; s sf_$tag; sleep 4; echo "sshfail $host gap=$g $tag: file=$( [ -e $R/out/sf_$tag.txt ] && echo EXISTS:$(cat $R/out/sf_$tag.txt) || echo absent )"; }
sshtype(){ # gap tag n
 local g=$1 tag=$2 n=$3 f=$R/remote_home/rl_$2.txt; rm -f $f $f.exp; : > $f.exp
 for i in $(seq 1 $n); do tok="RT${tag}_${i}_$(rnd 8)"; echo "$tok" >> $f.exp
  xdotool type --delay 12 -- "ssh -F c fake"; xdotool key Return; sleep $g
  xdotool type --delay 15 -- "echo $tok >> ~/rl_$tag.txt"; xdotool key Return; sleep 5
  xdotool type --delay 12 -- "exit"; xdotool key Return; sleep 2; done
 local ok=$(grep -cxFf $f.exp $f 2>/dev/null || true); ok=${ok:-0}; echo "sshtype gap=$g $tag: $ok/$n exact lines in remote file; local stray: $(ls $R/out $R/home 2>/dev/null | grep -c rl_$tag)"
}
sshtype2(){ # gap tag n keydelay
 local g=$1 tag=$2 n=$3 kd=$4 f=$R/remote_home/rl_$2.txt; rm -f $f $f.exp; : > $f.exp
 for i in $(seq 1 $n); do tok="RT${tag}_${i}_$(rnd 8)"; echo "$tok" >> $f.exp
  xdotool type --delay 12 -- "ssh -F c fake"; xdotool key Return; sleep $g
  xdotool type --delay $kd -- "echo $tok >> ~/rl_$tag.txt"; xdotool key Return; sleep 5
  xdotool type --delay 12 -- "exit"; xdotool key Return; sleep 2; done
 local ok=$(grep -cxFf $f.exp $f 2>/dev/null || true); ok=${ok:-0}; echo "sshtype2 gap=$g keydelay=$kd $tag: $ok/$n exact"
}
dlgtrial(){ # gap tag
 g=$1; tag=$2; tok="DLG${tag}_$(rnd 6)"; xdotool key ctrl+u; xdotool type --delay 12 -- "ssh -F c fake"; xdotool key Return; sleep $g; xdotool type --delay 15 -- "echo $tok >> ~/dl_$tag.txt"; xdotool key Return; sleep 3; s dlg_$tag; }
ccflood(){ tag=$1; n=$2; f=$R/out/ccf_$tag.txt; : > $f; : > $f.sent
 for i in $(seq 1 $n); do
  xdotool type --delay 10 -- "bash -c 'trap \"date +%s.%N >> ccf_$tag.txt; exit\" INT; yes flood-line-0123456789abcdefghijklmnopqrstuvwxyz'"; xdotool key Return; sleep 2.0
  date +%s.%N >> $f.sent; xdotool key ctrl+c; sleep 1.5; done
 python3 - $f $f.sent <<'PY'
import sys
r=[float(l) for l in open(sys.argv[1]) if l[0].isdigit()]; s=[float(l) for l in open(sys.argv[2])]
d=sorted((x-y)*1000 for x,y in zip(r,s))
print('ctrl-c under yes flood: n=%d got=%d ms median %.1f max %.1f (includes ~8ms xdotool spawn)'%(len(s),len(r),d[len(d)//2] if d else -1,d[-1] if d else -1))
PY
}
bigcat(){ tag=$1; sz=$2; python3 - "$sz" > /tmp/phosphor-realapp/big_$tag.txt <<'PY'
import sys,random
n=int(sys.argv[1]); r=random.Random(5)
out=[]; tot=0
while tot<n:
    l=''.join(r.choice('abcdefghijklmnopqrstuvwxyz0123456789 ') for _ in range(r.randint(20,120))); out.append(l); tot+=len(l)+1
print('\n'.join(out)[:n])
PY
xsel -b -i < /tmp/phosphor-realapp/big_$tag.txt; xdotool type --delay 12 -- "cat > bigcat_$tag.out"; xdotool key Return; sleep 1; xdotool key ctrl+shift+v; sleep $((3+sz/1000000*3)); xdotool key Return; sleep 0.5; xdotool key ctrl+d; sleep 3; (cat /tmp/phosphor-realapp/big_$tag.txt; echo) | cmp - $R/out/bigcat_$tag.out && echo "bigcat $tag $sz: byte-exact"; }
subfast(){ # shell gap tag n
 local sh=$1 g=$2 tag=$3 n=$4 f=$R/out/sf2_$3.txt; rm -f $f $f.exp; : > $f.exp
 for i in $(seq 1 $n); do tok="SF${tag}${i}_$(rnd 8)"; echo "$tok" >> $f.exp
  xdotool type --delay 10 -- "$sh"; xdotool key Return; sleep $g
  xdotool type --delay 10 -- "echo $tok >> sf2_$tag.txt"; xdotool key Return; sleep 3
  xdotool type --delay 10 -- "exit"; xdotool key Return; sleep 1.5; done
 local ok=$(grep -cxFf $f.exp $f 2>/dev/null || true); echo "subfast $sh gap=$g $tag: ${ok:-0}/$n exact"
}
