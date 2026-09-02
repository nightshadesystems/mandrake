# Mandrake task runner. See docs/build.md for which targets run where.
#
# Workstation targets (any OS): build-crates, build-console, lint, test,
# check-illumos, ci.
# Build-host targets (OmniOS build zone): build-packages, publish-repo,
# build-iso, build-usb, build-pxe, test-boot. Placeholders until Phase 1.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]
set windows-shell := ["bash", "-eu", "-o", "pipefail", "-c"]

illumos_target := "x86_64-unknown-illumos"
console_dir := "console"

# List recipes
default:
    @just --list --unsorted

# ---------------------------------------------------------------- workstation

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
lint: lint-rust lint-console

# rustfmt check and clippy pedantic with warnings denied
lint-rust:
    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings

# ESLint, Prettier check, and tsc
lint-console:
    cd {{console_dir}} && pnpm install --frozen-lockfile && pnpm lint && pnpm typecheck

# Run unit tests for the host target; illumos-only tests are skipped here
test:
    cargo test --workspace

# Type-check every crate against the illumos target without linking
check-illumos:
    cargo check --workspace --all-targets --target {{illumos_target}}

# What CI runs
ci: lint build test check-illumos

# Regenerate docs/api.md from api/openapi.yaml
gen-api-docs: (not-yet "gen-api-docs" "2")

# ----------------------------------------------------------------- build host

# Build IPS packages from build/packages/*/build.sh
build-packages: (not-yet "build-packages" "1")

# Publish built packages to the nightshade.systems IPS repository
publish-repo: (not-yet "publish-repo" "1")

# Assemble the BIOS+UEFI hybrid ISO
build-iso: (not-yet "build-iso" "1")

# Assemble the dd-able USB image
build-usb: (not-yet "build-usb" "1")

# Assemble the PXE tarball: unix, miniroot, answer-file
build-pxe: (not-yet "build-pxe" "1")

# Boot the ISO under bhyve, wait for mandraked, run API smoke tests
test-boot: (not-yet "test-boot" "2")

[private]
not-yet target phase:
    @echo "just {{target}}: not implemented until Phase {{phase}}. See docs/build.md." >&2
    @exit 1
