# ADR-0014: Installer answers, first boot, and the management path

- **Status:** Accepted
- **Date:** 2026-09-03
- **Phase:** 6 (Installer)

## Context

Spec §10 describes an interactive installer and an answer file for PXE:
disks, hostname, management NIC, address, gateway, DNS, an admin username
and password written to `mandraked`'s SQLite on first boot, a self-signed
certificate whose fingerprint is printed on the serial console and in the
MOTD, and the initial boot environment named `mandrake-<version>`. §12
adds SSH keys only, no root SSH, an admin OS user with the Primary
Administrator profile, and IP Filter allowing SSH and HTTPS on the
management VNIC only.

Phases 1 and 2 left kayak unmodified on the fork's `mandrake/r151054`
branch, with Mandrake reaching the media only through overlays and hooks
(ADR-0005) and the packages installed into the image by a hook
(ADR-0010). Kayak already has an answer-file mechanism: a shell script
fetched by MAC address from `http://<server>/kayak/<MAC>` and sourced by
the installer, whose verbs (`BuildRpool`, `SetHostname`, `UseDNS`,
`SetTimezone`, `SetRootPW`, `Postboot`) are functions in
`/kayak/lib/*.sh`. Its interactive path ends in a configuration menu
covering hostname, timezone, networking, root password, and an OS user.

Left open: how Mandrake's answers ride on kayak's format, how the admin
reaches the daemon's database when the installer runs in a ramdisk that
has neither the daemon nor its password hashing, where the management
address lives, how the fingerprint reaches the console and MOTD from a
daemon that cannot write either, and how much of the interactive path
becomes fork changes.

## Decision

**Answer files stay kayak's.** A Mandrake answer file is a kayak answer
file. Mandrake adds verbs as functions in `/kayak/lib/mandrake.sh`,
delivered into the installer ramdisk by the miniroot overlay:

| Verb | Meaning |
|---|---|
| `MandrakeAdmin <user> <password>` | The console admin, and the OS admin user |
| `MandrakeMgmt <link> dhcp` | Management VNIC `mgmt0` over `<link>`, DHCP |
| `MandrakeMgmt <link> <addr>/<prefix> [<gateway>]` | The same, static |
| `MandrakeSshKey '<public key>'` | An authorised key for the OS admin; repeatable |

`BuildRpool`, `SetHostname`, `UseDNS`, `SetTimezone`, and `SetRootPW`
are used as kayak defines them. The interactive path asks the same
questions and calls the same functions, so the two paths cannot drift.
The installed BE is `mandrake-<version>` on both paths, `<version>` being
the Mandrake version from `mandrake.env`.

**First boot.** The installer writes `/etc/mandrake/firstboot.json` into
the new BE: the admin username and password, mode 0600, owned by the
`mandrake` uid the package fixes at 63. The daemon reads it when it
starts, and if the users table is empty creates the admin with role
`admin`, records `user.create` with actor `installer`, then overwrites
and unlinks the file. A file found when users already exist is destroyed
without being applied and a warning is logged. The password is in clear
on the local root pool between install and first boot, which is the same
exposure kayak's own answer files have for the root password; hashing in
the ramdisk would mean shipping the daemon into the miniroot for its
argon2 parameters, and that is deferred until the ramdisk size is worth
paying.

**Banner.** A third service in the daemon's bundle,
`svc:/system/mandrake/banner`, is transient, runs as root after
`mandraked` is online, computes the SHA-256 fingerprint of
`/etc/mandrake/tls/cert.pem` with `openssl`, prints the console URL and
fingerprint to `/dev/msglog` and `/dev/console`, and rewrites the block
between two marker lines in `/etc/motd`. Every boot refreshes it, so a
replaced certificate shows on the next login.

**Management path.** The installer creates the VNIC `mgmt0` over the
chosen physical link and puts the management address on `mgmt0`, DHCP or
static, with the default route when given. The physical link stays free
for aggregations, VLANs, and zone NICs, and the Phase 3 protection
(ADR-0011) has one link to guard. The address is persistent `ipadm`
configuration applied by kayak's `Postboot` mechanism on first boot.

**OS access.** The admin username is also an OS user with the Primary
Administrator profile, no password, and the authorised keys from
`MandrakeSshKey` or the interactive paste field. `sshd_config` gets
`PermitRootLogin no` and `PasswordAuthentication no` from the installer;
the file is `preserve=true`, so `pkg` leaves the edit alone. Root keeps
its password for the serial console. IP Filter is enabled with a rule set
that passes SSH and HTTPS in on `mgmt0`, passes everything out, and
blocks other inbound traffic to the global zone; it is written by the
installer because it names the management link.

**Fork changes.** New installer screens are new scripts under
`build/installer/`, staged into the miniroot at `/kayak/installer/` by
the overlay. The fork carries one Mandrake patch, in
`build/patches/kayak/`, that makes kayak's menu run Mandrake's
configuration after the image install instead of the OmniOS
configuration menu. Kayak's dialog and text menu machinery, disk
selection, and image install are used unchanged.

**Media.** Installs keep receiving kayak's ZFS stream. Spec §10 step 3
describes a local IPS repository on the media; the stream already needs
no network and the packages are inside it, so a second install path is
not built. `pkg` on an installed host still needs the
`nightshade.systems` origin for updates, which Phase 7 covers.

## Consequences

- One answer format, kayak's, with four Mandrake verbs; existing kayak
  documentation applies to the rest. A Mandrake answer file without
  `MandrakeAdmin` installs a host with no console user, and the installer
  refuses it before touching a disk.
- The daemon owns admin creation, so the console and the socket path
  (`mandrakectl users create` as root, ADR-0007) stay the only ways a
  user is created; the installer never writes SQLite.
- The banner service is the only part of Mandrake that writes `/etc/motd`;
  the branded text from Phase 1 stays above its markers.
- Hosts installed by hand onto a preconfigured rpool skip Mandrake's
  screens and come up with no management VNIC and no admin; the recovery
  is the root socket and `dladm`/`ipadm` by hand, documented in
  `docs/build.md`.
- The single fork patch must be carried forward when the OmniOS pin moves,
  alongside the overlay backports from ADR-0005.
- The ramdisk gains only shell scripts; the miniroot package list does not
  change.
