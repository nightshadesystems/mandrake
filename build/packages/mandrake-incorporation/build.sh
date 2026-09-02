#!/usr/bin/bash
# shellcheck disable=SC2034,SC2154,SC2086,SC2164 # omnios-build framework: globals, word-split tools, pushd/popd style
#
# incorporation/mandrake/mandrake-incorporation: pins the Mandrake packages
# to one release (ADR-0010). A bare manifest published with the framework's
# publish_manifest, the same way omnios-build publishes `entire`.

# shellcheck source=/dev/null
. ../../omnios-build/lib/build.sh

PROG=mandrake-incorporation
PKG=incorporation/mandrake/mandrake-incorporation
SUMMARY="Mandrake package version incorporation"
DESC="Pins the Mandrake packages to one release so they update together"

: "${MANDRAKE_SOURCE:=$SRCDIR/../../..}"
MANDRAKE_SOURCE=$(cd "$MANDRAKE_SOURCE" && pwd)
VER=$(sed -n 's/^version = "\(.*\)"/\1/p' "$MANDRAKE_SOURCE/Cargo.toml" | head -1)
[ -n "$VER" ] || logerr "cannot read the workspace version from Cargo.toml"

XFORM_ARGS="-DMANDRAKE_VER=$VER"

init
prep_build
publish_manifest $PKG $SRCDIR/incorporation.p5m
clean_up
