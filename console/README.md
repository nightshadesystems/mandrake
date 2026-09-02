# mandrake-console

Web console for Mandrake. Vite + React + TypeScript, built to static assets in
`dist/` and embedded in `mandraked` with `rust-embed`.

```sh
pnpm install
pnpm dev         # dev server
pnpm lint        # eslint + prettier check
pnpm typecheck   # tsc -b
pnpm build       # tsc -b && vite build -> dist/
```

Rules (spec §8, §14):

- TypeScript strict. ESLint and Prettier must pass.
- No component library beyond what `design/` defines.
- The console talks only to the public API through a client generated from
  `api/openapi.yaml`. Never hand-write `fetch` calls. If the console needs
  something the API does not expose, extend the API and the OpenAPI file first.
