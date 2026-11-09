#!/bin/sh
# p_noinstall and p_noscp: refuse scp / sftp (what the remote helper installer's fallback uses); everything else runs normally,
# including `bash -s` with the helper streamed on stdin (the in-band upload). p_noinstall also has a read-only ~/.phosphor; p_noscp does not.
case "$SSH_ORIGINAL_COMMAND" in
  scp\ *|*/scp\ *|*sftp-server*|internal-sftp*) echo "sshlab: scp/sftp are disabled for this user" >&2; exit 1 ;;
esac
if [ -n "$SSH_ORIGINAL_COMMAND" ]; then exec "$SHELL" -c "$SSH_ORIGINAL_COMMAND"; fi
exec "$SHELL" -l
