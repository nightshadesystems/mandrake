# build

Everything that produces packages and media. Runs on the OmniOS build host,
not the workstation (docs/build.md). Populated from Phase 1.

Planned layout (spec §4):

```
build/
├── omnios-build/   # fork of github.com/omniosorg/omnios-build, vendored
├── kayak/          # fork of github.com/omniosorg/kayak, vendored
├── packages/       # one build.sh per Mandrake package
├── manifests/      # IPS manifests and the pinned incorporation
└── media/          # ISO, USB, and PXE assembly
```

Whether the forks are vendored as submodules or subtrees is decided in an ADR
at the start of Phase 1.
