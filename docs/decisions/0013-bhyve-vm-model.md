# ADR-0013: bhyve VMs: layout, brand rendering, disks and snapshots, consoles

- **Status:** Accepted
- **Date:** 2026-09-02
- **Phase:** 5 (bhyve)

## Context

Spec §7 defines a VM as a bhyve-brand zone with vcpus, memory, bootrom,
ACPI, ordered zvol-backed disks with a boot flag, image-backed cdroms,
VNICs, a serial console always and VNC bound to loopback behind the
proxy, autoboot, and state. §6.1 puts serial and VNC behind
authenticated WebSockets, and §12 forbids a routable VNC address. Left
open: where disks live and how they are named, how the VM maps onto the
OmniOS bhyve brand's zonecfg, what a VM snapshot is, how the daemon
reaches a VNC socket that root owns inside the zone, and what the
lifecycle verbs mean for a guest.

## Decision

**Layout.** A VM `name` is the bhyve-brand zone `name` with zonepath
`/<pool>/vms/<name>` on dataset `<pool>/vms/<name>`. Its disks are zvols
`<pool>/vms/<name>/disk<N>` numbered from 0 in the order given; the boot
disk is `disk0` unless another carries the flag. A disk is a clone of a
`vm-raw` image's `@image` snapshot, or a blank zvol of the requested size.
Cdroms are `vm-iso` images referenced by their file path. Identity is the
`mandrake-id` zone attribute (ADR-0012); disks and cdroms are addressed by
index, and `mandrake-image` records the boot image.

**Brand rendering.** The VM spec renders to the OmniOS bhyve brand's
zonecfg: `vcpus`, `ram`, `bootrom` (`BHYVE_RELEASE` for `uefi`,
`BHYVE_RELEASE_CSM` for `uefi-csm`), `acpi`, `vnc` (`on` with the socket
inside the zone, or `off`), `bootdisk` and `disk` device resources
matching `/dev/zvol/rdsk/<zvol>`, `cdrom` attributes with the ISO path
and a matching read-only lofs mount, and `anet` NICs as for zones. The
attribute spellings are confirmed against `zonecfg export` of a VM built
by hand on OmniOS r151054, and the renderer lives in one function.

**Snapshots.** A VM snapshot is `zfs snapshot -r <pool>/vms/<name>@<snap>`,
covering every disk at once. Taken while the guest runs it is
crash-consistent, which the API and the console say plainly. Rollback
requires the VM stopped and rolls back every child. The snapshot's id is
the storage snapshot id of the top-level dataset, so it is also visible
under `/storage/snapshots`.

**Lifecycle.** `start` boots; `stop` is `zoneadm shutdown`, which the brand
turns into an ACPI power button, and waits; `stop` with `force` halts;
`restart` is a shutdown with reboot; `reset` is halt then boot. Create,
delete, and each of these are jobs. Delete keeps the VM dataset and its
disks unless `purge`.

**Consoles.** The serial console is `zlogin -C` under a pty, the same
proxy as zones. VNC binds a Unix socket under the zone root that only
root can open; the daemon relays it over a WebSocket as raw RFB bytes for
noVNC by running `pfexec nc -U <socket>` and piping its stdin and stdout,
so the privilege granted is exactly one socket. If the release's brand
can place the socket where the daemon user may open it, the relay
connects directly and `nc` leaves the profile. One VNC and one serial
session per VM at a time.

**Changes to a running VM.** vcpus, memory, bootrom, ACPI, disks, cdroms,
and NICs are written to the zonecfg at once and take effect at the next
boot; the API refuses nothing on that account but reports it. Removing a
disk destroys its zvol only with `purge`.

## Consequences

- Disk clones make a VM from an image as cheap as a zone; a VM's disks
  are ordinary volumes under `/storage/volumes`, protected from destroy
  there while the VM exists.
- Recursive snapshots keep disks consistent with each other, at the cost
  of one snapshot name space per VM.
- The `nc` relay adds a process per VNC session; acceptable for a single
  host, and revisited if the brand offers a socket the daemon may open.
- New console dependency: `@novnc/novnc`.
- Reopen when: r151054's brand names differ from the rendering (the
  renderer changes, the API does not); a guest needs a device the brand
  expresses only through `extra`; or live disk attach becomes available
  and worth exposing.
