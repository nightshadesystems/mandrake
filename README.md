# Mandrake

An illumos-based hypervisor operating system from Nightshade Systems, derived
from OmniOS CE. Mandrake runs virtual machines under bhyve and containers as
illumos zones on a native ZFS root, managed through a web console backed by a
single Rust daemon (`mandraked`) with a thin CLI (`mandrakectl`) for scripting
and recovery.

Mandrake is a hypervisor appliance, not a network operating system. There is no
configuration shell and no commit model. Illumos system state is the source of
truth; Mandrake adds an API, a console, and the metadata illumos does not hold.

## Layout

| Path | Purpose |
|---|---|
| `crates/` | Rust workspace: daemon, CLI, shared types, illumos drivers |
| `console/` | Web console (Vite + React + TypeScript), embedded in `mandraked` |
| `api/openapi.yaml` | API contract, source of truth for API shape |
| `docs/` | Spec, ADRs, generated API reference, build notes |
| `build/` | OmniOS build-system and installer forks, IPS manifests, media assembly |
| `branding/` | Loader banner, MOTD, `/etc/release` |
| `overlay/` | Files dropped onto the image root |

Start with [docs/mandrake-spec.md](docs/mandrake-spec.md). Decisions live in
[docs/decisions/](docs/decisions/). Build instructions are in
[docs/build.md](docs/build.md).

## Building

Requires stable Rust with the `x86_64-unknown-illumos` target, Node 22 with
pnpm, and [`just`](https://github.com/casey/just).

```sh
just vendor          # fork submodules, once per clone
just build-crates    # cargo build --workspace
just build-console   # pnpm install && pnpm build in console/
just lint            # fmt check, clippy pedantic, eslint, prettier
```

Media and package targets run on an OmniOS build host. See `docs/build.md`.

## Upstream

Mandrake consumes the OmniOS CE kernel and core packages unmodified and layers
a `nightshade.systems` IPS publisher on top. OmniOS CE is credited in
`/etc/release` and no upstream attribution is removed.

## License

MIT. See [LICENSE](LICENSE).
