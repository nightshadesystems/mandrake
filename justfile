# Mandrake task runner. See docs/build.md for which targets run where.
#
# Workstation targets (any OS): vendor, build-crates, build-console, lint,
# test, check-illumos, ci.
# Build-host targets (OmniOS r151054): build-packages and publish-repo as the
# build user; build-iso, build-usb, build-pxe, build-media, init-repo as root.
# Placeholder until Phase 6: test-boot.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]

illumos_target := "x86_64-unknown-illumos"
console_dir := "console"

# List recipes
default:
    @just --list --unsorted

# ---------------------------------------------------------------- workstation

# Initialise the vendored fork submodules (handles the Windows quirks)
vendor:
    bash build/vendor.sh

# Build every Rust crate for the host target
build-crates:
    cargo build --workspace

# Build the web console to console/dist
build-console:
    cd {{console_dir}} && pnpm install --frozen-lockfile && pnpm build

# Build crates and console
build: build-crates build-console

# Format Rust and console sources in place
fmt:
    cargo fmt --all
    cd {{console_dir}} && pnpm install --frozen-lockfile && pnpm format

# Lint everything; fails on any warning
lint: lint-rust lint-console lint-sh

# rustfmt check and clippy pedantic with warnings denied
lint-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# ESLint, Prettier check, and tsc
lint-console:
    cd {{console_dir}} && pnpm install --frozen-lockfile && pnpm lint && pnpm typecheck

# shellcheck every tracked shell script (skips, loudly, if shellcheck is absent)
lint-sh:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v shellcheck >/dev/null; then
        echo "lint-sh: shellcheck not installed, skipping" >&2
        exit 0
    fi
    git ls-files '*.sh' | xargs shellcheck -S style -x -P SCRIPTDIR
    shellcheck -s sh -S style crates/mandrake-smf/manifests/svc-mandraked

# Run unit tests for the host target; illumos-only tests are skipped here
test:
    cargo test --workspace

# Type-check every crate against the illumos target without linking.
# Needs clang and the illumos sysroot for the crates that compile C; the
# sysroot is fetched on first use (build/cross/illumos-sysroot.sh).
check-illumos:
    #!/usr/bin/env bash
    set -euo pipefail
    if [ -z "${CC_x86_64_unknown_illumos:-}" ]; then
        sysroot=${MANDRAKE_ILLUMOS_SYSROOT:-$HOME/.cache/mandrake/illumos-sysroot}
        bash build/cross/illumos-sysroot.sh "$sysroot" >/dev/null
        if ! command -v clang >/dev/null; then
            if [ -x "/c/Program Files/LLVM/bin/clang.exe" ]; then
                export PATH="/c/Program Files/LLVM/bin:$PATH"
            else
                echo "check-illumos: clang not found; install LLVM (docs/build.md)" >&2
                exit 1
            fi
        fi
        case "$(uname -s)" in MINGW* | MSYS* | CYGWIN*) sysroot=$(cygpath -m "$sysroot") ;; esac
        export CC_x86_64_unknown_illumos="clang --target=x86_64-unknown-illumos --sysroot=$sysroot"
        # GNU ar archives illumos ELF objects fine when llvm-ar is not installed.
        ar=$(command -v llvm-ar || command -v ar) || { echo "check-illumos: no ar found" >&2; exit 1; }
        export AR_x86_64_unknown_illumos="$ar"
    fi
    cargo check --workspace --all-targets --target {{illumos_target}}

# What CI runs
ci: lint build test check-illumos check-api-docs

# Build the illumos cross toolchain on Linux (build/cross/illumos-toolchain.sh)
toolchain-illumos *args:
    bash build/cross/illumos-toolchain.sh all {{args}}

# Cross-compile release binaries for illumos on Linux with that toolchain
build-illumos:
    #!/usr/bin/env bash
    set -euo pipefail
    prefix=${MANDRAKE_ILLUMOS_TOOLCHAIN:-$HOME/.cache/mandrake/illumos-toolchain}
    [ -x "$prefix/bin/x86_64-illumos-gcc" ] || { echo "build-illumos: no toolchain at $prefix; run just toolchain-illumos" >&2; exit 1; }
    export PATH="$prefix/bin:$PATH"
    export CC_x86_64_unknown_illumos=x86_64-illumos-gcc
    export CXX_x86_64_unknown_illumos=x86_64-illumos-g++
    export AR_x86_64_unknown_illumos=x86_64-illumos-ar
    export RANLIB_x86_64_unknown_illumos=x86_64-illumos-ranlib
    export CARGO_TARGET_X86_64_UNKNOWN_ILLUMOS_LINKER=x86_64-illumos-gcc
    cargo build --release --workspace --target {{illumos_target}} --features mandraked/bundled-sqlite

# Regenerate docs/api.md from api/openapi.yaml
gen-api-docs:
    cd {{console_dir}} && pnpm install --frozen-lockfile && pnpm gen-api-docs

# Fail if docs/api.md is out of date with api/openapi.yaml
check-api-docs:
    cd {{console_dir}} && pnpm install --frozen-lockfile && pnpm check-api-docs

# ----------------------------------------------------------------- build host

# Build install media: build-media [-n] [-o DIR] zfs|miniroot|iso|usb|pxe|all|clean
build-media *args:
    bash build/media/build-media.sh {{args}}

# Assemble the BIOS+UEFI hybrid ISO into build/out
build-iso:
    bash build/media/build-media.sh iso

# Assemble the dd-able USB image into build/out (builds the ISO first)
build-usb:
    bash build/media/build-media.sh usb

# Assemble the PXE tarball into build/out
build-pxe:
    bash build/media/build-media.sh pxe

# Create the empty nightshade.systems IPS repository: init-repo [-s PORT] [DIR]
init-repo *args:
    bash build/media/init-repo.sh {{args}}

# Build IPS packages into build/out/repo: build-packages [-r DIR] [PACKAGE...]
build-packages *args:
    bash build/packages/build-packages.sh {{args}}

# Publish the build repository elsewhere: publish-repo [-s SRC] DEST
publish-repo *args:
    bash build/media/publish-repo.sh {{args}}

# Boot the ISO under bhyve, wait for mandraked, run API smoke tests.
# Needs the unattended installer (Phase 6): mandraked runs on the
# installed system, not on the live media.
test-boot: (not-yet "test-boot" "6")

[private]
not-yet target phase:
    @echo "just {{target}}: not implemented until Phase {{phase}}. See docs/build.md." >&2
    @exit 1
