# ADR-0012: Images and zones: storage layout, signed sources, identity, jobs, console

- **Status:** Accepted
- **Date:** 2026-09-02
- **Phase:** 4 (Images and zones)

## Context

Spec §7 defines images as ZFS datasets or zvols under `<pool>/images`,
imported from Ed25519-signed JSON indexes and verified by sha256, with VM
and zone creation being a ZFS clone. Zones are native or lx brand, carry a
`mandrake-id` zone attribute, and are deleted without losing their
datasets unless `purge=true`. Spec §6.1 makes image fetch and long zone
operations background jobs and puts the console behind an authenticated
WebSocket proxy. Left open: the on-disk layout and staging, the index
format and what an unsigned source may do, which pool receives images and
zones when several exist, how zone NICs are expressed, what is a job, and
how a terminal reaches `zlogin` from a daemon that cannot use `unsafe`.

## Decision

**Layout.** Every image gets a Mandrake id at import and lives at
`<pool>/images/<id>`: a filesystem received from a ZFS stream for
`zone-native` and `zone-lx`, a zvol written from a raw disk image for
`vm-raw`, and a plain file `/<pool>/images/iso/<id>.iso` for `vm-iso`.
Datasets and zvols get a snapshot `@image` once complete; zones and VMs
clone that snapshot and never copy bytes. Downloads land in
`/<pool>/images/staging/<id>.part` and are verified against the expected
sha256 while streaming; gzip and xz payloads are decompressed by the
illumos `gzip -dc` and `xz -dc` piped directly into `zfs receive` or the
zvol, as two processes with no shell (ADR-0003). The catalogue is SQLite:
`image_sources`, `image_catalogue` (the last index of each source), and
`images` (imported images with their state, hash, size, pool, dataset,
and origin).

**Sources.** A source is a URL to `index.json` with a detached signature
`index.json.sig`: base64 Ed25519 over the exact bytes of the index. The
index is `{ "name", "generated_at", "images": [ { "name", "version",
"type", "url", "sha256", "size", "description", "os" } ] }`; `url` may be
relative to the index. A source stores its public key; without one it is
*unverified*: its catalogue is shown, but import from it is refused with
422 `unverified-source`. A direct import from a URL with an explicit
sha256 is allowed for operators, who vouch for the hash. The two built-in
sources (OmniOS and nightshade.systems) ship as rows with their URLs and
keys, cannot be deleted, and can be disabled. Keys are set by the
publisher; `mandrake-image-index` (a small tool in the images crate)
generates a keypair and builds and signs an index from a directory of
files, so anyone can host a source.

**Which pool.** Imports and zone creation take `pool`; the default is the
data pool with the most free space, and `rpool` only when it is named or
is the only pool.

**Zones.** A zone's id is the zonecfg attribute `mandrake-id`, set at
create and assigned on first sight for zones created out of band; the
image it came from is `mandrake-image`. Zones exposed here are every brand
except `bhyve`, which is the VM family (Phase 5); native means `ipkg`,
`lipkg`, or `sparse`. The zonepath is `/<pool>/zones/<name>` on dataset
`<pool>/zones/<name>`: a clone of the image for lx and for native zones
given an image, a fresh dataset filled by `zoneadm install` from the
global zone's packages for native zones without one. lx install adopts
the pre-cloned zonepath; the exact `zoneadm install` argument vector is
confirmed on OmniOS r151054 before the driver is final and lives in one
function. Zones are always exclusive-IP; each NIC is a zonecfg `anet`
resource (`linkname`, `lower-link`, `mac-address` when pinned,
`vlan-id`, `allowed-address`, `defrouter`), so the VNIC exists only while
the zone runs and the address is applied by the brand where it can be.
Metadata lives in the `metadata` table keyed by the zone id (ADR-0002).

**Jobs.** Image import, zone create (zonecfg then install), start, stop,
restart, and delete are jobs that publish `job.progress` and, for zones,
`zone.state` events. Reads and zonecfg-only updates are synchronous.
Deleting halts, uninstalls, and removes the configuration; the zone's
datasets stay unless `purge=true`.

**Console.** `GET /zones/{id}/console` upgrades to a WebSocket that
carries the zone console: server frames are terminal output, client text
or binary frames are input, and a client text frame that parses as
`{"resize": {"cols": N, "rows": N}}` resizes the terminal. The daemon runs
`pfexec zlogin -C` under a pseudo-terminal from the `portable-pty` crate,
approved for this purpose, since `zlogin -C` requires a terminal and the
workspace forbids `unsafe`. One console session per zone at a time; a
second connect gets 409 `busy`. Role `operator`.

## Consequences

- Cloning from `@image` keeps zone creation fast and space-free, and
  deleting an image while clones exist is refused by ZFS and surfaced as
  409, so images count their dependents.
- The staging directory can hold partial downloads after a crash; job
  recovery on startup marks such images `failed` and removes the file.
- Unverified sources are useful for testing and private mirrors but need
  an explicit key before they can be imported from; the built-in sources
  are exactly as trustworthy as the keys shipped with the package.
- New dependencies: `ed25519-dalek` for signatures, `portable-pty` for
  the console, and `@xterm/xterm` with `@xterm/addon-fit` in the console.
- Reopen when: `zoneadm install` on the target release cannot adopt a
  pre-cloned zonepath for lx (fall back to installing from the retained
  archive; the API does not change); or when a brand needs a network
  resource `anet` cannot express.
