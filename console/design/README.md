# console/design

Export of the Nightshade Systems design system: tokens as CSS variables and
JSON, plus component primitives. This is the only source of colour, type,
spacing, and radius values used anywhere in the console (spec §8).

The export happens in Phase 2, before the first page is built. Nothing in
`console/src/` may reference a value not defined here.

Planned layout:

```
design/
├── tokens.css        # CSS custom properties
├── tokens.json       # same values, for tooling
└── primitives/       # Button, Input, Table, Dialog, ...
```
