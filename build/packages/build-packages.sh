#!/bin/bash
#
# Build the Mandrake IPS packages with the omnios-build framework and
# publish them into the local nightshade.systems file repository.
#
#   build-packages.sh [-r REPO_DIR] [PACKAGE...]
#
#   REPO_DIR   file repository (default build/out/repo; created if needed)
#   PACKAGE    directories under build/packages to build; default: all,
#              in dependency order (daemon, cli, incorporation)
#
# Runs on the OmniOS build host as the build user, not root (omnios-build
# refuses root). Needs developer/omnios-build-tools, ooce/developer/rust,
# and ooce/runtime/node-22; see docs/build.md.

set -euo pipefail

here=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$here/../.." && pwd)
ob=$repo/build/omnios-build
repo_dir=$repo/build/out/repo

die() { echo "build-packages: $*" >&2; exit 1; }
log() { printf '\n=== %s\n' "$*"; }

while getopts 'r:h' opt; do
    case $opt in
        r) repo_dir=$OPTARG ;;
        h) sed -n '3,15p' "$0"; exit 0 ;;
        *) sed -n '3,15p' "$0"; exit 2 ;;
    esac
done
shift $((OPTIND - 1))

[ "$(uname -s)" = SunOS ] || die "run this on the OmniOS build host"
[ "$(id -u)" != 0 ] || die "omnios-build must not run as root"
[ -f "$ob/lib/build.sh" ] || die "omnios-build submodule missing; run 'just vendor'"
for t in gmake pkgsend pkgrepo /opt/ooce/bin/cargo /opt/ooce/bin/node; do
    command -v "$t" >/dev/null || [ -x "$t" ] || die "missing tool: $t"
done

log "site configuration"
cp "$repo/build/site.sh" "$ob/lib/site.sh"
export MANDRAKE_REPO_DIR=$repo_dir
export MANDRAKE_SOURCE=$repo
if [ ! -f "$repo_dir/pkg5.repository" ]; then
    bash "$repo/build/media/init-repo.sh" "$repo_dir"
fi

packages=("$@")
if [ ${#packages[@]} -eq 0 ]; then
    packages=(mandraked mandrakectl mandrake-incorporation)
fi

for p in "${packages[@]}"; do
    dir=$here/$p
    [ -x "$dir/build.sh" ] || die "no build.sh in build/packages/$p"
    log "building $p"
    (cd "$dir" && ./build.sh -b -r "file://$repo_dir/")
done

log "rebuilding repository index"
pkgrepo -s "$repo_dir" rebuild
pkgrepo -s "$repo_dir" list -H | awk '{ print $2 "@" $3 }'
log "done: $repo_dir"
