# ADR-0005: Customise install media through kayak overlays, not kayak changes

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 1 (Media), applies through Phase 6

## Context

Phase 1 needs install media that boot with Mandrake branding and install a
system listing both the `omnios` and `nightshade.systems` publishers, built
from stock OmniOS r151054 packages. Kayak produces the installer ramdisk, the
installed-system ZFS stream, and the ISO, USB, and PXE media from a package
repository.

Kayak on its master branch has an overlay mechanism: `ZFS_CUSTOM_OVERLAY`
and `MINIROOT_CUSTOM_OVERLAY` name directories copied onto the
installed-system image and the installer ramdisk, and scripts in
`.overlay-hooks/` run against the image root afterwards. The r151054 release
branch predates it. The illumos loader reads `/boot/conf.d/*` after
`loader.conf.local`, so a conf.d snippet overrides the menu title kayak
writes for install media.

The alternatives were to patch kayak's scripts directly in the fork for each
branding point, or to post-process kayak's outputs (receive, edit, re-send
the stream; remaster the ISO) in Mandrake's own scripts.

## Decision

Mandrake's media build (`build/media/build-media.sh`) drives unmodified kayak
targets and injects everything through the overlay mechanism:

- `overlay/` and `branding/` are staged into the two overlay directories.
- The loader brand and logo are new Forth files, `brand-mandrake.4th` and
  `logo-mandrake.4th`, selected by `/boot/conf.d/mandrake`. No packaged
  file is replaced.
- `/etc/motd` is replaced; it is `preserve=true` in `SUNWcs`, so `pkg`
  treats the edit as an administrator's.
- The `nightshade.systems` publisher is added by a post-overlay hook that
  runs `pkg set-publisher --no-refresh` against the image root.

The only fork content is the backport of the upstream overlay commits onto
`mandrake/r151054` (`build/patches/kayak/`). Kayak logic is not otherwise
changed in Phase 1. The ISO volume label stays `OmniOS r151054`; it is
cosmetic and used only by `mount_media` to match the media to the image.

Kayak installs by receiving a prebuilt ZFS stream, so installs already need
no network. Spec §10 step 3 describes installing from a local IPS
repository on the media instead; that is not how kayak works and is not
needed for the no-network goal. The installer phase (6) should keep the
stream mechanism and revisit §10 rather than build a second install path.

## Consequences

- Rebranding never fights `pkg verify` or `pkg update`: the Forth files
  and conf.d snippet are unpackaged additions, and motd is preserved.
- `/etc/release` and `/etc/os-release` stay OmniOS's until Mandrake ships
  its own `release/name` package in Phase 2, because kayak, `beadm`, and
  the installer parse them and an overlaid copy would be reverted by `pkg`.
- Hooks are only warnings to kayak when they fail, so `build-media.sh`
  verifies the finished image and ISO itself and fails loudly.
- PXE loads Forth files and config over TFTP, which cannot list
  `/boot/conf.d`; the PXE tarball carries the loader variables in
  `loader.conf.local` instead, appended by `build-media.sh`.
- If a later phase needs a change that overlays cannot express (installer
  dialogs, BE naming, hostname defaults in Phase 6), it becomes a fork
  commit on `mandrake/<release>` and, where sensible, an upstream pull
  request.
