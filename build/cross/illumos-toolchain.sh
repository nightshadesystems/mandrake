#!/bin/bash
#
# Build a GCC cross toolchain for x86_64-unknown-illumos on Linux, the way
# the Rust project's own CI does for its illumos target: binutils and GCC
# against the illumos sysroot from github.com/illumos/sysroot.
#
#   illumos-toolchain.sh [sysroot|binutils|gcc|all] [PREFIX]
#
#   PREFIX  install directory (default ~/.cache/mandrake/illumos-toolchain)
#
# Afterwards, for `cargo build --target x86_64-unknown-illumos`:
#
#   export PATH="$PREFIX/bin:$PATH"
#   export CC_x86_64_unknown_illumos=x86_64-illumos-gcc
#   export CXX_x86_64_unknown_illumos=x86_64-illumos-g++
#   export AR_x86_64_unknown_illumos=x86_64-illumos-ar
#   export RANLIB_x86_64_unknown_illumos=x86_64-illumos-ranlib
#   export CARGO_TARGET_X86_64_UNKNOWN_ILLUMOS_LINKER=x86_64-illumos-gcc
#
# GCC 8.4 is deliberate: newer 7.x refuse to enable TLS for Solaris targets
# and the Rust CI note says to re-verify TLS when changing it. Building it
# needs a host GCC of about the same era (Ubuntu 20.04's 9.x works); the
# build workflow runs this in such a container. Needs: build-essential,
# curl, xz-utils, bzip2, libgmp-dev, libmpfr-dev, libmpc-dev, texinfo, file.
# Documentation is skipped (MAKEINFO=true) so a missing makeinfo cannot
# fail the build either way.

set -euo pipefail

phase=${1:-all}
prefix=${2:-$HOME/.cache/mandrake/illumos-toolchain}
arch=x86_64
build_target=$arch-pc-solaris2.10
program_prefix=$arch-illumos-
jobs=$(getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)

gcc_version=8.4.0
gcc_sum=e30a6e52d10e1f27ed55104ad233c30bd1e99cfb5ff98ab022dc941edd1b2dd4
gcc_url="https://ftp.gnu.org/gnu/gcc/gcc-$gcc_version/gcc-$gcc_version.tar.xz"

binutils_version=2.40
binutils_sum=f8298eb153a4b37d112e945aa5cb2850040bcf26a3ea65b5a715c83afe05e48a
binutils_url="https://ftp.gnu.org/gnu/binutils/binutils-$binutils_version.tar.bz2"

sysroot_version=20181213-de6af22ae73b-v1
sysroot_sum=ee792d956dfa6967453cebe9286a149143290d296a8ce4b8a91d36bea89f8112
sysroot_url="https://github.com/illumos/sysroot/releases/download/$sysroot_version/illumos-sysroot-i386-$sysroot_version.tar.gz"
sysroot_dir=$prefix/sysroot

work=${MANDRAKE_TOOLCHAIN_WORK:-${TMPDIR:-/tmp}/mandrake-illumos-toolchain}

die() { echo "illumos-toolchain: $*" >&2; exit 1; }
log() { printf '\n=== %s\n' "$*"; }

# fetch URL SHA256 DEST_DIR: download, verify, extract into DEST_DIR.
fetch() {
    local url=$1 sum=$2 dest=$3 file
    file=$work/$(basename "$url")
    mkdir -p "$work" "$dest"
    if [ ! -f "$file" ]; then
        log "downloading $url"
        curl -sfL -o "$file" "$url"
    fi
    echo "$sum  $file" | sha256sum -c - >/dev/null || die "checksum mismatch for $file"
    log "extracting $(basename "$file") into $dest"
    case $file in
        *.tar.xz) tar -xJf "$file" -C "$dest" ;;
        *.tar.bz2) tar -xjf "$file" -C "$dest" ;;
        *.tar.gz) tar -xzf "$file" -C "$dest" ;;
        *) die "unknown archive $file" ;;
    esac
}

do_sysroot() {
    [ -f "$sysroot_dir/usr/include/assert.h" ] && { echo "sysroot present"; return; }
    fetch "$sysroot_url" "$sysroot_sum" "$sysroot_dir"
}

do_binutils() {
    [ -x "$prefix/bin/${program_prefix}ld" ] && { echo "binutils present"; return; }
    fetch "$binutils_url" "$binutils_sum" "$work/src"
    mkdir -p "$work/build/binutils"
    log "building binutils $binutils_version"
    (
        cd "$work/build/binutils"
        "$work/src/binutils-$binutils_version/configure" \
            --prefix="$prefix" \
            --target="$build_target" \
            --program-prefix="$program_prefix" \
            --with-sysroot="$sysroot_dir" \
            --disable-werror >configure.log
        make -j "$jobs" MAKEINFO=true >make.log 2>&1 || { tail -50 make.log; die "binutils build failed"; }
        make install MAKEINFO=true >install.log
    )
    rm -rf "$work/build/binutils" "$work/src/binutils-$binutils_version"
}

do_gcc() {
    [ -x "$prefix/bin/${program_prefix}gcc" ] && { echo "gcc present"; return; }
    fetch "$gcc_url" "$gcc_sum" "$work/src"
    mkdir -p "$work/build/gcc"
    log "building gcc $gcc_version (this takes a while)"
    (
        cd "$work/build/gcc"
        export PATH="$prefix/bin:$PATH"
        export CFLAGS='-fPIC'
        export CXXFLAGS='-fPIC'
        export CFLAGS_FOR_TARGET='-fPIC'
        export CXXFLAGS_FOR_TARGET='-fPIC'
        "$work/src/gcc-$gcc_version/configure" \
            --prefix="$prefix" \
            --target="$build_target" \
            --program-prefix="$program_prefix" \
            --with-sysroot="$sysroot_dir" \
            --with-gnu-as \
            --with-gnu-ld \
            --disable-nls \
            --disable-libgomp \
            --disable-libquadmath \
            --disable-libssp \
            --disable-libvtv \
            --disable-libcilkrts \
            --disable-libada \
            --disable-libsanitizer \
            --disable-libquadmath-support \
            --disable-shared \
            --disable-werror \
            --enable-languages=c,c++ \
            --enable-tls >configure.log
        make -j "$jobs" MAKEINFO=true >make.log 2>&1 || { tail -80 make.log; die "gcc build failed"; }
        make install MAKEINFO=true >install.log
    )
    rm -rf "$work/build/gcc" "$work/src/gcc-$gcc_version"
}

case $phase in
    sysroot) do_sysroot ;;
    binutils) do_sysroot; do_binutils ;;
    gcc) do_sysroot; do_gcc ;;
    all) do_sysroot; do_binutils; do_gcc ;;
    *) die "unknown phase '$phase' (sysroot|binutils|gcc|all)" ;;
esac

log "toolchain at $prefix"
for tool in "$prefix/bin/$program_prefix"*; do
    printf '%s ' "$(basename "$tool")"
done
echo
cat <<EOF

export PATH="$prefix/bin:\$PATH"
export CC_x86_64_unknown_illumos=${program_prefix}gcc
export CXX_x86_64_unknown_illumos=${program_prefix}g++
export AR_x86_64_unknown_illumos=${program_prefix}ar
export RANLIB_x86_64_unknown_illumos=${program_prefix}ranlib
export CARGO_TARGET_X86_64_UNKNOWN_ILLUMOS_LINKER=${program_prefix}gcc
EOF
