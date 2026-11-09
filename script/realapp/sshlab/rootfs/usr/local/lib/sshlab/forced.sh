#!/bin/sh
# p_force: sshd's ForceCommand. Prints a marker and runs the user's shell as a login shell (or the original command).
echo "forced-command-active" >&2
if [ -n "$SSH_ORIGINAL_COMMAND" ]; then exec "$SHELL" -c "$SSH_ORIGINAL_COMMAND"; fi
exec "$SHELL" -l
