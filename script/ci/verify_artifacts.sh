#!/usr/bin/env bash
# script/ci/verify_artifacts.sh <linux|macos> <dist-dir> [sums-file]
# Post-build verification of what a build/release job is about to publish (#1515). Honest about scope:
#   FAILS the job:  the sums file is missing, lists a file that is absent, or does not match the bytes in <dist-dir>;
#                   an artifact not covered by the sums; a .deb that dpkg-deb cannot read or whose payload lacks the app
#                   binary / .desktop entry; a DMG that hdiutil cannot verify.
#   REPORTS ONLY:   code-signing state. These builds are UNSIGNED / ad-hoc signed (no Developer ID, no notarization: see
#                   docs/release-signing.md), so `codesign --verify` and `spctl --assess` results are printed with an
#                   "UNSIGNED" note and never fail the job. They start failing it once PHOSPHOR_REQUIRE_SIGNED=1.
# Nothing here needs root: the .deb is unpacked with `dpkg -x` into a scratch directory.
set -u
plat="${1:?platform: linux|macos}"; dir="${2:?dist dir}"; sums="${3:-$dir/SHA256SUMS}"
fail=0; note() { echo "verify_artifacts: $*"; }; bad() { echo "verify_artifacts: ERROR $*" >&2; fail=1; }
[ -d "$dir" ] || { echo "verify_artifacts: ERROR no such dir $dir" >&2; exit 1; }

# 1. checksums: present, non-empty, every listed file exists and matches, every file is listed.
if [ ! -s "$sums" ]; then bad "sums file $sums is missing or empty"; else
  if command -v sha256sum >/dev/null; then shacmd="sha256sum"; else shacmd="shasum -a 256"; fi
  n=0
  while read -r want name; do
    [ -n "$want" ] || continue; name="${name#\*}"; n=$((n + 1))
    f="$dir/$name"
    [ -f "$f" ] || { bad "$sums lists $name but $f does not exist"; continue; }
    got=$($shacmd "$f" | awk '{print $1}')
    [ "$got" = "$want" ] || bad "digest mismatch for $name: sums say $want, file is $got"
  done < "$sums"
  [ "$n" -gt 0 ] || bad "$sums has no entries"
  for f in "$dir"/*; do
    b=$(basename "$f"); [ -f "$f" ] || continue; [ "$b" = "$(basename "$sums")" ] && continue
    awk -v n="$b" '{x=$2; sub(/^\*/,"",x); if (x==n) found=1} END{exit !found}' "$sums" || bad "$b is in $dir but not in $sums"
  done
  note "checksums: $n entries checked against $dir"
fi

case "$plat" in
linux)
  scratch=$(mktemp -d) || exit 1; trap 'rm -rf "$scratch"' EXIT
  for deb in "$dir"/*.deb; do
    [ -f "$deb" ] || continue
    note "deb: $(basename "$deb")"
    dpkg-deb --info "$deb" >"$scratch/info" 2>&1 || { cat "$scratch/info" >&2; bad "dpkg-deb --info failed for $deb"; continue; }
    grep -E '^ *(Package|Version|Architecture):' "$scratch/info"
    dpkg-deb --contents "$deb" >"$scratch/contents" 2>&1 || { bad "dpkg-deb --contents failed for $deb"; continue; }
    [ "$(wc -l <"$scratch/contents")" -gt 0 ] || bad "$deb has an empty payload"
    # install-in-a-scratch-root: unpack as the current user, then check the binary and its desktop entry landed
    root="$scratch/root-$(basename "$deb")"; mkdir -p "$root"
    dpkg -x "$deb" "$root" || { bad "dpkg -x failed for $deb"; continue; }
    bin=$(find "$root" -type f -perm -u+x \( -path '*/usr/bin/*' -o -path '*/opt/*' \) \( -name 'phosphor*' -o -name 'zap*' -o -name 'warp*' \) | head -1)
    if [ -z "$bin" ]; then bad "no executable app binary under usr/bin or opt in $deb"
    else
      note "deb payload binary: ${bin#"$root"}"
      head -c 4 "$bin" | grep -q ELF || bad "${bin#"$root"} is not an ELF binary"
    fi
    ls "$root"/usr/share/applications/*.desktop >/dev/null 2>&1 || bad "no .desktop entry in $deb"
  done
  note "linux: the .deb and AppImage are NOT signed (no GPG signature, no repository metadata); the sums are the only integrity check"
  ;;
macos)
  for dmg in "$dir"/*.dmg; do
    [ -f "$dmg" ] || continue
    note "dmg: $(basename "$dmg")"
    hdiutil verify "$dmg" >/dev/null 2>&1 || bad "hdiutil verify failed for $dmg"
    mnt=$(mktemp -d)
    if hdiutil attach -readonly -nobrowse -noverify -mountpoint "$mnt" "$dmg" >/dev/null 2>&1; then
      for app in "$mnt"/*.app; do
        [ -d "$app" ] || continue
        if out=$(codesign --verify --deep --strict --verbose=2 "$app" 2>&1); then
          note "codesign --verify --deep: ok (valid ad-hoc or development signature; NOT Developer ID)"
        else
          echo "$out"; note "codesign --verify: FAILED  [UNSIGNED or invalid signature]"
          [ "${PHOSPHOR_REQUIRE_SIGNED:-}" = 1 ] && bad "codesign verification failed"
        fi
        codesign -dv "$app" 2>&1 | grep -E 'Identifier|Authority|Signature|TeamIdentifier' | sed 's/^/  /'
        if out=$(spctl --assess --type execute --verbose=2 "$app" 2>&1); then
          note "spctl --assess: accepted"
        else
          echo "$out"; note "spctl --assess: REJECTED  [UNSIGNED: not Developer ID signed or notarized, Gatekeeper will warn users. Expected until docs/release-signing.md is done]"
          [ "${PHOSPHOR_REQUIRE_SIGNED:-}" = 1 ] && bad "spctl assessment rejected the app"
        fi
      done
      hdiutil detach "$mnt" >/dev/null 2>&1 || hdiutil detach -force "$mnt" >/dev/null 2>&1 || true
    else
      note "could not mount $dmg (sandbox or no GUI session); codesign/spctl skipped [UNSIGNED state unknown]"
    fi
    rmdir "$mnt" 2>/dev/null || true
  done
  ;;
*) echo "verify_artifacts: unknown platform $plat" >&2; exit 1 ;;
esac
if [ "$fail" = 0 ]; then note "OK"; else echo "verify_artifacts: FAILED" >&2; fi
exit "$fail"
