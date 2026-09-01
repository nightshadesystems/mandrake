# testdata

Captured real output from illumos tooling, used by parser unit tests. Each
file is named `<command>.<variant>.txt` (for example `zoneadm-list-p.two-zones.txt`)
and begins with a comment line recording the OmniOS release and the exact
command that produced it.

Never edit captured output by hand. Re-capture on the build host.
