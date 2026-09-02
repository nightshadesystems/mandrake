# ADR-0003: Drivers shell out to illumos tooling

- **Status:** Accepted
- **Date:** 2026-09-01
- **Phase:** 3 (Storage and network), applies to every driver crate

## Context

The driver crates (`mandrake-zones`, `mandrake-bhyve`, `mandrake-net`,
`mandrake-zfs`, `mandrake-smf`) must create, inspect, and change illumos
objects. Two mechanisms exist:

1. **Native libraries via FFI**: `libzonecfg`, `libdladm`, `libipadm`,
   `libzfs`, `libbe`. These are private, unstable interfaces with no ABI
   guarantee between OmniOS releases, are sparsely documented, and require
   `unsafe` Rust plus per-release binding maintenance.
2. **The command-line tools**: `zonecfg`, `zoneadm`, `dladm`, `ipadm`, `zfs`,
   `zpool`, `beadm`, `svcs`, `svcadm`. These are the supported operator
   interface, are what `zadm` and every OmniOS document use, and most offer a
   parsable output mode (`-p`, `-H`, `-o`) intended for scripts.

The security baseline (spec §12) already requires `mandraked` to run as a
dedicated user with RBAC profiles for exactly these commands, so the privilege
model is the same either way.

## Decision

Every driver shells out to the native illumos command-line tools and parses
their output. Where a parsable mode exists it is used; free-form output is
parsed only where no alternative exists and the parser is covered by tests
against captured real output.

Rules for every driver crate:

- Each driver exposes **typed operations** (`create_vnic`, `list_datasets`,
  `halt_zone`) with typed arguments and typed errors. There is no generic
  apply-desired-state entry point.
- Commands are built as argument vectors, never as shell strings. No `sh -c`.
- Every invocation is logged at `debug` with its full argument vector and at
  `warn` on non-zero exit, with stderr captured into the error.
- Parsers are pure functions over `&str` and are unit-tested against files in
  `crates/<driver>/testdata/` captured from a real OmniOS host. Captured files
  are never edited by hand.
- Anything that actually invokes a tool is an integration test gated on
  `cfg(target_os = "illumos")` and skipped elsewhere with a visible marker.
- Direct FFI is permitted later as an optimisation for a specific hot path,
  recorded in its own ADR, and must keep the same typed operation surface.

## Consequences

- No `unsafe` code in the drivers; the workspace forbids it.
- Behaviour tracks the OmniOS release exactly, because the tools are the
  release. Upgrading the pin means re-capturing testdata and re-running
  integration tests, not rebinding a library.
- Latency per operation is a process spawn plus parse, typically a few
  milliseconds. The short-TTL cache from ADR-0002 keeps list endpoints cheap.
- Output formats can change between releases. The captured-output tests are
  the early warning; a parser failure on a new release is a build break, not a
  runtime surprise.
- Reopen this decision for a specific driver only if measured latency or a
  missing parsable mode makes the CLI route unfit, and record the FFI
  exception as a new ADR.
