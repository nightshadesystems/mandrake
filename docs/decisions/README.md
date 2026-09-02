# Architecture Decision Records

Decisions not covered by `docs/mandrake-spec.md` are recorded here before any
code that depends on them is written (spec §14).

Numbering is sequential and never reused. A superseded ADR keeps its file and
gains a `Superseded by` line; the replacement links back.

Use [0000-template.md](0000-template.md) for new records.

| ADR | Title | Status |
|---|---|---|
| [0001](0001-overlay-not-fork.md) | Overlay an IPS publisher on OmniOS rather than fork it | Accepted |
| [0002](0002-illumos-source-of-truth-sqlite-metadata.md) | Illumos is the source of truth; SQLite holds only metadata | Accepted |
| [0003](0003-shell-out-drivers.md) | Drivers shell out to illumos tooling | Accepted |
