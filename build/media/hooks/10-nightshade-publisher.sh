#!/bin/bash
#
# kayak post-overlay hook: add the nightshade.systems publisher to the
# installed-system image so a fresh install lists both publishers.
#
# apply_custom_overlay runs this as `bash <hook> <image-root>` after kayak
# has configured the omnios publisher and before the ZFS stream is taken.
# The hook directory is deleted afterwards. Configuration comes from the
# copy of mandrake.env staged alongside this script by build-media.sh.

set -euo pipefail

root=${1:?image root}
here=$(cd "$(dirname "$0")" && pwd)

# shellcheck source=../mandrake.env
. "$here/mandrake.env"

echo " --- adding publisher $MANDRAKE_PUBLISHER ($MANDRAKE_PUBLISHER_URL)"

# --no-refresh: the origin need not be reachable at build time.
pkg -R "$root" set-publisher --no-refresh \
    -g "$MANDRAKE_PUBLISHER_URL" "$MANDRAKE_PUBLISHER"

# Packages are not signed yet (ADR-0006); verify signatures when present.
pkg -R "$root" set-publisher --no-refresh \
    --set-property signature-policy=verify "$MANDRAKE_PUBLISHER"

# kayak purged history before the overlay ran; keep the image clean.
pkg -R "$root" purge-history

pkg -R "$root" publisher
