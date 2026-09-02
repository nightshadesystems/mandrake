# mandrake-console

Web console for Mandrake. Vite + React + TypeScript, built to static assets
in `dist/` and embedded in `mandraked` with `rust-embed`.

```
console/
├── design/         # the Nightshade design system export (see design/README.md)
├── public/vendor/  # Clarity icons, copied from node_modules at build time (ignored)
├── scripts/        # gen-api-docs.mjs (docs/api.md), vendor.mjs (public/vendor)
└── src/
    ├── api/        # client.ts (openapi-fetch + problem handling), hooks.ts, events.ts
    │   └── schema.d.ts   generated from api/openapi.yaml (ignored)
    ├── design/     # index.tsx: typed facade over design/components; clr-icon typing
    ├── pages/      # Login, Shell, Dashboard, Users, Audit, NotYet
    ├── App.tsx     # routes
    ├── main.tsx    # fonts, styles, query client
    ├── app.css     # layout only; every value comes from design/ tokens
    ├── fmt.ts      # timestamps, sizes, durations per the content rules
    └── theme.ts    # dark default, light via data-theme, remembered per browser
```

```sh
pnpm install
pnpm dev         # generates the client, vendors icons, starts the dev server
pnpm lint        # eslint + prettier check
pnpm typecheck   # tsc -b
pnpm build       # dist/, what mandraked embeds
```

Developing against a daemon: run `mandraked` locally, for example

```sh
mandraked --listen 127.0.0.1:8443 --db /tmp/m.db --tls-dir /tmp/m-tls --no-socket
MANDRAKE_DEV_SERVER=https://127.0.0.1:8443 MANDRAKE_DEV_TLS_DIR=/tmp/m-tls pnpm dev
```

The dev server proxies `/api` (including the event WebSocket) to the daemon
and serves itself over HTTPS with the daemon's certificate, so the `Secure`
session cookie is accepted. The first user must exist already; create it
with `mandrakectl` over the root socket on the host, or on a development
machine through the daemon's integration-test path.

Rules (spec §8, §14, ADR-0008):

- TypeScript strict. ESLint and Prettier must pass.
- No component library beyond what `design/` defines; pages import from
  `src/design/index.tsx`, never from `design/components` directly.
- The console talks only to the public API through the generated client.
  Never hand-write `fetch` calls; ESLint bans the global. If the console
  needs something the API does not expose, extend the API and the OpenAPI
  file first.
- Nothing loads from a CDN: fonts come from `@fontsource`, icons from the
  vendored `@clr/icons` bundle.
