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
| [0004](0004-vendor-forks-as-submodules.md) | Vendor the build-system forks as git submodules on release branches | Accepted |
| [0005](0005-media-customisation-via-kayak-overlays.md) | Customise install media through kayak overlays, not kayak changes | Accepted |
| [0006](0006-nightshade-publisher.md) | The nightshade.systems publisher: name, origin, and signing | Accepted |
| [0007](0007-auth-sessions-and-tokens.md) | Local users, sessions, tokens, and the root socket | Accepted |
| [0008](0008-console-stack-and-vendoring.md) | Console stack, generated client, and vendored design assets | Accepted |
| [0009](0009-sqlite-linkage.md) | SQLite from the OmniOS package on illumos, bundled elsewhere | Accepted |
