#!/usr/bin/env bash
# Installs the Zap CLI binary on the remote host, used by remote-server-proxy.
#
# setup.rs replaces these placeholders at runtime:
#   {download_base_url}     - e.g. https://github.com/jwp2987/phosphor/releases/latest/download
#   {install_dir}           - e.g. ~/.phosphor/remote-server
#   {binary_name}           - e.g. phosphor-oss (the channel COMMAND name)
#   {release_asset_prefix}  - e.g. phosphor-cli (the published ASSET name --
#                             deliberately different from {binary_name}; they
#                             drifted apart at the rebrand and the mismatch made
#                             every remote install 404)
#   {version_suffix}        - e.g. -v0.2026..., empty when there's no release tag
#   {staging_tarball_path}  - pre-uploaded tarball path for the SCP fallback; empty for the normal download path
#   {bundled_resources_dir_name}
#                           - e.g. bundled_resources; the global, version-independent
#                             directory under {install_dir} that receives the release
#                             artifact's resources/ tree (bundled skills, settings schema)
#   {sha256_linux_x86_64}, {sha256_linux_aarch64}, {sha256_macos_x86_64}, {sha256_macos_aarch64}
#                           - SHA-256 of the published CLI tarball for each remote platform,
#                             compiled into the client at build time (setup.rs::expected_sha256);
#                             empty when the client was built without a pinned digest. Delivered
#                             over the SSH channel this script arrives on, not fetched from
#                             GitHub -- see the fail-closed check on the download path below.
set -e

# ---------------------------------------------------------------------------
# EXIT-CODE CONTRACT
#
# This script's exit status is the client's ONLY signal about what went wrong,
# and the client routes on it: some failures are grounds for the SCP fallback
# (client downloads the tarball itself and pushes it over the authenticated
# SSH channel) and some must fail closed. The mirror of this block lives in
# `app/src/remote_server/ssh_transport.rs::classify_install_failure`; a test
# there parses the `EXIT_*=` assignments below out of the rendered script and
# asserts every one of them is classified, so the two cannot drift apart.
#
# Three outcomes have to stay DISTINGUISHABLE:
#
#   * transport failure -- the host could not fetch the bytes at all. Nothing
#     was verified because there was nothing to verify. The SCP fallback is the
#     designed recovery.
#   * integrity failure -- the bytes arrived and are NOT the pinned release, or
#     could not be checked. Fail closed. The SCP fallback installs through the
#     unverified staging branch below, so treating this as retryable would turn
#     detected tampering into an unverified install. This is the security
#     property; do not weaken it.
#   * success.
#
# Every fallible command is therefore either guarded (`|| code=$?`) or covered
# by the ERR trap below. `set -e` on its own exits with the FAILING COMMAND'S
# status, and curl's status space overlaps this one -- curl 6 (DNS failure) is
# this script's "digest mismatch", curl 7 (connection refused) and 22 (HTTP
# error) are unassigned here. Letting those leak meant a DNS failure was read
# as tampering and hard-failed, while an attacker-injected RST or a 500 was
# read as an unrecognised code and fell through to the unverified fallback.
# Both directions were wrong, which is why nothing is left to `set -e` now.
# ---------------------------------------------------------------------------
# EXIT_UNSUPPORTED_PLATFORM is 10, NOT 2: bash reserves its own statuses --
# 1 (general error), 2 (usage/PARSE error), 126 (found but not executable),
# 127 (not found) and 128+N (killed by signal N). A syntax error cannot be
# caught by the ERR trap below, because the interpreter never gets as far as
# running the trap; it just exits 2. While this script owned 2, a script the
# placeholder substitution in `setup.rs::install_script` had mangled would
# have arrived at the client as "unsupported arch/OS". Every code below is
# therefore chosen outside bash's reserved set, so an interpreter-level abort
# lands in the client's unrecognised-code branch (which fails closed) instead
# of impersonating a verdict this script never reached.
EXIT_UNSUPPORTED_PLATFORM=10
EXIT_NO_FETCHER=3
EXIT_NO_PINNED_DIGEST=4
EXIT_NO_DIGEST_TOOL=5
EXIT_DIGEST_MISMATCH=6
EXIT_DOWNLOAD_FAILED=7
EXIT_BAD_TARBALL=8
EXIT_INSTALL_FAILED=9

# Backstop for anything not explicitly guarded: remap an unclassified `set -e`
# abort onto EXIT_INSTALL_FAILED so a stray tool's exit status can never be
# mistaken for one of the codes above. An explicit `exit N` does NOT go through
# this trap, so the assignments above still reach the client verbatim.
#
# What this trap does NOT cover -- and why the codes above dodge bash's
# reserved statuses: a parse error aborts the interpreter before any trap is
# installed, so bash's own 2 is emitted with nothing to remap it. That status
# now belongs to no code in this contract, so it fails closed as unrecognised.
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

