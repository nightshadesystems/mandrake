# testdata

Command output the parsers in `src/parse.rs` are tested against.

- `*.synthetic.txt`: hand-written in the documented format, used until a
  real capture exists (ADR-0011). Delete each one when its capture lands.
- `<command>.<host>.txt` with a `.meta` sidecar: captured verbatim on an
  OmniOS host by `build/tools/capture-testdata.sh`. Never edit by hand;
  re-capture. The sidecar records the host, release, date, and exact
  command so the data stays reproducible.

A parser that disagrees with a capture is a bug in the parser.
