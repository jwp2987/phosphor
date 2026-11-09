#!/usr/bin/env bash
# Installs the bundled `phosphor-remote` helper on the remote host. Nothing is downloaded:
# the client pipes this script into `bash -s` and appends the gzip-compressed helper to the
# same stdin (or, for the SCP fallback, uploads the file first and names it below).
#
# setup.rs replaces these placeholders at runtime:
#   {install_dir}           - e.g. ~/.phosphor/remote-server
#   {helper_file_name}      - e.g. phosphor-remote-v0.1.18 (the file this run installs)
#   {helper_current_link}   - name of the symlink pointed at the verified helper
#   {one_version_binaries}  - base names the one-version cleanup manages
#   {daemon_version_hash}   - short hash of the helper version (daemon file names)
#   {staging_gz_path}       - SCP-fallback upload to read instead of stdin; empty for in-band
#   {sha256_linux_x86_64}, {sha256_linux_aarch64}, {sha256_macos_x86_64}, {sha256_macos_aarch64}
#                           - SHA-256 of the DECOMPRESSED helper for each platform, compiled
#                             into the client (setup.rs::pinned_helper_sha256) and delivered
#                             down the authenticated ssh channel this script arrives on.
#   one_version_cleanup     - (braced, on a line of its own further down) the shared cleanup fragment
#                             (one_version_cleanup.sh). Not spelled braced in this comment: every
#                             braced occurrence is substituted, comments included.
#
# STRUCTURE: everything is inside `main` and the last line is the call. That is load-bearing for
# the in-band upload: bash reads this script from a pipe one command at a time, so when it runs
# `main`, the compressed helper that follows the script on the same stdin has not been read yet,
# and `gunzip` inside `main` receives exactly those bytes. A command placed after `main` would
# never run (stdin is at EOF by then).
set -e
set -E

main() {
# ---------------------------------------------------------------------------
# EXIT-CODE CONTRACT
#
# This script's exit status is the client's ONLY signal about what went wrong, and the client
# routes on it. The mirror of this block lives in
# `app/src/remote_server/ssh_transport.rs::classify_install_failure`; a test there parses the
# `EXIT_*=` assignments below out of the rendered script and asserts every one of them is
# classified, so the two cannot drift apart.
#
# Three outcomes have to stay DISTINGUISHABLE:
#
#   * transport failure -- no usable bytes reached the host (EXIT_UPLOAD_FAILED). Nothing was verified
#     because there was nothing to verify; the SCP fallback is the designed recovery.
#   * integrity failure -- bytes arrived and are NOT the pinned helper, or could not be checked
#     (EXIT_NO_PINNED_DIGEST, EXIT_NO_DIGEST_TOOL, EXIT_DIGEST_MISMATCH). Fail closed, never retried through another route.
#   * success.
#
# Every code is chosen outside bash's reserved statuses (1 general error, 2 usage/PARSE error,
# 126 not executable, 127 not found, 128+N signals): a parse error aborts the interpreter
# before any trap exists and exits 2, so an interpreter-level abort lands in the client's
# unrecognised-code branch (which fails closed) instead of impersonating a verdict.
# ---------------------------------------------------------------------------
EXIT_UNSUPPORTED_PLATFORM=10
EXIT_NO_DECOMPRESSOR=3
EXIT_NO_PINNED_DIGEST=4
EXIT_NO_DIGEST_TOOL=5
EXIT_DIGEST_MISMATCH=6
EXIT_UPLOAD_FAILED=7
EXIT_BAD_UPLOAD=8
EXIT_INSTALL_FAILED=9

# Backstop for anything not explicitly guarded: remap an unclassified `set -e` abort onto
# EXIT_INSTALL_FAILED so a stray tool's status can never be mistaken for one of the codes above.
# An explicit `exit N` does NOT go through this trap.
on_unexpected_error() {
  status=$?
  echo "error: install script failed unexpectedly at line $1 (status $status)" >&2
  exit "$EXIT_INSTALL_FAILED"
}
trap 'on_unexpected_error $LINENO' ERR

arch=$(uname -m)
case "$arch" in
  x86_64|amd64)  arch_name=x86_64 ;;
  aarch64|arm64) arch_name=aarch64 ;;
  *) echo "unsupported arch: $arch" >&2; exit "$EXIT_UNSUPPORTED_PLATFORM" ;;
