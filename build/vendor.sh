#!/bin/bash
#
# Initialise the vendored fork submodules (ADR-0004):
#   build/kayak         nightshadesystems/mandrake-kayak
#   build/omnios-build  nightshadesystems/mandrake-build
#
# omnios-build cannot be checked out as-is on Windows: it contains
# build/XML::Parser (a colon is illegal in NTFS names) and a few patch
# files whose paths exceed MAX_PATH. On Windows this script leaves that
# one directory out of the working tree with a sparse checkout; the
# repository itself is complete and the build host is never Windows.

set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
cd "$repo"

case "$(uname -s)" in
    MINGW* | MSYS* | CYGWIN*) windows=1 ;;
    *) windows=0 ;;
esac

git submodule sync --quiet

echo "=== build/kayak"
git submodule update --init build/kayak

echo "=== build/omnios-build"
if ((windows)); then
    # First pass clones and registers the module; the checkout is expected
    # to fail on the colon path. Second pass, after configuring the sparse
    # checkout, succeeds.
    git -c core.protectNTFS=false -c core.longpaths=true \
        submodule update --init build/omnios-build 2>/dev/null || true
    git -C build/omnios-build config core.protectNTFS false
    git -C build/omnios-build config core.longpaths true
    git -C build/omnios-build sparse-checkout init --no-cone
    git -C build/omnios-build sparse-checkout set '/*' '!/build/XML::Parser/'
    git -C build/omnios-build reset --hard --quiet HEAD
    git -c core.protectNTFS=false -c core.longpaths=true \
        submodule update --init build/omnios-build
    echo "(build/XML::Parser left out of the working tree on Windows)"
else
    git submodule update --init build/omnios-build
fi

git submodule status
