# ADR-0002: Illumos is the source of truth; SQLite holds only metadata

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 2 (Daemon core), applies to every driver phase after

## Context

`mandraked` manages zones, bhyve VMs, Crossbow links, ZFS datasets, and boot
environments. Each of those already has an authoritative store inside illumos:
`zonecfg` XML, the dladm and ipadm persistent configuration, ZFS pool metadata,
and the BE list. Operators will also change things out of band, from a serial
console or an SSH session, during recovery.

Options considered for the daemon's own state:

1. **Own database as the source of truth**, reconciled to illumos by a control
   loop. This is the SmartOS `vmadm` and network-OS model. It duplicates state,
   needs a reconciler, and drifts the moment someone runs `zoneadm` by hand.
2. **Illumos as the source of truth**, with the daemon reading live system state
   on demand and storing only what illumos has no place for.

Mandrake is explicitly not a network OS and has no commit model (spec preamble,
§2). Option 1 would pull one in through the back door.

## Decision

Illumos system state is the source of truth for every infrastructure object.
`mandraked` reads zones, links, datasets, and BEs from the system on demand,
caches results with a short TTL, and invalidates the cache on every write it
performs.

Each Mandrake object is mapped 1:1 onto an illumos object and carries a UUID
stored in illumos so the mapping survives reboots and out-of-band changes:

- zones and VMs: a zone attribute named `mandrake-id`
- datasets, volumes, and snapshots: the ZFS user property
  `nightshade.systems:mandrake-id`

A single SQLite database (`rusqlite`, WAL mode) at `/var/mandrake/mandrake.db`
on `rpool/mandrake/var` holds only what illumos cannot:

- users, password hashes, sessions, API tokens
- the audit log
- the image catalogue and image sources
- per-object metadata: display name, description, tags, notes, keyed by UUID

If an illumos object appears without a `mandrake-id`, the daemon assigns one on
first sight. If the SQLite metadata row for an object is missing, the object is
still fully valid; it simply has no description or tags.

## Consequences

- No reconcile loop, no drift detection, no pending state. What `zoneadm list`
  says is what the API says.
- Out-of-band changes are supported by design, which is what a recovery-first
  appliance needs.
- Every read is a shell-out (see ADR-0003) and so is slower than a database
  read. The short-TTL cache bounds this; listing pages must tolerate a
  few-hundred-millisecond first load.
- Deleting an object in illumos orphans its metadata row. A periodic sweep
  removes rows whose UUID no longer exists on the system; it never deletes
  anything in illumos.
- Multi-host is out of scope (spec §2), but because IDs are UUIDs held in
  illumos, a later fleet layer can index hosts without migrating this schema.
- Reopen this decision if a resource turns up that illumos cannot hold enough
  of to reconstruct the API view. None has so far.
