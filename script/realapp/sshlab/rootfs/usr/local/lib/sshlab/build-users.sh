#!/bin/sh
# Image build step: create the accounts of users.tsv (no homes: labd makes them at lease time on a tmpfs /home), /etc/shells, the ash name, groups.
set -eu
L=/usr/local/lib/sshlab
ln -sf /bin/busybox /usr/local/bin/ash
python3 "$L/usersdb.py" shells >> /etc/shells
python3 "$L/usersdb.py" useradd | while read -r line; do $line; done
mkdir -p /var/lib/appci/ctl /work /etc/sshlab
