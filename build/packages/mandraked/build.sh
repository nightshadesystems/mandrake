#!/usr/bin/bash
# shellcheck disable=SC2034,SC2154,SC2086,SC2164 # omnios-build framework: globals, word-split tools, pushd/popd style
#
# system/mandrake/daemon: mandraked with the embedded console, SMF
# services, method script, RBAC profile, user, and directories.
#
# Uses the omnios-build framework from build/omnios-build. Source is this
# repository, copied into the build directory; nothing is downloaded.

# shellcheck source=/dev/null
. ../../omnios-build/lib/build.sh

PROG=mandraked
PKG=system/mandrake/daemon
SUMMARY="Mandrake host management daemon and web console"
DESC="mandraked serves the Mandrake HTTP API and embedded web console over HTTPS \
and a root recovery socket, and manages zones, bhyve VMs, networking, and storage."

: "${MANDRAKE_SOURCE:=$SRCDIR/../../..}"
MANDRAKE_SOURCE=$(cd "$MANDRAKE_SOURCE" && pwd)
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' "$MANDRAKE_SOURCE/Cargo.toml" | head -1)
[ -n "$VER" ] || logerr "cannot read the workspace version from Cargo.toml"

set_arch 64
forgo_isaexec
PATH+=:$OOCEBIN

BUILD_DEPENDS_IPS="
    ooce/developer/rust
    ooce/runtime/node-22
"
RUN_DEPENDS_IPS="
    database/sqlite-3
    library/security/openssl-3
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

build_console() {
    logmsg "Building the console"
    pushd $TMPDIR/$BUILDDIR/console >/dev/null || logerr "chdir console"
    logcmd env COREPACK_ENABLE_DOWNLOAD_PROMPT=0 corepack pnpm install --frozen-lockfile \
        || logerr "pnpm install failed"
    logcmd env COREPACK_ENABLE_DOWNLOAD_PROMPT=0 corepack pnpm build \
        || logerr "console build failed"
    popd >/dev/null
}

build_daemon() {
    logmsg "Building mandraked (release)"
    pushd $TMPDIR/$BUILDDIR >/dev/null || logerr "chdir"
    logcmd $CARGO build --release --locked -p mandraked || logerr "cargo build failed"
    popd >/dev/null
}

install_daemon() {
    logmsg "Installing"
    logcmd $MKDIR -p $DESTDIR/usr/lib/mandrake || logerr "mkdir"
    logcmd $CP $CARGO_TARGET/release/mandraked $DESTDIR/usr/lib/mandrake/mandraked \
        || logerr "install binary"

    typeset manifests=$MANDRAKE_SOURCE/crates/mandrake-smf/manifests
    logcmd $MKDIR -p $DESTDIR/lib/svc/manifest/system/mandrake $DESTDIR/lib/svc/method \
        || logerr "mkdir smf"
    logcmd $CP $manifests/mandraked.xml $DESTDIR/lib/svc/manifest/system/mandrake/mandraked.xml \
        || logerr "install manifest"
    logcmd $CP $manifests/svc-mandraked $DESTDIR/lib/svc/method/svc-mandraked \
        || logerr "install method"
    logcmd /usr/sbin/svccfg validate $DESTDIR/lib/svc/manifest/system/mandrake/mandraked.xml \
        || logerr "manifest does not validate"

    # RBAC profile the daemon will use through pfexec from Phase 3 (spec §12).
    install_profattr
    install_execattr
    install_userattr
}

init
copy_source
prep_build
build_console
build_daemon
install_daemon
make_package
clean_up
