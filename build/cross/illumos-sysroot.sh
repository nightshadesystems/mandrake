#!/bin/bash
#
# Fetch the illumos sysroot that `cargo check --target x86_64-unknown-illumos`
# needs on a non-illumos host. Crates with C code (ring, and SQLite on
# non-illumos hosts) run a C compiler even for a type-check, so clang needs
# illumos headers to point at. This is the same sysroot the Rust project's
# own CI uses for the illumos target.
#
#   illumos-sysroot.sh [DIR]     default: ~/.cache/mandrake/illumos-sysroot
#
# Prints the environment `just check-illumos` sets, for use by hand.

set -euo pipefail

release="20181213-de6af22ae73b-v1"
url="https://github.com/illumos/sysroot/releases/download/$release/illumos-sysroot-i386-$release.tar.gz"
dir=${1:-$HOME/.cache/mandrake/illumos-sysroot}

if [ ! -f "$dir/usr/include/assert.h" ]; then
    echo "=== fetching illumos sysroot $release into $dir"
    mkdir -p "$dir"
    tmp=$(mktemp)
    curl -sfL -o "$tmp" "$url"
    # On Windows a few symlinks in the tarball cannot be created; the
    # headers and libraries we need extract fine, so tolerate that.
    tar -xzf "$tmp" -C "$dir" 2>/dev/null || true
    rm -f "$tmp"
    [ -f "$dir/usr/include/assert.h" ] || {
        echo "illumos-sysroot: extraction failed, $dir/usr/include/assert.h missing" >&2
        exit 1
    }
fi

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) shown=$(cygpath -m "$dir") ;;
    *) shown=$dir ;;
esac

echo "illumos sysroot ready at $dir"
echo "export CC_x86_64_unknown_illumos=\"clang --target=x86_64-unknown-illumos --sysroot=$shown\""
echo "export AR_x86_64_unknown_illumos=llvm-ar"
