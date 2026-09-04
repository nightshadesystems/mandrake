# testdata

Command output the daemon's own parsers (`src/pkg.rs`) are tested
against, with the same rules as the driver crates (ADR-0011):

- `*.synthetic.txt`: hand-written in the documented format, used until a
  real capture exists. Delete each one when its capture lands.
- `<command>.<host>.txt` with a `.meta` sidecar: captured verbatim on an
  OmniOS host by `build/tools/capture-testdata.sh pkg`. Never edit by
  hand; re-capture.

`pkg update -nv` is human-readable output, not `-p`; the parser reads the
header counts, the `Create boot environment` line, and the `Changed
packages` section, and reports anything else as unparsed so the daemon
refuses to apply a plan it did not understand (ADR-0015).
