#!/bin/sh
# p_noinstall: refuse scp / sftp (what the remote-server installer's fallback uses); everything else runs normally.
case "$SSH_ORIGINAL_COMMAND" in
  scp\ *|*/scp\ *|*sftp-server*|internal-sftp*) echo "sshlab: scp/sftp are disabled for this user" >&2; exit 1 ;;
esac
if [ -n "$SSH_ORIGINAL_COMMAND" ]; then exec "$SHELL" -c "$SSH_ORIGINAL_COMMAND"; fi
exec "$SHELL" -l
