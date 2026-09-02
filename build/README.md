# build

Everything that produces packages and media. The scripts here run on the
OmniOS build host, never on the workstation (docs/build.md). Claude Code
writes them; Cody runs them.

```
build/
├── kayak/           # submodule: nightshadesystems/mandrake-kayak, branch mandrake/r151054
├── omnios-build/    # submodule: nightshadesystems/mandrake-build, branch mandrake/r151054
├── patches/kayak/   # upstream commits backported onto the fork's release branch
├── media/           # Mandrake media build: overlays, hooks, ISO/USB/PXE assembly
├── vendor.sh        # initialise the submodules, with the Windows workaround
├── packages/        # one build.sh per Mandrake package          (Phase 2)
├── manifests/       # IPS manifests and the pinned incorporation (Phase 2)
└── out/             # build products, ignored by git
```

`just vendor` runs `vendor.sh`. `just build-iso`, `build-usb`, `build-pxe`
and `init-repo` wrap `media/`. See [media/README.md](media/README.md).
