#!/bin/bash
#
# kayak post-overlay hook: install the Mandrake packages into the
# installed-system image from the build-time repository.
#
# build-media.sh stages build.env next to this script when the local
# repository has the packages; without it the hook does nothing, which is
# how a Phase 1 style branded-only ISO is still produced. The publisher was
# added by 10-nightshade-publisher.sh; the repository here is a temporary
# origin (`pkg install -g`) and is not persisted into the image.

set -euo pipefail

root=${1:?image root}
here=$(cd "$(dirname "$0")" && pwd)

if [ ! -f "$here/build.env" ]; then
    echo " --- no build repository staged; skipping Mandrake packages"
    exit 0
fi
# shellcheck source=/dev/null
. "$here/build.env"
: "${MANDRAKE_BUILD_REPO:?build.env must set MANDRAKE_BUILD_REPO}"
: "${MANDRAKE_PACKAGES:=system/mandrake/daemon system/mandrake/cli incorporation/mandrake/mandrake-incorporation}"

echo " --- installing Mandrake packages from $MANDRAKE_BUILD_REPO"
# shellcheck disable=SC2086 # the package list is word-split on purpose
pkg -R "$root" install --no-refresh -g "$MANDRAKE_BUILD_REPO" $MANDRAKE_PACKAGES

pkg -R "$root" purge-history
pkg -R "$root" list -H system/mandrake/daemon system/mandrake/cli
