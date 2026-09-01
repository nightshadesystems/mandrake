# Mandrake — OmniOS-based Hypervisor OS

**Org:** Nightshade Systems
**Repo:** `github.com/nightshadesystems/mandrake`
**Status:** Kickoff spec, v0.2 — 2026-09-01

Mandrake is an illumos-based hypervisor operating system derived from OmniOS CE.
It runs virtual machines under bhyve and containers as illumos zones (native and
lx), on a native ZFS root with boot environments. It is managed primarily through
a web console backed by a single Rust daemon exposing an HTTP API. A thin CLI
exists for scripting and recovery.

Mandrake is a hypervisor appliance, not a network operating system. There is no
configuration shell, no candidate/commit model, and no config-file DSL. The
illumos system state (zones, datasets, links) is the source of truth for
infrastructure objects; Mandrake adds an API, a console, and the metadata
illumos doesn't hold.

---

## 0. Git rules — read first

- **Claude Code never runs `git commit`, `git push`, `git tag`, or anything
  that writes history. Never. Not with `--author`, not with a Cody identity, not
  "just this once."**
- At each natural commit point, stop and output exactly one plain single-line
  commit message with no prefix, no type tag, no trailing punctuation. Then wait
  for Cody to commit and say "continue".
- Staging with `git add` is fine. Reading history is fine.

## 1. Goals

- Bootable, installable media (ISO, USB, PXE) built from a pinned OmniOS release
  plus a `nightshade.systems` IPS publisher overlay.
- Persistent ZFS-root install with `beadm` boot environments preserved. Upgrades
  are new BEs; rollback is a reboot.
- `mandraked`: one daemon owning host management — networking (Crossbow),
  storage (ZFS), zones, bhyve VMs, images, users, audit — behind an HTTP+JSON API
  with WebSocket for events and consoles.
- `mandrake-console`: a web UI served by `mandraked`, built to the Nightshade
  Systems design system. This is the primary management surface.
- `mandrakectl`: thin CLI over the same API. JSON-first. Recovery and scripting,
  not a config shell.
- Everything reachable from a serial console for bring-up and recovery.

## 2. Non-goals (v1)

- Clustering, live migration, shared-storage HA, multi-host anything. Single host.
  Keep object IDs globally unique (UUIDs) so a fleet layer can be added later
  without a migration, but build none of it.
- Forking `illumos-omnios` gate. Consume OmniOS kernel and core packages as-is.
- SPARC. x86-64 only.
- Declarative config files or GitOps-style reconciliation. Imperative API.
- Nested virtualization beyond what bhyve provides.

## 3. Upstream base

| Component | Source | Pin |
|---|---|---|
| OS / kernel / core packages | OmniOS CE `omnios` publisher | r151054 LTS (re-pin at kickoff to current LTS) |
| Build system | `github.com/omniosorg/omnios-build` | fork → `nightshadesystems/mandrake-build` |
| Installer / media | `github.com/omniosorg/kayak` | fork → `nightshadesystems/mandrake-kayak` |
| Zone tooling reference | `github.com/omniosorg/zadm` | reference only, not shipped |
| Hypervisor | bhyve (in OmniOS) | as shipped in pinned release |

**Overlay, don't rebuild.** Mandrake ships an IPS publisher `nightshade.systems`
layered on top of `omnios`. Mandrake packages (`mandraked`, `mandrakectl`,
console assets, branding, curated incorporation) live there. Core OS packages
come from OmniOS unmodified.

## 4. Repository layout

Monorepo. Rust workspace plus a TypeScript console.

