# ADR-0001: Overlay an IPS publisher on OmniOS rather than fork it

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 1 (Media), applies to every phase after

## Context

Mandrake needs a bootable, installable operating system with bhyve, zones,
Crossbow, and a ZFS root with boot environments. OmniOS CE already ships all of
that, on a predictable LTS cadence, with an IPS packaging system designed for
layering publishers.

Two ways to build on it were considered:

1. **Fork the `illumos-omnios` gate and `omnios-build`**, rebrand every package,
   and publish a single Mandrake publisher containing the whole OS.
2. **Overlay.** Consume OmniOS kernel and core packages unmodified from the
   `omnios` publisher and add a second publisher, `nightshade.systems`, that
   carries only what Mandrake adds: `mandraked`, `mandrakectl`, console assets,
   branding, and a curated incorporation that pins the OmniOS release.

A full fork means owning security updates for the entire OS, rebuilding the
world on every upstream change, and diverging from upstream semantics that
`zadm`, `beadm`, and the bhyve brand assume. Nothing in the goals (spec §1)
needs a kernel or libc change.

## Decision

Mandrake overlays. Core OS packages come from OmniOS CE unmodified. Mandrake
packages live on the `nightshade.systems` IPS publisher, layered on top. The
`omnios-build` and `kayak` repositories are forked only to add Mandrake package
recipes and installer branding, not to change what they build from upstream.

The OmniOS pin is **r151054 LTS**, the current LTS as of this date. It is
re-pinned only at an LTS boundary and recorded by updating this ADR.

OmniOS CE remains credited in `/etc/release` and in `docs/`. No upstream
attribution, copyright, or licence file is removed or rewritten.

## Consequences

- Upstream security and bug fixes arrive as ordinary `pkg update` runs into a
  new boot environment. Mandrake does not rebuild the kernel.
- Mandrake cannot change kernel, libc, or core userland behaviour. If something
  is genuinely needed upstream, it goes upstream first.
- The installer must lay down two publishers and the incorporation must pin
  both. Media builds carry a subset of the `omnios` repo so installs need no
  network (spec §10).
- Branding is limited to what packages and overlay files can change: loader
  banner, MOTD, `/etc/release`, prompt, console title (spec §11).
- Reopen this decision if a required feature cannot be delivered without
  patching a core OmniOS package. That has not happened yet.
