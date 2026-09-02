#!/bin/bash
#
# Create the nightshade.systems IPS repository, empty, and optionally serve
# it read-only for a demo or a build. Runs on the OmniOS build host.
#
#   init-repo.sh [-s PORT] [REPO_DIR]
#
#   REPO_DIR   file repository to create or reuse (default build/out/repo)
#   -s PORT    after creating, serve REPO_DIR with pkg.depotd on PORT
#              in the foreground until interrupted
#
# Packages are published into this repository from Phase 2 on. In Phase 1
# it exists so the publisher is real and `pkg refresh` against it works.

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

# shellcheck source=mandrake.env
. "$here/mandrake.env"

port=
while getopts 's:h' opt; do
    case $opt in
        s) port=$OPTARG ;;
        h) sed -n '3,14p' "$0"; exit 0 ;;
        *) sed -n '3,14p' "$0"; exit 2 ;;
    esac
done
shift $((OPTIND - 1))
repo_dir=${1:-$repo/build/out/repo}

die() { echo "init-repo: $*" >&2; exit 1; }

command -v pkgrepo >/dev/null || die "pkgrepo not found; run this on the OmniOS build host"

if [ ! -f "$repo_dir/pkg5.repository" ]; then
    echo "=== creating repository at $repo_dir"
    pkgrepo create "$repo_dir"
fi

if ! pkgrepo list -s "$repo_dir" -H -p "$MANDRAKE_PUBLISHER" >/dev/null 2>&1 \
    && ! pkgrepo get -s "$repo_dir" -H -p "$MANDRAKE_PUBLISHER" >/dev/null 2>&1; then
    echo "=== adding publisher $MANDRAKE_PUBLISHER"
    pkgrepo add-publisher -s "$repo_dir" "$MANDRAKE_PUBLISHER"
fi

pkgrepo set -s "$repo_dir" publisher/prefix="$MANDRAKE_PUBLISHER"
pkgrepo info -s "$repo_dir"

if [ -n "$port" ]; then
    echo "=== serving $repo_dir on port $port (Ctrl-C to stop)"
    echo "    point MANDRAKE_PUBLISHER_URL at http://<this host>:$port/ to refresh from it"
    exec /usr/lib/pkg.depotd -d "$repo_dir" -p "$port" --readonly
fi
