# build/packages

IPS packages on the `nightshade.systems` publisher, built with the
omnios-build framework from `build/omnios-build` (ADR-0001, ADR-0010).

| Directory | Package | Contents |
|---|---|---|
| `mandraked/` | `system/mandrake/daemon` | daemon binary with the embedded console, SMF services, method script, RBAC profile, `mandrake` user and directories |
| `mandrakectl/` | `system/mandrake/cli` | the CLI |
| `mandrake-incorporation/` | `incorporation/mandrake/mandrake-incorporation` | pins the two packages to one release |

Each `build.sh` sources `../../omnios-build/lib/build.sh` and follows the
framework's conventions (`PKG`, `VER`, `SUMMARY`, `local.mog`, `make_package`).
The source is this repository, copied into the build's temp directory rather
than downloaded. `build-packages.sh` copies `build/site.sh` into the
framework, builds in order, and rebuilds the repository index.

```sh
just build-packages           # build/out/repo, all packages
just build-packages mandrakectl
just publish-repo /path/or/http://repo   # pkgrecv into another repository
```

Version comes from the workspace `Cargo.toml`. The package FMRI branch is
the OmniOS release the build host runs, so packages built on r151054
install only on r151054 (ADR-0006).
