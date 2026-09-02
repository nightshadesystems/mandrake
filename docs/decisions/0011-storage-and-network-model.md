# ADR-0011: Storage and network drivers: identity, protection, jobs, testing

- **Status:** Accepted
- **Date:** 2026-09-02
- **Phase:** 3 (Storage and network)

## Context

Spec §7 maps every Mandrake object 1:1 onto an illumos object and gives
zones and datasets a stored UUID. Phase 3 adds Crossbow links and IP
addresses, which have no property store, and pools, which on illumos have
no user properties either. It also introduces the first slow operations,
the first shell-outs that mutate the system, and parsers that the spec
wants tested against captured real output while no illumos host is
available to the workstation.

## Decision

**Identity.**

- Datasets and snapshots carry `nightshade.systems:mandrake-id` as a ZFS
  user property, set at creation and assigned on first sight for objects
  created out of band (ADR-0002).
- A pool's id is its root dataset's id.
- Links, addresses, and routes get a deterministic UUID v5 over the host
  id, the object kind, and the object's name (for routes: family,
  destination, and gateway). It survives reboots and needs no table. A
  `dladm rename-link` changes the id; that is documented behaviour, not a
  bug.
- Metadata (display name, description, tags, notes) lives in the SQLite
  `metadata` table keyed by that id for every kind (ADR-0002).

**Protection.** Mirroring the `rpool` rule in spec §7:

- `rpool` refuses destroy and vdev changes.
- Datasets under `rpool/ROOT`, the pool root datasets, and every dataset
  under `rpool/mandrake` refuse destroy and property changes other than
  metadata.
- The address the daemon's HTTPS listener is reachable on, the IP
  interface carrying it, and every link beneath it down to the physical
  port are `protected: true` and refuse delete. The listener address is
  resolved at request time from the bound socket and the address table.

**Privilege.** Reads run as the `mandrake` user. Mutations run through
`pfexec` with the `Mandrake Management` profile from ADR-0010, one tool
invocation per operation, never `sh -c` (ADR-0003).

**Slow operations are jobs.** Scrubs and resilvers are tracked as jobs
that poll `zpool status` until the scan finishes and publish
`job.progress` events. Pool and dataset creation, destruction, snapshot
operations, and every network change are synchronous; they complete in
under a second on the systems Mandrake targets, and their failure modes
are immediate.

**Cache.** List reads are cached for two seconds per command and
invalidated by every mutation the daemon performs. Out-of-band changes
show within that window.

**Routes.** Only static routes are managed, persisted with `route -p`.
Interface and dynamic routes are listed read-only.

**Driver shape and testing.** Each driver exposes a trait of typed
operations with two implementations: the real one shelling out, and an
in-memory fake with the same observable behaviour, so route tests run on
any host. Parsers are pure functions over `&str`. Until real output is
captured, tests use hand-written samples in the documented formats, named
`*.synthetic.txt` under `testdata/`. `build/tools/capture-testdata.sh`
captures the real commands on an OmniOS host with a provenance header;
once a capture is committed its synthetic counterpart is deleted, and a
parser that disagrees with a capture is a bug in the parser.

## Consequences

- The topology view can be drawn from `GET /network/links` alone: every
  link names what it sits over.
- Renaming a link through `dladm` orphans its metadata row; the hourly
  sweep removes it, as for any other vanished id.
- The fake driver is a second implementation to keep honest; its tests
  assert behaviour that the real one is also tested against on illumos
  with `cfg(target_os = "illumos")` integration tests.
- Locking a busy pool during a scrub job is not attempted; concurrent
  API mutations on the same pool are serialised by ZFS itself.
- If a later phase needs pool property changes (autoexpand, ashift on
  add) or vdev attach/replace, they are new operations under
  `/storage/pools/{id}`, not changes to this model.
