#!/bin/bash
#
# Publish the packages from the local build repository to another IPS
# repository: a file path (created if needed) or an HTTP repository that
# accepts pkgsend (a pkg.depotd started with --writable-root).
#
#   publish-repo.sh [-s SOURCE_DIR] DEST
#
#   SOURCE_DIR  local build repository (default build/out/repo)
#   DEST        /path/to/repo, file:///path, or http://host:port/
#
# Packages are unsigned (ADR-0006); signing gets its own ADR before the
# origin in mandrake.env is served publicly.

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)

# shellcheck source=mandrake.env
. "$here/mandrake.env"

source_dir=$repo/build/out/repo
while getopts 's:h' opt; do
    case $opt in
        s) source_dir=$OPTARG ;;
        h) sed -n '3,13p' "$0"; exit 0 ;;
        *) sed -n '3,13p' "$0"; exit 2 ;;
    esac
done
shift $((OPTIND - 1))
dest=${1:?destination repository}

die() { echo "publish-repo: $*" >&2; exit 1; }

command -v pkgrecv >/dev/null || die "pkgrecv not found; run this on the OmniOS build host"
[ -f "$source_dir/pkg5.repository" ] || die "no repository at $source_dir; run build-packages first"

case $dest in
    http://* | https://*) ;;
    file://*) dest=${dest#file://} ;;
esac
if [ "${dest#http}" = "$dest" ] && [ ! -f "$dest/pkg5.repository" ]; then
    echo "=== creating repository at $dest"
    pkgrepo create "$dest"
    pkgrepo add-publisher -s "$dest" "$MANDRAKE_PUBLISHER"
    pkgrepo set -s "$dest" publisher/prefix="$MANDRAKE_PUBLISHER"
fi

echo "=== publishing $MANDRAKE_PUBLISHER packages from $source_dir to $dest"
pkgrecv -s "$source_dir" -d "$dest" -m latest --clone -p "$MANDRAKE_PUBLISHER" 2>/dev/null \
    || pkgrecv -s "$source_dir" -d "$dest" -m latest '*'
if [ "${dest#http}" = "$dest" ]; then
    pkgrepo -s "$dest" rebuild
    pkgrepo -s "$dest" list -H | awk '{ print $2 "@" $3 }'
fi
echo "=== done"
