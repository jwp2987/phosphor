#!/bin/bash
# appci/bytes_reader.sh OUTFILE SECONDS: the bash twin of bytes_reader.py for the Windows Git Bash pane (#2072, T9 meta_and_word_keys): the test
# VM's Python is a native Windows build with no termios, so the raw tty is set with msys `stty` and read by bash itself. Same output format: one
# "epoch hex" line per byte that reaches the pty (bytes_reader.py logs one line per chunk; readers join the hex), appended at once. Creates
# OUTFILE.ready when listening. Ends on Ctrl-C or after SECONDS. Needs bash >= 4 (fractional `read -t`, `printf -v`); no forks per byte.
# Limits (measured on bash 5.3, a pty): bash's own `read -n` puts the tty in its cbreak mode for each read, which turns ISIG and ICRNL back on.
# So Ctrl-C ends the reader by SIGINT (it is not logged), CR is logged as 0a, and Ctrl-Z / Ctrl-\ are signals: do not use it for those bytes.
out=$1
secs=${2:-30}
old=$(stty -g)
trap 'stty "$old"' EXIT
stty raw -echo
: > "$out"
: > "$out.ready"
end=$((SECONDS + secs))
while [ "$SECONDS" -lt "$end" ]; do
  if LC_ALL=C IFS= read -r -s -n 1 -d '' -t 0.2 c; then
    if [ -z "$c" ]; then
      n=0
    else
      LC_ALL=C printf -v n '%d' "'$c"
    fi
    printf -v hex '%02x' $((n & 255))
    printf '%s %s\n' "${EPOCHREALTIME:-$SECONDS}" "$hex" >> "$out"
    [ "$hex" = 03 ] && break
  fi
done
