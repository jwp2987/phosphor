# One-version cleanup: keep exactly one installed remote-server version on this host.
#
# A shell fragment shared by two callers, so they cannot drift apart: the install script
# (install_remote_server.sh, run after the new binary has proved it starts) and
# `setup::one_version_cleanup_command` (run by the client after `verify_installed_binary`
# and on every connect that finds the right version already in place).
#
# Inputs, set by the caller before this text runs:
#   ovc_dir       the install directory (already expanded or tilde-led)
#   ovc_binaries  space-separated base names whose versioned/unversioned files are managed
#   ovc_keep      the file name inside ovc_dir to keep
#   ovc_hash      short hash of the kept version, as used in daemon file names; empty
#                 for an unversioned (dev) build, which sweeps no daemon files
#
# What it removes: every other file named "<base>" or "<base>-*" in ovc_dir (older versions
# and the legacy unversioned binary), and the socket and pid files of other versions whose
# daemon is no longer running. It never touches the files of a live daemon (the daemon exits on
# its own idle timeout), the resources tree, or anything else. Always succeeds.
for ovc_base in $ovc_binaries; do
  for ovc_f in "$ovc_dir/$ovc_base" "$ovc_dir/$ovc_base"-*; do
    [ -f "$ovc_f" ] || continue
    [ "$ovc_f" = "$ovc_dir/$ovc_keep" ] && continue
    rm -f "$ovc_f" 2>/dev/null || true
  done
done
if [ -n "$ovc_hash" ]; then
  for ovc_d in "$ovc_dir"/*/; do
    [ -d "$ovc_d" ] || continue
    for ovc_p in "$ovc_d"server.pid "$ovc_d"server-*.pid; do
      [ -f "$ovc_p" ] || continue
      [ "$ovc_p" = "${ovc_d}server-$ovc_hash.pid" ] && continue
      ovc_pid=$(cat "$ovc_p" 2>/dev/null || true)
      if [ -n "$ovc_pid" ] && kill -0 "$ovc_pid" 2>/dev/null; then continue; fi
      rm -f "$ovc_p" "${ovc_p%.pid}.sock" 2>/dev/null || true
    done
    for ovc_s in "$ovc_d"server.sock "$ovc_d"server-*.sock; do
      [ -e "$ovc_s" ] || continue
      [ "$ovc_s" = "${ovc_d}server-$ovc_hash.sock" ] && continue
      [ -f "${ovc_s%.sock}.pid" ] && continue
      rm -f "$ovc_s" 2>/dev/null || true
    done
  done
fi
:
