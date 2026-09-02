# ADR-0010: Packaging and service layout for the daemon

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 2 (Daemon core), applies to every package after

## Context

Spec §6 fixes the SMF service name and the metadata path; §12 fixes a
dedicated user with RBAC profiles. Left open: the package names, where the
binaries live, how the daemon gets a runtime directory under `/var/run`
without running as root, whether the console is its own package, and how
the packages reach the install media.

Constraints that shaped the choices:

- IPS cannot deliver into `/var/run`; it is a tmpfs created at boot.
- The daemon must not run as root, so it cannot create that directory
  itself, and Rust in this workspace forbids the `unsafe` a privilege drop
  would need.
- The console assets are embedded in the daemon binary (spec §6.1), so a
  separate package would deliver nothing.
- Kayak's `-p profile` mechanism on r151054 reassigns its publisher
  variable to the last profile publisher and then resets *that* publisher
  to the OmniOS origin, corrupting the image's publishers.

## Decision

**Packages** on `nightshade.systems`:

| Package | Delivers |
|---|---|
| `system/mandrake/daemon` | `/usr/lib/mandrake/mandraked`, the SMF bundle, the method script, the `Mandrake Management` RBAC profile, the `mandrake` user and group (uid and gid 63), `/var/mandrake` and `/etc/mandrake/tls` |
| `system/mandrake/cli` | `/usr/bin/mandrakectl` |
| `incorporation/mandrake/mandrake-incorporation` | `type=incorporate` pins on both, install-hold `core-os.mandrake` |

Package version is the workspace version; the FMRI branch is the OmniOS
release of the build host. Built with the omnios-build framework from
recipes under `build/packages/`, with the source copied from this
repository rather than fetched.

**Services.** One bundle, two services. `svc:/system/mandrake/setup` is
transient, runs as root, and creates `/var/run/mandrake` owned by
`mandrake`. `svc:/system/mandrake/mandraked` depends on it and runs with
`method_credential user=mandrake privileges=basic,net_privaddr`, so it can
bind 443 and nothing else beyond a normal user. Configuration is SMF
properties under `config/` (listen, socket, db, tls_dir, log) that the
method script turns into flags; there is no config file. Both services are
enabled in the manifest, so `manifest-import` brings the daemon up on the
first boot of an installed system.

**On the media.** A second overlay hook installs the packages into the
installed-system image with `pkg -R <root> install -g <build repo>`; the
build repository is a temporary origin and is not persisted. The kayak
profile mechanism is not used.

## Consequences

- `pkg verify system/mandrake/daemon` is clean on a running host: the
  daemon writes only under `/var/mandrake`, `/etc/mandrake/tls`, and
  `/var/run/mandrake`, none of them packaged files.
- The RBAC profile grants `pfexec` to the illumos tools from spec §12 now,
  before any driver uses them; it is the security baseline, not a per-phase
  grant.
- A console-only change still means rebuilding and updating the daemon
  package. That is the cost of zero external dependencies for the UI.
- Building the daemon package needs Rust and Node from omnios-extra on the
  build host, and network access for crates.io and the npm registry until
  dependencies are vendored (a later decision).
- Moving the OmniOS pin requires rebuilding the packages on a host of the
  new release; the incorporation makes the old ones uninstallable there.
