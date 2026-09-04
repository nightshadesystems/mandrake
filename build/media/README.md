# build/media

Mandrake install media: stock OmniOS from the pinned release plus Mandrake
branding and the `nightshade.systems` publisher, built with kayak.

| File | Purpose |
|---|---|
| `mandrake.env` | The only configuration: release, version, publisher, origin URL |
| `build-media.sh` | Stages overlays, drives kayak, verifies, collects into `build/out/` |
| `init-repo.sh` | Creates the empty `nightshade.systems` file repository; can serve it |
| `hooks/` | kayak post-overlay hooks run inside the installed-system image |
| `test-boot.sh` | Unattended PXE install into a bhyve VM on the build host plus API smoke tests (`just test-boot`) |
| `../installer/` | Answer-file verbs, interactive screens, sample answer file; staged into the ramdisk (ADR-0014) |

How the pieces reach the image (ADR-0005):

- `overlay/` is copied verbatim onto the installed-system root.
- `branding/motd` becomes `/etc/motd`; `branding/loader/` becomes
  `/boot/forth/{brand,logo}-mandrake.4th` and `/boot/conf.d/mandrake` on
  both the installed system and the installer ramdisk, which is also the
  ISO root.
- `hooks/*.sh` run once with the image root as their argument, after kayak
  has set the `omnios` publisher. `10-nightshade-publisher.sh` adds ours.

Run on the build host as root:

```sh
just build-iso          # build/out/mandrake-<ver>-r151054.iso
just build-usb          # ...usb (also builds the ISO)
just build-pxe          # ...-pxe.tar.gz
just build-media -n iso # stage and show what would run
just build-media clean  # destroy the kayak dataset and staging
```

Set `PKGURL` to build from a local OmniOS mirror, `PREBUILT_ILLUMOS` to a
built illumos-omnios workspace (kayak recommends it), and `BUILDSEND` if the
default dataset name does not suit.
