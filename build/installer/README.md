# build/installer

Mandrake's additions to the kayak installer (ADR-0014). Everything here
reaches the installer ramdisk through the miniroot overlay that
`build/media/build-media.sh` stages; kayak's own scripts are not copied
or replaced.

| File | In the ramdisk | Purpose |
|---|---|---|
| `lib/mandrake.sh` | `/kayak/lib/mandrake.sh` | Answer-file verbs and `MandrakeApply`; sets the BE name |
| `mandrake-config` | `/kayak/installer/mandrake-config` | Interactive screens: admin, SSH key, management NIC, DNS |
| `answer.sample` | PXE tarball `http/kayak/000000000000.sample` | Unattended install template |

The fork patch in `build/patches/kayak/` sources `mandrake.sh` from
kayak's `install_help.sh`, runs `MandrakeApply` after an answer file, and
sends the interactive path through Mandrake's screens.

## Answer files

A Mandrake answer file is a kayak answer file with four more verbs:

| Verb | Effect |
|---|---|
| `MandrakeAdmin <user> <password>` | Console admin created by the daemon on first boot; OS user with the Primary Administrator profile, no password |
| `MandrakeMgmt <link> dhcp` | VNIC `mgmt0` over `<link>` with a DHCP address |
| `MandrakeMgmt <link> <addr>/<prefix> [<gateway>]` | The same, static |
| `MandrakeSshKey '<key>'` | An authorised key for the OS admin; repeatable |

`MandrakeAdmin` and `MandrakeMgmt` are required; the install stops before
the reboot without them. Kayak's `BuildRpool`, `SetHostname`,
`SetTimezone`, `UseDNS`, `RootPW`, and `Postboot` work as documented in
`build/kayak/lib/LAYOUT.md`.

What `MandrakeApply` writes into the new boot environment:

- `/etc/mandrake/firstboot.json` (0600, owner `mandrake`) with the admin
  username and password; the daemon creates the admin and destroys the
  file on its first start.
- First-boot commands (`/.initialboot`) that create the OS admin and its
  keys, the `mgmt0` VNIC and address, and enable IP Filter.
- `sshd_config`: `PermitRootLogin no`, `PasswordAuthentication no`.
- `/etc/ipf/ipf.conf`: SSH and HTTPS in on `mgmt0`, everything out,
  other inbound blocked.