esac

os_kernel=$(uname -s)
case "$os_kernel" in
  Darwin) os_name=macos ;;
  Linux)  os_name=linux ;;
  *) echo "unsupported OS: $os_kernel" >&2; exit "$EXIT_UNSUPPORTED_PLATFORM" ;;
esac

# Everything this script creates is private to the user; the helper itself is chmod 755 below.
umask 077

install_dir="{install_dir}"
case "$install_dir" in
  "~"|"~/"*) install_dir="${HOME}${install_dir#\~}" ;;
esac
# Remember whether this run created the install directory. A failed install must not leave an
# EMPTY directory behind: the client treats the directory's existence as "something was
# installed here" and would silently retry on every later connect instead of offering the
# install/continue choice (#931).
created_install_dir=0
[ -d "$install_dir" ] || created_install_dir=1
mkdir -p "$install_dir"
if [ "$created_install_dir" -eq 1 ]; then chmod 700 "$install_dir"; fi

tmpfile="${install_dir:?}/.up.$$"
cleanup() {
  status=$?
  rm -f "$tmpfile" 2>/dev/null || true
  # `rmdir` only removes an EMPTY directory, so this can never delete a previous install, and
  # only a failed run that created the directory itself even tries.
  if [ "$status" -ne 0 ] && [ "$created_install_dir" -eq 1 ]; then
    rmdir "$install_dir" 2>/dev/null || true
  fi
}
trap cleanup EXIT

# SHA-256 of the decompressed helper for each platform, compiled into the client that generated
# this script. It reaches this host over the authenticated SSH channel, so no network location
# is trusted. Empty means the client was built without a digest; see the fail-closed branch.
case "$os_name-$arch_name" in
  linux-x86_64)   expected_sha256="{sha256_linux_x86_64}" ;;
  linux-aarch64)  expected_sha256="{sha256_linux_aarch64}" ;;
  macos-x86_64)   expected_sha256="{sha256_macos_x86_64}" ;;
  macos-aarch64)  expected_sha256="{sha256_macos_aarch64}" ;;
  *)              expected_sha256="" ;;
esac

# Prints the SHA-256 of "$1" as a bare lowercase hex digest, and returns non-zero -- printing
# nothing -- if it could not compute one. Tool availability differs by platform: coreutils
# `sha256sum` on Linux, BSD `shasum` on macOS, `openssl` as a last resort.
#
# Every branch is a PIPELINE, whose status is its LAST element's; `cut`/`awk` succeed on empty
# input, so a digest tool that is installed and then FAILS used to return 0 with empty output
# and be reported as a digest MISMATCH ("could not compute" fused with "computed, and it is not
# the pinned helper"). `pipefail` is set per invocation so it stays out of the rest of the
# script.
compute_sha256() {
  digest=""
  if command -v sha256sum >/dev/null 2>&1; then
    digest=$(set -o pipefail; sha256sum "$1" | cut -d' ' -f1) || return 1
  elif command -v shasum >/dev/null 2>&1; then
    digest=$(set -o pipefail; shasum -a 256 "$1" | cut -d' ' -f1) || return 1
  elif command -v openssl >/dev/null 2>&1; then
    digest=$(set -o pipefail; openssl dgst -sha256 "$1" | awk '{print $NF}') || return 1
  else
    return 1
  fi
  case "$digest" in
    ""|*[!0-9a-fA-F]*) return 1 ;;
  esac
  printf '%s\n' "$digest"
}

