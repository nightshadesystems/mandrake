# branding

Everything that makes the system say Mandrake / Nightshade Systems
(spec §11). OmniOS CE stays credited in `/etc/release` and in the MOTD; no
upstream attribution or copyright file is removed.

| File | Lands at | How |
|---|---|---|
| `loader/brand-mandrake.4th` | `/boot/forth/` | overlay; selected by `loader_brand` |
| `loader/logo-mandrake.4th` | `/boot/forth/` | overlay; selected by `loader_logo` |
| `loader/conf.d/mandrake` | `/boot/conf.d/` | overlay; read after `loader.conf.local` |
| `motd` | `/etc/motd` | overlay; `preserve=true` in `SUNWcs` |

`build/media/build-media.sh` stages these into kayak's overlays for both
the installed system and the installer ramdisk (ADR-0005). The loader files
are text-only so the banner renders identically on a framebuffer and on a
serial console.

Still OmniOS's, by design, until Phase 2 ships a `release/name` package:
`/etc/release`, `/etc/os-release`. Web console title: Phase 2.

Testing a Forth change without building media: copy the two `.4th` files
into `/boot/forth/` and the snippet into `/boot/conf.d/` on any OmniOS
r151054 host or VM and reboot; the loader menu shows the result.
