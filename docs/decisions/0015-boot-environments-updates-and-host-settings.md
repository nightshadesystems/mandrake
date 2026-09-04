# ADR-0015: Boot environments, updates, reboot, and host settings

- **Status:** Accepted
- **Date:** 2026-09-03
- **Phase:** 7 (Boot environments and updates)

## Context

Spec §1 makes upgrades new boot environments with rollback by reboot;
§13 asks for `beadm` integration and an update flow (new BE, `pkg update`
into it, activate, reboot, rollback from the console); §8 puts hostname,
time and NTP, boot environments, and updates on the System page, and §12
wants a custom TLS certificate uploadable there. Installed hosts carry
both the `omnios` and `nightshade.systems` publishers (ADR-0006), and the
`mandrake-incorporation` package pins the OmniOS core to the release
(ADR-0010).

Left open: how a boot environment is identified, what an "update" is and
who names the boot environment it lands in, whether the daemon may
reboot the host, how rollback is expressed, how the running daemon takes
a new certificate, and how the hostname, timezone, and NTP servers are
written so that `pkg` and SMF stay consistent.

## Decision

**Boot environments.** A BE is `beadm`'s own object: its `id` is the
`org.opensolaris.libbe:uuid` beadm assigns, read from `beadm list -H`
(fields `name;uuid;active;mountpoint;space;policy;created`), and its name
is its natural key on the wire and the CLI. Operations are `beadm list`,
`beadm create <name>` (a snapshot of the active BE), `beadm activate
<name>`, and `beadm destroy -F <name>`. Destroying the active or the
booted BE is refused. The `rpool` protection of ADR-0011 is unchanged;
BEs are the only way the API touches `rpool/ROOT`.

**Updates.** An update is whatever `pkg update` offers across every
configured publisher; the incorporation keeps the OmniOS core on the
release, so in practice that is Mandrake's packages plus OmniOS
maintenance. The flow is three separate calls:

1. *Check*: a job that runs `pkg refresh --full` and `pkg update -nv`,
   parses the dry run into a plan (packages with old and new versions,
   whether a reboot is needed, the BE the update would create), and
   stores it in SQLite with the check time. A host with nothing to do
   stores an empty plan.
2. *Apply*: a job that runs `pkg update -v --be-name <name>` with the
   BE name from the plan. `pkg` creates the BE, installs into it, and
   activates it; the running BE is not modified. Progress comes from
   `pkg`'s output. The job refuses to start while another apply runs and
   when the plan is empty or older than the last refresh.
3. *Reboot*: `POST /system/reboot`, admin only, audited before it runs,
   then `shutdown -y -g 0 -i 6` after a short grace so the response
   reaches the client. Nothing else in the daemon reboots the host.

The BE name is `mandrake-<version>` when the plan changes the Mandrake
incorporation, else `mandrake-<current version>-<yyyymmdd>`; a clash
appends `-N`. Rollback is `beadm activate` of the previously active BE
followed by the reboot call; the Updates page offers it while such a BE
exists, and the boot environments page offers activate generically.

**Host settings.** `PATCH /system` sets the hostname (`hostname`,
`/etc/nodename`, `/etc/inet/hosts`, and `svc:/system/identity:node`),
the timezone (`svccfg -s timezone:default setprop timezone/localtime`
and a refresh, which also rewrites `/etc/default/init`), and the NTP
servers (the `server` lines of `/etc/inet/chrony.conf`, then a restart
of `svc:/network/chrony`). Each is applied by the daemon through
`pfexec`; the hostname change applies at once to the running system and
the API reports the new value from `hostname`.

**TLS.** `PUT /system/tls` takes a PEM certificate chain and key. The
daemon parses both, checks the key matches the leaf, writes them to the
TLS directory with a backup of the previous pair, and reloads the
listener's configuration in place; no restart. The response carries the
new fingerprint, and the banner service picks it up on the next boot.
`DELETE /system/tls` returns to a generated self-signed pair.

**RBAC.** `/usr/bin/pkg`, `/usr/sbin/shutdown`, `/usr/sbin/svccfg`,
`/usr/bin/hostname`, and `/usr/sbin/svcadm` (already present) join the
Mandrake Management profile. `pkg` runs as root through `pfexec` with the
daemon's environment, never a shell.

## Consequences

- The daemon never edits a BE's contents; `pkg` does, into a BE it owns,
  so a failed update leaves the running system untouched and the new BE
  either absent or inactive.
- Two reboots are the cost of a rollback (one to try, one to go back),
  which is the ZFS-root model the spec chose.
- Parsing `pkg update -nv` output is the fragile part: it is
  human-readable, not `-p`. The parser is tested against captured output
  in `crates/mandraked/testdata/` and treats anything it cannot read as a
  plan it will not apply, reporting the raw text.
- Publisher changes (a moved origin, `pkg set-publisher`) are not in the
  API; they stay an operator task over SSH or the root socket.
- The TLS reload path means the console's fingerprint pinning in
  `mandrakectl --fingerprint` must be updated by whoever uploads a
  certificate; the response and audit row carry the new fingerprint.
- Snapshot schedules (§6.1) are not part of this phase and get their own
  decision if a Phase 8 is opened.