install_dir="{install_dir}"
case "$install_dir" in
  "~"|"~/"*) install_dir="${HOME}${install_dir#\~}" ;;
esac
mkdir -p "$install_dir"

tmpdir=$(mktemp -d "$install_dir/.install.XXXXXX")
# Best-effort cleanup of the staging directory. A failure here must not mask
# the real install result: when the trap fires, the binary has either already
# been moved to its final path, or the script has already failed for another
# reason — and that other error is more worth surfacing to the caller.
cleanup() {
  rm -rf "$tmpdir" 2>/dev/null || true
}
trap cleanup EXIT

# SHA-256 of each published tarball, compiled into the client that generated this script
# (setup.rs::expected_sha256). It reaches this host over the authenticated SSH channel, NOT
# from GitHub -- which is what makes it a real integrity check rather than a checksum the
# same attacker could replace alongside the artifact. Empty means the client was built
# without the digests; see the fail-closed branch below.
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
# Every branch is a PIPELINE, and a pipeline's status is its LAST element's. `cut`/`awk` succeed
# on empty input, so a digest tool that is installed and then FAILS -- an unreadable staging
# file, an LSM denial, OOM, an openssl in a FIPS profile that refuses the algorithm -- used to
# return 0 here with empty output. The caller's `|| exit "$EXIT_NO_DIGEST_TOOL"` never fired,
# the empty string compared unequal to the pinned digest, and the script reported
# EXIT_DIGEST_MISMATCH: "could not compute" collapsed into "computed, and it is not the release
# we pinned". That is the same fusion one layer down that this whole contract exists to prevent,
# and it fails closed on a FALSE tampering alarm where the SCP fallback would have worked.
#
# `pipefail` is set per invocation rather than globally at the top of the script: the
# `find ... | head -n1` pipelines further down close the pipe on purpose, so `find` dies of
# SIGPIPE and a global `pipefail` would abort those with 141 (measured, not assumed).
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
  # A tool can also succeed and still hand back nothing usable. Anything that is not a bare hex
  # string is "no digest", never a digest that happens to differ from the pinned one.
  case "$digest" in
    ""|*[!0-9a-fA-F]*) return 1 ;;
  esac
  printf '%s\n' "$digest"
}

staging_tarball_path="{staging_tarball_path}"
if [ -n "$staging_tarball_path" ]; then
  case "$staging_tarball_path" in
    "~"|"~/"*) staging_tarball_path="${HOME}${staging_tarball_path#\~}" ;;
  esac
  mv "$staging_tarball_path" "$tmpdir/zap.tar.gz"
  # No verification on this path, deliberately: this tarball was uploaded by the client over
  # the same authenticated SSH connection that is running this script. There is no published
  # release to have a digest for (it is a locally cross-compiled dev binary), and the bytes
  # never crossed an untrusted network.
