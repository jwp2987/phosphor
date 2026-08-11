#!/usr/bin/env bash
# Installs the Zap CLI binary on the remote host, used by remote-server-proxy.
#
# setup.rs replaces these placeholders at runtime:
#   {download_base_url}     - e.g. https://github.com/jwp2987/phosphor/releases/latest/download
#   {install_dir}           - e.g. ~/.zap/remote-server
#   {binary_name}           - e.g. zap-oss (the channel COMMAND name)
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

staging_tarball_path="{staging_tarball_path}"
if [ -n "$staging_tarball_path" ]; then
  case "$staging_tarball_path" in
    "~"|"~/"*) staging_tarball_path="${HOME}${staging_tarball_path#\~}" ;;
  esac
  mv "$staging_tarball_path" "$tmpdir/zap.tar.gz"
else
  url="{download_base_url}/{release_asset_prefix}-$os_name-$arch_name.tar.gz"
  if command -v curl >/dev/null 2>&1; then
    curl -fSL --connect-timeout 15 "$url" -o "$tmpdir/zap.tar.gz"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$tmpdir/zap.tar.gz" "$url"
  else
    echo "error: neither curl nor wget is available" >&2
    exit 3
  fi
fi

tar -xzf "$tmpdir/zap.tar.gz" -C "$tmpdir"

bin="$tmpdir/{binary_name}"
if [ ! -f "$bin" ]; then
  bin=$(find "$tmpdir" -type f \( -name 'zap-oss' -o -name 'warp-oss' -o -name 'oz*' \) ! -path "$tmpdir/resources/*" ! -name '*.tar.gz' | head -n1)
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
