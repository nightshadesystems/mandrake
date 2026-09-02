# Building Mandrake

Three environments are involved (spec §5). This document says what runs where.

## Workstation

Where code is written and unit-tested. Any OS with:

- stable Rust via `rustup`; `rust-toolchain.toml` adds the
  `x86_64-unknown-illumos` target automatically
- Node 22 and pnpm 10
- [`just`](https://github.com/casey/just)

```sh
just build-crates     # cargo build --workspace (host target)
just build-console    # pnpm install + build in console/
just lint             # cargo fmt --check, clippy pedantic, eslint, prettier
just test             # cargo test --workspace, host target
just check-illumos    # cargo check --target x86_64-unknown-illumos
```

`check-illumos` type-checks against the illumos `std` without linking, so it
works with no sysroot. A full cross-build with an illumos sysroot for fast
iteration is set up when the first driver crate needs it.

## Build host

An OmniOS LTS install (a VM is fine) with a dedicated build zone. Every
`omnios-build` and `kayak` invocation runs here. Claude Code writes the
scripts; Cody runs them.

```sh
just build-packages   # build.sh per package under build/packages/
just publish-repo     # publish to the nightshade.systems IPS repo
just build-iso        # BIOS+UEFI hybrid ISO
just build-usb        # dd image
just build-pxe        # unix + miniroot + answer-file tarball
just test-boot        # boot the ISO under bhyve, wait for mandraked, smoke test
```

These targets are placeholders until Phase 1 and exit with a message saying so.

## Test host

bhyve-capable bare metal or a nested-virtualisation VM, used for booting media
and exercising zones and VMs. Each phase ends with a demo here (spec §13).

## Test conventions

- Parsers are unit-tested against captured real output in
  `crates/<driver>/testdata/`. See the README in each directory.
- Anything that invokes illumos tooling is an integration test annotated:

  ```rust
  #[test]
  #[cfg_attr(not(target_os = "illumos"), ignore = "requires illumos")]
  fn creates_a_vnic() { /* ... */ }
  ```

  On other hosts it shows as `ignored` in `cargo test` output with that reason.

## Continuous integration

`.github/workflows/ci.yml` runs on every push and pull request on an Ubuntu
runner: format check, clippy with warnings as errors, build, test, illumos
type-check, then console lint and build. GitHub has no illumos runner, so
integration tests and media builds are not in CI; they run on the build host.