# Refuse rather than install unverified. A client built without the digests cannot tell the
# helper it bundled from a swapped one, and "warn and continue" would silently drop the
# protection exactly when the build is misconfigured.
if [ -z "$expected_sha256" ]; then
  echo "error: this client was built without a pinned SHA-256 for $os_name-$arch_name;" >&2
  echo "       refusing to install an unverified remote helper" >&2
  exit "$EXIT_NO_PINNED_DIGEST"
fi

if command -v gunzip >/dev/null 2>&1; then
  decompress="gunzip -c"
elif command -v gzip >/dev/null 2>&1; then
  decompress="gzip -dc"
else
  echo "error: neither gunzip nor gzip is available on this host" >&2
  exit "$EXIT_NO_DECOMPRESSOR"
fi

staging_gz_path="{staging_gz_path}"
if [ -n "$staging_gz_path" ]; then
  case "$staging_gz_path" in
    "~"|"~/"*) staging_gz_path="${HOME}${staging_gz_path#\~}" ;;
  esac
  if [ ! -s "$staging_gz_path" ]; then
    echo "error: the uploaded helper is missing or empty: $staging_gz_path" >&2
    exit "$EXIT_UPLOAD_FAILED"
  fi
  # `|| decompress_status=$?` is load-bearing: capture the status instead of letting `set -e`
  # abort with the decompressor's own, which would collide with this script's codes.
  decompress_status=0
  $decompress < "$staging_gz_path" > "$tmpfile" || decompress_status=$?
  rm -f "${staging_gz_path:?}" 2>/dev/null || true
else
  decompress_status=0
  $decompress > "$tmpfile" || decompress_status=$?
fi
if [ ! -s "$tmpfile" ]; then
  # Nothing came out of the decompressor: no bytes reached this host (an empty or immediately
  # broken stream). Nothing was verified; a transport problem, not a verdict.
  echo "error: no helper data arrived (decompressor status $decompress_status)" >&2
  exit "$EXIT_UPLOAD_FAILED"
fi
if [ "$decompress_status" -ne 0 ]; then
  echo "error: could not decompress the uploaded helper (status $decompress_status)" >&2
  exit "$EXIT_BAD_UPLOAD"
fi

# An empty `actual_sha256` is not a mismatching digest -- it is the absence of a verdict, which
# is what EXIT_NO_DIGEST_TOOL means; the client reports the two apart.
actual_sha256=$(compute_sha256 "$tmpfile") || actual_sha256=""
if [ -z "$actual_sha256" ]; then
  echo "error: could not compute a SHA-256 of the uploaded helper" >&2
  echo "       (no working sha256sum, shasum or openssl on this host);" >&2
  echo "       refusing to install an unverified remote helper" >&2
  exit "$EXIT_NO_DIGEST_TOOL"
fi
if [ "$actual_sha256" != "$expected_sha256" ]; then
  echo "error: remote helper failed integrity check" >&2
  echo "       expected: $expected_sha256" >&2
  echo "       actual:   $actual_sha256" >&2
  exit "$EXIT_DIGEST_MISMATCH"
fi

helper_path="${install_dir:?}/{helper_file_name}"
chmod 755 "$tmpfile"
mv "$tmpfile" "$helper_path"
# The binary must actually start on this host before anything older is touched.
if ! "$helper_path" --version >/dev/null 2>&1; then
  echo "error: the installed helper does not start on this host" >&2
  rm -f "$helper_path" 2>/dev/null || true
  exit "$EXIT_INSTALL_FAILED"
fi
ln -sfn "{helper_file_name}" "$install_dir/{helper_current_link}" 2>/dev/null || true

# One-version cleanup (setup.rs::one_version_cleanup_command is the client-side twin of this
# block; both run the same fragment), now that the new helper has proved it starts.
ovc_dir="$install_dir"
ovc_binaries="{one_version_binaries}"
ovc_keep="{helper_file_name}"
ovc_hash="{daemon_version_hash}"
{one_version_cleanup}
}

main