```
mandrake/
├── Cargo.toml                  # workspace
├── docs/
│   ├── spec.md                 # this document
│   ├── decisions/              # ADRs, numbered
│   ├── api.md                  # generated from OpenAPI, committed
│   └── build.md                # producing media
├── crates/
│   ├── mandraked/              # daemon: API server, auth, audit, static console
│   ├── mandrakectl/            # thin CLI
│   ├── mandrake-core/          # shared types, IDs, errors
│   ├── mandrake-zones/         # zonecfg/zoneadm driver
│   ├── mandrake-bhyve/         # bhyve brand driver, VM lifecycle, console proxy
│   ├── mandrake-net/           # dladm / ipadm / Crossbow driver
│   ├── mandrake-zfs/           # zfs/zpool/beadm driver
│   ├── mandrake-images/        # catalogue, fetch, verify, import
│   └── mandrake-smf/           # SMF manifests and service control
├── console/                    # web console
│   ├── design/                 # design system export: tokens, primitives
│   ├── src/
│   └── package.json
├── api/
│   └── openapi.yaml            # API contract, source of truth
├── build/
│   ├── omnios-build/           # submodule/subtree of fork
│   ├── kayak/                  # submodule/subtree of fork
│   ├── packages/               # build.sh per Mandrake package
│   ├── manifests/              # IPS manifests, incorporation
│   └── media/                  # ISO/USB/PXE assembly
├── branding/                   # loader banner, MOTD, /etc/release
├── overlay/                    # files dropped onto the image root
└── justfile
```

## 5. Build and development environment

- **Workstation:** Claude Code runs here. Rust targets `x86_64-unknown-illumos`
  (Tier 2 with host tools). Cross-compile from Linux with an illumos sysroot for
  fast iteration; native builds in the build zone for release. Console builds
  anywhere Node runs.
- **Build host:** OmniOS LTS (VM is fine) with a dedicated build zone. All
  `omnios-build` and `kayak` invocations run here. Claude Code writes the
  scripts; Cody runs them.
- **Test host:** bhyve-capable bare metal or nested-virt VM for booting media and
  exercising zones/VMs.

`justfile` targets at minimum: `build-crates`, `build-console`,
`build-packages`, `publish-repo`, `build-iso`, `build-usb`, `build-pxe`,
`test-boot` (boots ISO under bhyve on the build host, waits for `mandraked`
online, runs API smoke tests).

## 6. `mandraked`

SMF service `svc:/system/mandrake/mandraked:default`. Single process, `tokio`,
`axum`.

### 6.1 Responsibilities

- HTTP+JSON API per `api/openapi.yaml`. WebSocket endpoints for the event stream
  and for VM serial/VNC console proxying.
- Serves `mandrake-console` static assets, embedded in the binary via
  `rust-embed`, so the appliance has zero external dependencies for its UI.
- Auth: local users with argon2 password hashes, session cookies for the console,
  bearer tokens for API/CLI. Roles: `admin`, `operator`, `viewer`.
- Metadata store: SQLite (`rusqlite`, WAL mode) at
  `/var/mandrake/mandrake.db` on `rpool/mandrake/var`. Holds only what illumos
  doesn't: users, tokens, sessions, audit log, image catalogue, and per-object
  metadata (display name, description, tags, notes). Infrastructure objects are
  read from illumos on demand and cached with short TTL, invalidated on write.
- Audit: every mutating call logged with actor, object, before/after summary,
  result. Exposed via API and console.
- Background workers: image fetch/verify/import, snapshot schedules, long-running
  zone/VM operations with job IDs and progress over WebSocket.

### 6.2 Drivers

All drivers shell out to illumos native tooling — `zonecfg`, `zoneadm`, `dladm`,
`ipadm`, `zfs`, `zpool`, `beadm`. Parse `-p` parsable output where it exists.
Direct FFI is a later optimisation. Each driver exposes typed operations, not a
generic reconcile loop.

### 6.3 API shape

REST, plural nouns, UUID ids, cursor pagination, `Idempotency-Key` on POST.
Long operations return `202` with a job resource. Errors are RFC 7807 problem
details. The OpenAPI file is the contract; `docs/api.md` is generated from it
and committed so reviewers can read it without tooling.

Resource families: `system`, `network/{links,vnics,aggrs,vlans,etherstubs,
addresses,routes}`, `storage/{pools,datasets,volumes,snapshots}`,
`images`, `zones`, `vms`, `jobs`, `events`, `users`, `tokens`, `audit`.

## 7. Resource model

Illumos is the source of truth. Mandrake objects map 1:1 onto illumos objects
and carry a UUID stored as a zone attribute (`mandrake-id`) or ZFS user
property (`nightshade.systems:mandrake-id`) so the mapping survives reboots
and out-of-band changes.

