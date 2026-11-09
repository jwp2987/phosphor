#!/bin/bash
# Image build step: create the accounts of users.tsv (no homes: labd makes them at lease time on a tmpfs /home), /etc/shells, the ash name.
# /etc/sshlab is a symlink to /run/sshlab (Containerfile) and is NOT created here. A failing usersdb.py must fail the build, never create no users.
set -euo pipefail
L=/usr/local/lib/sshlab
ln -sf /bin/busybox /usr/local/bin/ash
python3 "$L/usersdb.py" shells >> /etc/shells
python3 "$L/usersdb.py" useradd > /tmp/useradd.list
test -s /tmp/useradd.list
while read -r line; do $line; done < /tmp/useradd.list
rm -f /tmp/useradd.list
mkdir -p /var/lib/appci/ctl /work
