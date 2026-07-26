#!/usr/bin/env bash
# Pre-install check for the Zap remote-server binary.
#
# stdout prints a structured key=value summary. Exit code 0 means the probe
# completed; non-zero means the probe process failed, and the client will
# treat it as `status=unknown` and fail open.
#
# Important: the Zap Linux remote-server is now statically linked by
# zap_release.yml for the `x86_64-unknown-linux-musl` target (static-musl). The
# artifact doesn't depend on the host's dynamic libc, so it can run on any
# Linux x86_64 host — including older glibc distros (CentOS 7 = 2.17, Amazon
# Linux 2 = 2.26, Ubuntu 20.04 / Debian 11 = 2.31) and musl distros (Alpine, etc.).
#
# Since the binary is static, libc probing is no longer used as a "gate", only kept as telemetry.

set -u

# Legacy field: kept `required_glibc` for compatibility with old client parsing logic.
# A static musl binary actually has no glibc floor; this is output purely for
# backward compatibility and no longer participates in the status decision below.
required_glibc="2.17"
echo "required_glibc=${required_glibc}"

# 1. Identify the libc family, and the version in the glibc case (pure telemetry, doesn't affect status).
libc_family="unknown"
libc_version=""

if version=$(getconf GNU_LIBC_VERSION 2>/dev/null); then
    # Output looks like: "glibc 2.35"
    libc_family="glibc"
    libc_version="${version##* }"
elif ldd_out=$(ldd --version 2>&1 | head -n1); then
    case "$ldd_out" in
        *musl*)   libc_family="musl"   ;;
        *uClibc*) libc_family="uclibc" ;;
        *)
            v=$(printf '%s\n' "$ldd_out" | grep -oE '[0-9]+\.[0-9]+' | head -n1)
            if [ -n "$v" ]; then
                libc_family="glibc"
                libc_version="$v"
            fi
            ;;
    esac
fi

echo "libc_family=${libc_family}"
[ -n "$libc_version" ] && echo "libc_version=${libc_version}"

# 2. Determine the support status.
#
# remote-server is a static musl binary that doesn't link the host libc, so it
# can run under any glibc version (including below 2.35) as well as musl /
# uclibc hosts. As long as we can successfully identify this as a Linux
# x86_64 host, we report `supported`; if no libc clues at all can be probed
# (neither getconf nor ldd is available), fall back to `unknown`, letting the
# client fail open and try the install as usual.
status="unknown"
reason=""

if [ "$libc_family" = "glibc" ] \
   || [ "$libc_family" = "musl" ] \
   || [ "$libc_family" = "uclibc" ] \
   || [ "$libc_family" = "bionic" ]; then
    status="supported"
fi

echo "status=${status}"
if [ -n "$reason" ]; then
    echo "reason=${reason}"
fi