else
  url="{download_base_url}/{release_asset_prefix}-$os_name-$arch_name.tar.gz"
  # Refuse rather than install unverified. A client built without the digests cannot tell a
  # good release from a tampered one, and "warn and continue" would silently drop the
  # protection precisely when the build is misconfigured.
  if [ -z "$expected_sha256" ]; then
    echo "error: this client was built without a pinned SHA-256 for $os_name-$arch_name;" >&2
    echo "       refusing to install an unverified remote server from $url" >&2
    exit "$EXIT_NO_PINNED_DIGEST"
  fi
  # --proto/--proto-redir keep a redirect from downgrading the transport to plain HTTP;
  # release downloads legitimately redirect to a CDN, so -L has to stay.
  #
  # `|| fetch_status=$?` is load-bearing: it captures the fetcher's status instead of letting
  # `set -e` abort with it. See the EXIT-CODE CONTRACT above -- an uncaptured curl status is
  # indistinguishable from this script's own verdict.
  fetch_status=0
  if command -v curl >/dev/null 2>&1; then
    curl -fSL --proto '=https' --proto-redir '=https' --connect-timeout 15 \
      "$url" -o "$tmpdir/zap.tar.gz" || fetch_status=$?
  elif command -v wget >/dev/null 2>&1; then
    wget -q --https-only -O "$tmpdir/zap.tar.gz" "$url" || fetch_status=$?
  else
    echo "error: neither curl nor wget is available" >&2
    exit "$EXIT_NO_FETCHER"
  fi
  if [ "$fetch_status" -ne 0 ]; then
    # DNS failure, connection refused, TLS failure, timeout, HTTP 4xx/5xx: the host never got
    # the bytes, so no integrity claim was made and none was violated. This is a genuine
    # transport failure and the SCP fallback is legitimate -- unlike the digest failures below.
    echo "error: could not download the remote server tarball" >&2
    echo "       url:          $url" >&2
    echo "       fetcher exit: $fetch_status" >&2
    exit "$EXIT_DOWNLOAD_FAILED"
  fi

  # Two guards for one property, deliberately: `compute_sha256` already refuses to print a
  # non-digest, and an empty `actual_sha256` is re-checked here so that no future edit to the
  # helper can make "no digest" arrive at the comparison below. An empty string is not a
  # mismatching digest -- it is the absence of a verdict, which is what EXIT_NO_DIGEST_TOOL
  # means and why the client classifies the two codes the same way but reports them apart.
  actual_sha256=$(compute_sha256 "$tmpdir/zap.tar.gz") || actual_sha256=""
  if [ -z "$actual_sha256" ]; then
    echo "error: could not compute a SHA-256 of the downloaded tarball" >&2
    echo "       (no working sha256sum, shasum or openssl on this host);" >&2
    echo "       refusing to install an unverified remote server" >&2
    exit "$EXIT_NO_DIGEST_TOOL"
  fi
  if [ "$actual_sha256" != "$expected_sha256" ]; then
    # The download SUCCEEDED and the bytes are not the pinned release. Fail closed: the client
    # must not retry this through the unverified staging path.
    echo "error: remote server tarball failed integrity check" >&2
    echo "       url:      $url" >&2
    echo "       expected: $expected_sha256" >&2
    echo "       actual:   $actual_sha256" >&2
    exit "$EXIT_DIGEST_MISMATCH"
  fi
fi

# On the download path the digest already matched, so a tarball that will not open or holds no
# recognised binary is a broken *pinned* artifact. Re-fetching it via the SCP fallback would
# pull the identical bytes, so these are fatal rather than retryable.
tar -xzf "$tmpdir/zap.tar.gz" -C "$tmpdir" || {
  echo "error: could not extract the remote server tarball" >&2
  exit "$EXIT_BAD_TARBALL"
}

bin="$tmpdir/{binary_name}"
if [ ! -f "$bin" ]; then
  # Fallback for a tarball whose layout does not match {binary_name}: older release assets,
  # and assets published either side of a binary rename. The names are every binary this
  # project has shipped under -- `phosphor-oss` (current), `zap-oss` (pre-rename), and
  # upstream's `warp-oss` / `oz*`. Dropping an old name here would strand hosts installing a
  # previously published tarball; the digest check above already established the archive is
  # the one this client expects, so a broad name match costs nothing.
  bin=$(find "$tmpdir" -type f \( -name 'phosphor-oss' -o -name 'zap-oss' -o -name 'warp-oss' -o -name 'oz*' \) ! -path "$tmpdir/resources/*" ! -name '*.tar.gz' | head -n1)
fi
if [ -z "$bin" ]; then echo "no binary found in tarball" >&2; exit "$EXIT_BAD_TARBALL"; fi
chmod +x "$bin"
mv "$bin" "$install_dir/{binary_name}{version_suffix}"

# Install the artifact's resources/ tree (bundled skills, settings schema) into the
# global, version-independent directory the daemon reads at startup
# (`remote_server_bundled_resources_dir()` in setup.rs). Deliberately NOT
# version-scoped: the last install wins, and the removal command leaves this
# directory in place so an already-running older daemon keeps the skills it
# parsed at startup.
#
# Absent resources/ is not an error: dev-mode installs cross-compile a bare
# binary with no resources tree, and older release artifacts predate it.
resources_src="$tmpdir/resources"
if [ ! -d "$resources_src" ]; then
  resources_src=$(find "$tmpdir" -maxdepth 3 -type d -name resources | head -n1)
fi
if [ -n "$resources_src" ] && [ -d "$resources_src" ]; then
  resources_dir="$install_dir/{bundled_resources_dir_name}"
  # Swap through a staging path so a failed install cannot leave the directory
  # half-populated for a daemon that is starting up concurrently.
  staged="$install_dir/.{bundled_resources_dir_name}.new.$$"
  previous="$install_dir/.{bundled_resources_dir_name}.old.$$"
  rm -rf "$staged" "$previous"
  mv "$resources_src" "$staged"
  if [ -d "$resources_dir" ]; then
    mv "$resources_dir" "$previous"
  fi
  mv "$staged" "$resources_dir"
  rm -rf "$previous"
fi
