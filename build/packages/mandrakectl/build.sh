#!/usr/bin/bash
# shellcheck disable=SC2034,SC2154,SC2086,SC2164 # omnios-build framework: globals, word-split tools, pushd/popd style
#
# system/mandrake/cli: mandrakectl.
#
# Shares the cargo target directory with the daemon build, so building
# after system/mandrake/daemon reuses its compiled dependencies.

# shellcheck source=/dev/null
. ../../omnios-build/lib/build.sh

PROG=mandrakectl
PKG=system/mandrake/cli
SUMMARY="Mandrake command-line client"
DESC="mandrakectl is a thin JSON-first client for the Mandrake API, for \
scripting and for recovery over the daemon's root socket."

: "${MANDRAKE_SOURCE:=$SRCDIR/../../..}"
MANDRAKE_SOURCE=$(cd "$MANDRAKE_SOURCE" && pwd)
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' "$MANDRAKE_SOURCE/Cargo.toml" | head -1)
[ -n "$VER" ] || logerr "cannot read the workspace version from Cargo.toml"

set_arch 64
forgo_isaexec
PATH+=:$OOCEBIN

BUILD_DEPENDS_IPS="
    ooce/developer/rust
"
RUN_DEPENDS_IPS="
    ?system/mandrake/daemon
"

BUILDDIR=mandrake-$VER
CARGO_TARGET=$TMPDIR/mandrake-target
export CARGO_TARGET_DIR=$CARGO_TARGET

copy_source() {
    logmsg "Copying source from $MANDRAKE_SOURCE"
    logcmd $MKDIR -p $TMPDIR/$BUILDDIR || logerr "mkdir"
    logcmd $RSYNC -a --delete \
        --exclude .git --exclude target --exclude node_modules \
        --exclude console/dist --exclude console/public/vendor \
        --exclude build/out --exclude build/omnios-build --exclude build/kayak \
        "$MANDRAKE_SOURCE/" "$TMPDIR/$BUILDDIR/" || logerr "rsync failed"
    ((EXTRACT_MODE)) && exit
}

build_cli() {
    logmsg "Building mandrakectl (release)"
    pushd $TMPDIR/$BUILDDIR >/dev/null || logerr "chdir"
    logcmd $CARGO build --release --locked -p mandrakectl || logerr "cargo build failed"
    popd >/dev/null
}

install_cli() {
    logmsg "Installing"
    logcmd $MKDIR -p $DESTDIR/usr/bin || logerr "mkdir"
    logcmd $CP $CARGO_TARGET/release/mandrakectl $DESTDIR/usr/bin/mandrakectl \
        || logerr "install binary"
}

init
copy_source
prep_build
build_cli
install_cli
make_package
clean_up