**VM** — a bhyve-brand zone. Fields: `id`, `name`, `vcpus`, `memory`, `bootrom`
(`uefi` | `uefi-csm`), `acpi`, `disks[]` (zvol-backed, ordered, boot flag),
`cdroms[]` (image-backed), `nics[]` (VNIC over a link, MAC auto or pinned,
optional VLAN), `console` (serial always; VNC bind loopback, exposed via proxy),
`autoboot`, `state`. Rendered to `zonecfg` with the OmniOS bhyve brand attrs.

**Zone** — native or lx brand. Fields: `id`, `name`, `brand`, `image`, `dataset`,
`nics[]`, `cpu_cap`, `memory_cap`, `autoboot`, `state`.

**Image** — a ZFS dataset (zone images) or zvol (VM images) under `<pool>/images`,
imported from a source, verified by sha256. VM and zone creation is a ZFS clone;
nothing copies bytes twice. Types: `zone-native`, `zone-lx`, `vm-raw`, `vm-iso`.

**Image source** — a URL to an Ed25519-signed JSON index
`{name, version, type, url, sha256, size}`. Ships with an OmniOS source and a
`nightshade.systems` source; users can add their own.

**Network** — Crossbow objects surfaced directly: physical links, aggrs (LACP),
VLANs, etherstubs, VNICs, addresses, routes. No abstraction layer over them.

**Storage** — pools, datasets, volumes, snapshots. `rpool` is observed and
protected: no destroy, no vdev changes via API. Data pools are fully managed.

Destructive rules: deleting a VM or zone halts and uninstalls it but keeps its
datasets unless `purge=true`. Pool destroy requires the pool name echoed back in
the request body. Snapshots are never auto-deleted outside a schedule the user
created.

## 8. `mandrake-console`

The primary management surface. Vite + React + TypeScript, single-page, built to
static assets and embedded in `mandraked`.

**Design system:** the Nightshade Systems design system at
`https://claude.ai/design/p/3f222e10-15be-474b-803d-a8e39c46eb86` is the source of
truth for tokens, type, colour, spacing, and component primitives. Export it
into `console/design/` (tokens as CSS variables + JSON, primitives as
components) before any page is built. Nothing in the console uses a colour,
font, or radius that isn't defined there.

Pages, in build order:
1. Login, session, user menu
2. Dashboard — host summary, resource gauges, recent events, running VMs/zones
3. VMs — list, detail, create wizard (image → sizing → disks → networking →
   review), lifecycle actions, serial console (xterm.js over WS), VNC (noVNC
   over WS proxy), snapshots
4. Zones — same shape as VMs minus VNC
5. Images — catalogue, sources, import progress
6. Network — link topology view, VNIC/aggr/VLAN/etherstub CRUD
7. Storage — pools with vdev layout and health, datasets, volumes, snapshots
8. System — hostname/time/NTP, users and tokens, boot environments (create,
   activate, rollback), updates, audit log

Console talks only to the public API. If the console needs something the API
doesn't expose, the API gets extended first and the OpenAPI file updated.

## 9. `mandrakectl`

Thin. Every command maps to one API call. `--json` by default when stdout isn't a
TTY, human tables otherwise. Uses a token from `~/.config/mandrake/token` or
`MANDRAKE_TOKEN`. Works over the Unix socket without auth when run as root on the
host, so recovery is possible with no network and no console.

## 10. Installer and media

Kayak fork. Interactive path plus answer-file for PXE.

1. Loader banner with Mandrake branding.
2. Disk detection; single-disk or mirrored `rpool`.
3. Install from local IPS repo on media — `nightshade.systems` plus pinned
   `omnios` subset. No network required.
4. Initial BE `mandrake-<version>`.
5. Prompt hostname, management NIC, address/DHCP, gateway, DNS, admin username
   and password. Write to `mandraked`'s SQLite on first boot.
6. Generate a self-signed TLS cert for the console. Print the console URL and
   fingerprint on the serial console and MOTD.
7. Enable `mandraked`.

Media: `.iso` (BIOS+UEFI hybrid), `.usb` dd image, PXE tarball with `unix` +
`miniroot` + answer-file.

