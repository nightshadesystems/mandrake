# ADR-0009: SQLite from the OmniOS package on illumos, bundled elsewhere

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 2 (Daemon core)

## Context

`mandraked` uses `rusqlite` (spec §6.1). Its `bundled` feature compiles
SQLite from C, which needs a C compiler for the target: cross-checking for
`x86_64-unknown-illumos` on a workstation or in CI would fail, and the
appliance would carry a private SQLite copy outside OmniOS's update path.
OmniOS ships `database/sqlite-3` with `libsqlite3.so`.

## Decision

On illumos, `rusqlite` links the system `libsqlite3` (no `bundled`
feature) and the `mandraked` package depends on `database/sqlite-3`. On
every other target it uses `bundled`, so unit tests run anywhere without a
system library. The split is a target-specific dependency in
`crates/mandraked/Cargo.toml`, not a cargo feature the caller has to
remember.

The database is opened in WAL mode with `synchronous=NORMAL`, foreign keys
on, and a 5 second busy timeout. Schema migrations are SQL files embedded in
the binary, applied in order on startup, tracked with `PRAGMA user_version`.
A newer binary migrates forward; an older binary refuses to start against
a newer schema, so a boot-environment rollback must also roll the data
back or keep the newer daemon.

## Consequences

- `cargo check --target x86_64-unknown-illumos` needs no C toolchain.
- The GitHub build workflow cross-compiles illumos binaries on Linux with a
  sysroot that has no `libsqlite3`, so those artifacts enable the
  `bundled-sqlite` feature. They are smoke-test downloads; the packages on
  the media still link the system library.
- SQLite fixes reach the appliance through `pkg update`, and the version
  is whatever the pinned OmniOS release ships (3.4x for r151054).
- The database file lives on `rpool/mandrake/var` (spec §6.1), a dataset
  the update flow (Phase 7) snapshots before activating a new BE.
