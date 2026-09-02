# ADR-0004: Vendor the build-system forks as git submodules on release branches

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 1 (Media)

## Context

Mandrake media and packages are produced by forks of two OmniOS repositories,
`omnios-build` and `kayak` (spec §3). The forks exist as
`nightshadesystems/mandrake-build` and `nightshadesystems/mandrake-kayak`.
The monorepo needs their contents under `build/` (spec §4) at a known
revision, and each must track the OmniOS release Mandrake pins (ADR-0001).

Two ways to vendor them were considered: git submodules, which pin a commit
of the separate fork repository, and git subtrees, which copy the fork's
contents into the monorepo history.

Subtrees would make the separate fork repositories redundant, make upstream
merges noisy in the monorepo log, and put tens of thousands of upstream
files into every monorepo checkout and diff.

## Decision

Both forks are git submodules:

| Path | Repository | Branch |
|---|---|---|
| `build/kayak` | `nightshadesystems/mandrake-kayak` | `mandrake/r151054` |
| `build/omnios-build` | `nightshadesystems/mandrake-build` | `mandrake/r151054` |

Each fork carries a `mandrake/<release>` branch: the upstream release branch
plus whatever Mandrake needs on top, kept as small as possible and preferably
limited to backported upstream commits (see `build/patches/`). The upstream
release branch itself stays untouched in the fork for syncing. The monorepo
pins a commit on the `mandrake/<release>` branch and moves the pin
deliberately, the same way the OmniOS release is pinned.

`build/vendor.sh` (`just vendor`) initialises the submodules.

## Consequences

- Fork changes are commits in the fork repositories, then a gitlink bump in
  the monorepo. Two commits per change, but each repository's history stays
  about one thing.
- A fresh clone needs `just vendor` before any build-host work.
- `omnios-build` contains `build/XML::Parser`, and a colon cannot appear in
  an NTFS file name, so a plain checkout fails on Windows. `vendor.sh`
  handles it with a sparse checkout that leaves that one directory out of
  the working tree, plus `core.longpaths`. Windows checkouts of the forks
  are for reading; builds run on OmniOS.
- Moving the OmniOS pin means creating `mandrake/<new release>` in each
  fork, re-checking the backports, and updating `.gitmodules` and the
  gitlinks together with ADR-0001.
