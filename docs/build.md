# Building Mandrake

Three environments are involved (spec §5). This document says what runs where.

## Workstation

Where code is written and unit-tested. Any OS with:

- stable Rust via `rustup`; `rust-toolchain.toml` adds the
  `x86_64-unknown-illumos` target automatically
- Node 22 and pnpm 10
- [`just`](https://github.com/casey/just), and `shellcheck` for `just lint`

```sh
just vendor           # initialise the fork submodules (once per clone)
just build-crates     # cargo build --workspace (host target)
just build-console    # pnpm install + build in console/
just lint             # cargo fmt --check, clippy pedantic, eslint, prettier, shellcheck
just test             # cargo test --workspace, host target
just check-illumos    # cargo check --target x86_64-unknown-illumos
just gen-api-docs     # regenerate docs/api.md from api/openapi.yaml
```

The console has its own dev loop against a locally running daemon; see
[console/README.md](../console/README.md). `just build-console` produces
`console/dist`, which `mandraked` embeds at build time, so build the console
before a release build of the daemon.

`check-illumos` type-checks against the illumos `std` without linking. The
crates that compile C (`ring` behind rustls, and SQLite on non-illumos
hosts) still need a C compiler for the target, so the recipe uses `clang`
with the illumos sysroot from `github.com/illumos/sysroot`, fetched on
first use into `~/.cache/mandrake/illumos-sysroot` by
`build/cross/illumos-sysroot.sh`. Install LLVM to get `clang` and
`llvm-ar`; on Windows the recipe also looks in `C:/Program Files/LLVM`.

A full cross-compile to illumos binaries works on Linux with a GCC cross
toolchain: `just toolchain-illumos` builds it once (about half an hour,
into `~/.cache/mandrake/illumos-toolchain`), then `just build-illumos`
produces release binaries under `target/x86_64-unknown-illumos/`. The
build workflow does the same in CI (see Continuous integration).

### Vendored forks

`build/kayak` and `build/omnios-build` are git submodules of the Nightshade
forks, pinned to their `mandrake/r151054` branches (ADR-0004). `just vendor`
initialises them.

On Windows, `omnios-build` needs a sparse checkout because it contains a
directory named `build/XML::Parser`. `vendor.sh` does this for you. For a
standalone clone of the fork on Windows, the same recipe is:

```sh
git clone --no-checkout https://github.com/nightshadesystems/mandrake-build.git
cd mandrake-build
git config core.protectNTFS false
git config core.longpaths true
git sparse-checkout init --no-cone
git sparse-checkout set '/*' '!/build/XML::Parser/'
git checkout mandrake/r151054
```

## Build host

An OmniOS **r151054** install (a VM is fine). Kayak takes the loader, boot
blocks, and `/boot/forth` from the running system, so the host release must
equal the media release; `build-media.sh` refuses otherwise. Media builds
run as root in the global zone, or in a zone with a delegated dataset and
the `fs-allowed=ufs` and lofs settings from `omnios-build/doc/zone-setup.md`.

Packages needed beyond a stock install:

```sh
pkg install developer/build/gnu-make developer/gcc14 media/cdrtools \
    compress/xz file/pv developer/versioning/git
```

`cdrtools` provides `mkisofs`; `pv` and `xz` are used by kayak. `gcc` builds
kayak's small helper binaries. A `PREBUILT_ILLUMOS` workspace is optional but
kayak warns without it.

```sh
just build-iso          # build/out/mandrake-<ver>-r151054.iso
just build-usb          # ...usb (builds the ISO first)
just build-pxe          # ...-pxe.tar.gz
just build-media -n iso # stage overlays and print the kayak command only
just build-media clean  # destroy the kayak dataset and staging directory
just init-repo          # create the empty nightshade.systems repo in build/out/repo
just init-repo -s 10000 # ...and serve it read-only on port 10000
```

Configuration lives in one file, `build/media/mandrake.env`. Environment
overrides: `PKGURL` (OmniOS package source, for a local mirror),
`PREBUILT_ILLUMOS`, `BUILDSEND` (kayak dataset), `STAGE_DIR`.

What the media contain and how Mandrake gets into them is in
[build/media/README.md](../build/media/README.md) and ADR-0005.

### Packages

The Mandrake packages are built with the omnios-build framework from the
recipes in [build/packages/](../build/packages/README.md), as the build
user (omnios-build refuses root). Beyond the media tools above the build
zone needs:

```sh
pkg install developer/omnios-build-tools ooce/developer/rust ooce/runtime/node-22
```

`ooce/*` packages come from the `extra.omnios` publisher, which the OmniOS
installer configures by default. Cargo and pnpm fetch dependencies over
the network during the build.

```sh
just build-packages          # build/out/repo: daemon, cli, incorporation
just build-iso               # now installs those packages into the image
just publish-repo DEST       # pkgrecv into another repository (path or http)
```

`build-media.sh` looks for `system/mandrake/daemon` in `build/out/repo`
(override with `MANDRAKE_BUILD_REPO`). If it is there, a post-overlay hook
installs the packages into the installed-system image and the verification
step checks they landed; if not, the media are built branded-only as in
Phase 1.

Still a placeholder until Phase 6: `just test-boot`, because `mandraked`
runs on the installed system and unattended installs need the kayak answer
file from that phase.

## Test host

bhyve-capable bare metal or a nested-virtualisation VM, used for booting media
and exercising zones and VMs. Each phase ends with a demo here (spec §13).

### Phase 1 demo

1. Boot `build/out/mandrake-<ver>-r151054.iso` under bhyve with a serial
   console. The loader menu shows the Mandrake banner and logo and the
   title "Welcome to the Mandrake installer".
2. Install to a disk with the interactive installer (still OmniOS's
   dialogs in Phase 1) and reboot.
3. Log in on the serial console. The MOTD says Mandrake. Run:

   ```sh
   pkg publisher
   ```

   Both `omnios` and `nightshade.systems` are listed. `cat /etc/release`
   still says OmniOS, by design until Phase 2.

### Phase 2 demo

1. `just build-packages`, then `just build-iso`; install as in Phase 1.
2. On first boot `svcs mandraked` shows `svc:/system/mandrake/mandraked:default`
   online. The serial console and `svcs -L mandraked` show the console URL
   and the certificate fingerprint.
3. Create the first admin over the root socket:

   ```sh
   pfexec mandrakectl users create admin --role admin --password-stdin
   ```

4. Open `https://<host>/` in a browser, accept or pin the certificate, and
   sign in as that admin. Create a second user under System → Users, then
   open System → Audit log: the `user.create` entry names the admin as the
   actor, and the dashboard's recent activity shows it live.
5. From a workstation, `mandrakectl --server https://<host> --fingerprint <fp> --token <t> users list`
   with a token created under the admin; `--json` for scripting.

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

`.github/workflows/ci.yml` is the gate: on every push and pull request an
Ubuntu runner does the Rust format check, clippy with warnings as errors,
build, test, and illumos type-check; console lint, typecheck, and build;
the API docs staleness check; and shellcheck over every script.

`.github/workflows/build.yml` produces artifacts on pushes to `main`, on
`v*` tags, and on demand: the built console, Linux host binaries for smoke
tests, and cross-compiled illumos binaries with the SMF files in a
versioned tarball. The illumos job builds a GCC cross toolchain with
`build/cross/illumos-toolchain.sh` (a port of what Rust's own CI does) the
first time, about half an hour, and restores it from the cache after that.
Those illumos binaries bundle SQLite because the sysroot has none; they are
for smoke testing, not what the ISO ships (ADR-0009).

GitHub has no illumos runner, so packages, media, and integration tests
stay on the build host. A self-hosted runner on that host would let CI run
`just build-packages` and `just build-iso` too.