## 11. Branding

Loader banner, MOTD, `/etc/release`, default prompt, console title all say
Mandrake / Nightshade Systems. OmniOS CE remains credited in `/etc/release` and
`docs/`; no attribution or copyright files are removed.

## 12. Security baseline

- `mandraked` runs as a dedicated user with RBAC profiles for `zonecfg`,
  `zoneadm`, `dladm`, `ipadm`, `zfs`, `zpool`, `beadm`. Root only where illumos
  genuinely requires it, via a narrowly scoped `pfexec` profile.
- Console and API on HTTPS only. Self-signed at install; custom cert uploadable
  via System page.
- SSH keys only; root SSH disabled; admin user with `pfexec` Primary
  Administrator.
- IP Filter default: SSH and HTTPS on the management VNIC only; deny all other
  inbound to the global zone.
- VNC and serial consoles never bind a routable address. Console access is via
  the authenticated WebSocket proxy only.
- Rate limiting and lockout on login. Tokens are hashed at rest.

## 13. Phases

Each phase ends with a demo on the test host. Console pages ship in the same
phase as the API they depend on.

**Phase 0 — Scaffold.** Workspace, crate and console skeletons, `docs/`,
ADR-0001 (overlay not fork), ADR-0002 (illumos as source of truth + SQLite
metadata), ADR-0003 (shell-out drivers), `api/openapi.yaml` stub, `justfile`,
CI stub (lint + build).

**Phase 1 — Media.** Vendored forks. Bootable ISO = stock OmniOS LTS + Mandrake
branding + empty publisher. Demo: boots, shows Mandrake banner, both publishers
listed.

**Phase 2 — Daemon core + console shell.** `mandraked` as SMF service, TLS,
auth, users/tokens, audit, `system` resources, event WebSocket. Console: design
system exported, login, dashboard shell, System → users. `mandrakectl` with
`system` and `users` commands. Packaged and on the ISO. Demo: install, log into
the console over HTTPS, create a user, see it in the audit log.

**Phase 3 — Storage and network.** `mandrake-zfs`, `mandrake-net`. Console:
Storage and Network pages. Demo: create a mirrored data pool and an aggr → VLAN
→ etherstub → VNIC chain from the console; `zpool status` and `dladm show-vnic`
agree.

**Phase 4 — Images and zones.** `mandrake-images`, `mandrake-zones`. Console:
Images and Zones pages. Demo: add a source, import an lx image, create a zone
with a VNIC on the etherstub, open its console in the browser.

**Phase 5 — bhyve.** `mandrake-bhyve`, console proxy for serial and VNC.
Console: VMs page with create wizard and noVNC. Demo: install a Linux guest from
an ISO image entirely in the browser.

**Phase 6 — Installer.** Kayak customisation per §10. Demo: unattended PXE
install to a blank box that comes up with the console reachable.

**Phase 7 — Boot environments and updates.** `beadm` integration, update flow
(new BE, `pkg update` into it, activate, reboot, rollback from console). Demo:
upgrade a running host from the console and roll it back.

## 14. Working agreement for Claude Code

- §0 git rules are absolute.
- Read this spec and `api/openapi.yaml` before writing code. The OpenAPI file
  wins on API shape; this spec wins on behaviour. Flag disagreements.
- One phase at a time. No early work on later-phase crates or pages.
- Decisions not covered here become ADRs in `docs/decisions/` before dependent
  code.
- Rust: `edition = "2024"`, `clippy::pedantic` clean, `thiserror`, `tracing`,
  `tokio`, `axum`, `rusqlite`, `rust-embed`. No `unwrap()` outside tests.
- Console: TypeScript strict, ESLint + Prettier, no component library beyond
  what `console/design/` defines. Generate the API client from
  `api/openapi.yaml`; never hand-write fetch calls.
- Everything that shells out gets an illumos-only integration test, skipped
  elsewhere with a clear marker. Unit-test parsers against captured real output
  in `crates/*/testdata/`.
- Prefer OmniOS conventions over SmartOS ones; note SmartOS behaviour in
  comments where relevant.
- Ask before adding a dependency outside the sets above.
