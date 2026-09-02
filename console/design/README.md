# console/design

Export of the Nightshade Systems design system (VMware Clarity re-skinned;
CC0, see `LICENSE`), copied from `nightshadesystems/design-system`. This is
the only source of colour, type, spacing, radius, and component primitives
used anywhere in the console (spec §8, ADR-0008).

```
design/
├── styles.css      # single CSS entry: tokens, then the Clarity class sheets
├── tokens/         # colours (dark default, [data-theme="light"]), type, spacing, motion
├── clarity/        # the Clarity class framework (.btn, .datagrid, .modal, ...)
├── components/     # React components, JSX, verbatim from the export
├── types/          # the export's *.d.ts prop interfaces, moved out of components/
├── assets/         # logos and brand glyphs
├── UPSTREAM.md     # the export's own readme: rules, iconography, caveats
└── LICENSE
```

Rules for this directory:

- **A copy, never a hand edit.** Re-export from upstream to change anything.
  The one deliberate edit is `tokens/fonts.css`: the Google Fonts import is
  replaced by the self-hosted `@fontsource` packages imported in
  `src/main.tsx`, so the appliance serves its own fonts.
- The `.d.ts` files live in `types/` rather than beside the `.jsx` because
  they declare only `*Props` interfaces and would otherwise shadow the
  components during TypeScript module resolution. `src/design/index.ts`
  is the typed facade the console imports from; it binds each JSX component
  to its upstream props interface. Nothing under `src/` imports
  `design/components` directly.
- Clarity icons (`@clr/icons`, UMD) are copied into `public/vendor/` by
  `scripts/vendor.mjs` at build time and loaded from `index.html`; the
  `<clr-icon>` element is declared for JSX in `src/design/clr-icon.d.ts`.
- ESLint and Prettier skip this directory.
