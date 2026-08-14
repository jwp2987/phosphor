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

arch=$(uname -m)
case "$arch" in
  x86_64|amd64)  arch_name=x86_64 ;;
  aarch64|arm64) arch_name=aarch64 ;;
  *) echo "unsupported arch: $arch" >&2; exit 2 ;;
esac

os_kernel=$(uname -s)
case "$os_kernel" in
  Darwin) os_name=macos ;;
  Linux)  os_name=linux ;;
  *) echo "unsupported OS: $os_kernel" >&2; exit 2 ;;
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

# Prints the SHA-256 of "$1" as a bare lowercase hex digest. Tool availability differs by
# platform: coreutils `sha256sum` on Linux, BSD `shasum` on macOS, `openssl` as a last resort.
compute_sha256() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d' ' -f1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d' ' -f1
  elif command -v openssl >/dev/null 2>&1; then
    openssl dgst -sha256 "$1" | awk '{print $NF}'
  else
    return 1
  fi
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
    exit 4
  fi
  # --proto/--proto-redir keep a redirect from downgrading the transport to plain HTTP;
  # release downloads legitimately redirect to a CDN, so -L has to stay.
  if command -v curl >/dev/null 2>&1; then
    curl -fSL --proto '=https' --proto-redir '=https' --connect-timeout 15 "$url" -o "$tmpdir/zap.tar.gz"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --https-only -O "$tmpdir/zap.tar.gz" "$url"
  else
    echo "error: neither curl nor wget is available" >&2
    exit 3
  fi

  actual_sha256=$(compute_sha256 "$tmpdir/zap.tar.gz") || {
    echo "error: no SHA-256 tool available (need sha256sum, shasum or openssl);" >&2
    echo "       refusing to install an unverified remote server" >&2
    exit 5
  }
  if [ "$actual_sha256" != "$expected_sha256" ]; then
    echo "error: remote server tarball failed integrity check" >&2
    echo "       url:      $url" >&2
    echo "       expected: $expected_sha256" >&2
    echo "       actual:   $actual_sha256" >&2
    exit 6
  fi
fi

tar -xzf "$tmpdir/zap.tar.gz" -C "$tmpdir"

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
if [ -z "$bin" ]; then echo "no binary found in tarball" >&2; exit 1; fi
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
