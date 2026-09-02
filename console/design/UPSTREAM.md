# Nightshade Clarity — Design System

A complete replication of **VMware's Clarity Design System** re-skinned as **Nightshade Systems**, with **both dark and light themes**. Clarity's component anatomy, density metrics, and token architecture are preserved verbatim (px values lifted from `ng-clarity` source); every color resolves to the Nightshade palette — violet `#A78BE8` on near-neutral dark grey `#18181C` — and Clarity's action-blue slot is remapped to Nightshade violet.

Nightshade Systems builds **network security infrastructure**: a firewall/edge-inspection platform with a browser console for policy authoring, traffic inspection, threat events, and fleet administration. Clarity's enterprise-console DNA (datagrids, wizards, vertical nav, stack views) fits that surface exactly.

## Sources & inputs

- **Nightshade logo & icon kit** (user-supplied, `assets/BRAND-KIT-README.txt`) — brand colors, Archivo + IBM Plex Mono typography, 14 logo SVGs, 30 firewall icon SVGs, clear-space rules.
- **github.com/vmware-clarity/ng-clarity** — component inventory, token architecture (`styles/core/tokens/*`, `theme.dark.scss`), density metrics (`_properties.density.scss`), component SCSS. All structural values come from here.
- **github.com/vmware-clarity/starters**, **clarity.design** — reference only. Clarity is MIT-licensed by Broadcom/VMware.
- **github.com/nightshadesystems/design-system** — earlier brand repo; superseded by the uploaded kit for color and type.

## Themes

Dark is the default (`:root`). Light is a single-selector scope:

```html
<html data-theme="light">   <!-- omit, or data-theme="dark", for dark -->
```

Every component reads the same `--cds-alias-*` tokens, so nothing else changes. The app header and login brand panel stay dark ink in both themes (Clarity's own convention). The UI kit ships a header toggle that persists to `localStorage`.

## Index

| Path | Purpose |
|---|---|
| `readme.md` | This document |
| `SKILL.md` | Agent-Skill manifest |
| `styles.css` | Global CSS entry — imports everything below |
| `tokens/` | `colors.css` (both themes), `typography.css`, `spacing.css`, `motion.css`, `fonts.css` |
| `clarity/` | The Clarity class framework (`.btn`, `.datagrid`, `.modal`, …), Nightshade-skinned |
| `components/<family>/` | React components — one directory per Clarity family |
| `guidelines/` | Foundation specimen cards for the Design System tab |
| `assets/logos/` | 14 Nightshade logo SVGs · `assets/icons/` 30 brand glyph SVGs · `assets/BRAND-KIT-README.txt` |
| `ui_kits/console/` | "Nightshade Console" — full Clarity app recreation (login, rules datagrid, policy wizard, events, settings) |

### Component families (Clarity inventory, all replicated)

Button & ButtonGroup · Icon · Forms (FormField, Input, Textarea, Select, NumberInput, Password, Checkbox, Radio, Toggle, Range, FileInput, InputGroup) · Combobox & Datalist · DatePicker · Datagrid · Table · TreeView · StackView · Alert (standard + app-level) · Label & Badge · Card (+CardBlock, CardMediaBlock) · Modal & SidePanel · Dropdown · Tooltip · Signpost · Header (+HeaderDropdown, HeaderDivider, HeaderAction) & Subnav · VerticalNav · Tabs · Breadcrumb · Accordion & CollapsiblePanel · Stepper · Timeline · Wizard · ProgressBar, Spinner & Skeleton · Charts (Donut, ChartLegend, ChartStat, BarChart, LineChart)

**Intentional additions:** the **Charts** family, built to the chart anatomy of Clarity-based consoles (donut summary bands, top-X horizontal bars, area line charts) and colored from the viz alias tokens — ng-clarity ships no chart components. Everything else exists in ng-clarity's `_components.clarity.scss` manifest. Clarity's viz tokens are reduced to an 8-series palette + severity colors (full 16-series × 11-step ramps omitted).

## Content Fundamentals

Nightshade writes like a security engineer briefing an operator — precise enough to act on at 3 a.m., never alarmist.

- **Direct, declarative, no fluff.** State what happened, what it means, what to do next. No hedging.
- **Plain technical English.** Real terms — *egress*, *SNI*, *quarantine*, *policy push* — never consumer softeners. Define jargon once.
- **"You" for the operator, "we" for Nightshade.** Never "users" in product copy.
- **Sentence case everywhere** — buttons, labels, titles, nav: "New rule", "Blocked sessions". Title Case only for proper nouns ("Nightshade Edge", "Modbus TCP", "TLS 1.3").
- **ALL CAPS** reserved for dispositions and severities: `ALLOWED`, `BLOCKED`, `QUARANTINED`, `SCANNING`, `TRUSTED`, `WARN`, `CRITICAL`.
- **Numbers always carry units** — `18 ms`, `1.2 Gb/s`, `443/tcp`, `99.99%`. ISO timestamps in the console (`2026-08-15 02:41:08 UTC`); relative time only where freshness is the point ("22 s ago").
- **No emoji, ever.** Unicode sparingly: `•` separator, `→` leads-to, `↗` external link, `×` multiplier.

Examples:
> **Empty state:** "No rules in this zone yet. Add a rule to start enforcing — until then, traffic follows the default deny."
> **Event:** "Repeated SMB probes from 203.0.113.44 — 1 820 attempts in 60 s. Source auto-blocked for 24 h. [Open source] [Acknowledge]"

## Visual Foundations

**Quiet surveillance**: one violet accent over violet-tinted neutrals, carried by Clarity's dense enterprise anatomy.

- **Color** — dark default, light available. Neutrals are near-neutral dark greys carrying a faint violet cast (hue ~255–260 at 4–10% saturation) — grey enough to read as a normal UI, tinted enough to feel like Nightshade. Dark surfaces layer up `#18181C → #1E1E23 → #292930`; header and login-brand panel sit below on `#131316`. Light: `#F5F5F7 → #FFFFFF → #EFEEF3`. One accent: violet `#A78BE8` (dark) / `#7658BC` (light, for contrast on white) signals interactivity, selection, and primary actions. Status hues — info `#6BA5FF`, allowed `#3FD69C`, warn `#F2A93B`, blocked/ember `#FF6A57` — appear only on stateful elements.
- **Type** — **Archivo** everywhere (brand kit's face), **IBM Plex Mono** for values, IPs, ports, timestamps, badges. Wordmark is Archivo Bold at −0.02em; the uppercase variant is SemiBold at 0.14em. Clarity's exact role scale: display 40/44 · headline 32/36 · title 24/32 · section 20/24 · subsection 16/24 · body 14/20 · secondary 13/16 · caption 11/16 · smallcaption 10/12 uppercase. Large roles run light (display/headline 300, title/section 400); UI text 400–600.
- **Spacing** — Clarity's space scale verbatim: 1/2/4/6/8/12/16/18/24/32/36/48/64/72/96. Row heights 16/24/32/36; datagrid rows 32 (compact 24); header 56.
- **Borders** — the defining Clarity trait: **2px strokes** on interactive controls (buttons, inputs, checkboxes, toggles), **1px** on containers. Borders define elevation; shadows stay low-opacity and are reserved for overlays.
- **Radii** — Clarity scale: 2/4/8/12/16. Controls & inputs 4, containers & menus 8, modals 12, pills 999.
- **Backgrounds** — no photography, no gradients. One allowed texture: 24–48px dot grid at ~5% white behind brand/login panels.
- **Motion** — Clarity's timing tokens (0.1–0.4s, `cubic-bezier(0,0.99,0,0.99)` primary); interaction speed: hovers 120ms, transitions 180ms, overlays 260ms. No bounce, no spring.
- **Hover** — surface lightens one ink step; text steps to `#F2F1F5`; accent brightens to `#B9A4F2`. **Press** — accent deepens to `#8F70D6`; no scaling. **Focus** — 2px violet outline, 1px offset, always visible.
- **Selection** — violet-soaked row/nav tint `#292336` (dark) / `#EEEAF9` (light), not a border.
- **Transparency/blur** — backdrop `rgba(12,12,14,0.68)` behind modals only; never on content surfaces.
- **Tooltips invert** (Clarity dark-theme rule): light `#D6D5DC` surface, ink text.
- **Cards** — container fill, 1px border, 8px radius, hairline shadow; clickable cards hover to a violet border.
- **Layout** — Clarity Orchestrator shell: 56px header carrying brand + context dropdowns + icon actions only; primary navigation in the 36px subnav tab row (3px violet underline); 240px vertical nav with flat full-bleed rows, violet group labels, top-right collapse chevron, collapsing to a 48px icon rail; content area 24px padding.

## Iconography

Two sets, with clear jobs:

1. **Clarity Icons — the UI icon system.** Loaded from CDN (both tags required):
   ```html
   <link rel="stylesheet" href="https://unpkg.com/@clr/icons@13.0.2/clr-icons.min.css">
   <script src="https://unpkg.com/@clr/icons@13.0.2/clr-icons.min.js"></script>
   ```
   Usage: `<clr-icon shape="cog" size="16">` or the `Icon` React component. ~500 shapes; variants via `class="is-solid"`, `dir="up|down|left|right"`. Use these for all generic UI affordances — carets, close, search, settings, nav.
2. **Nightshade brand glyphs — `assets/icons/`, 30 domain SVGs** (firewall, policy, rules, tunnel, egress, threat, inspect, status-*). 24px grid, 1.75px stroke, round caps. Copied in and converted to `currentColor`, so tint them with CSS `color`. Use for domain concepts the Clarity set doesn't name. Render stroke glyphs via CSS mask (`-webkit-mask-image` + `background:currentColor`) so they inherit color; `status-*` are filled badges — drop them in as `<img>`.

- **Color:** icons inherit `currentColor`. Inactive `#A2A1AB`, active/selected violet, status icons take their semantic hue.
- **Sizes:** 12 (inline), 16 (default), 20 (nav), 24 (page headers), 32+ (empty states).
- **No emoji as icons. No hand-drawn SVGs.** Dispositions are mono uppercase Labels, not icons.

## Caveats — please flag

1. **Lockup SVGs use live `<text>` in Archivo.** Loaded as `<img>` they can't reach the page's webfont, so cards and the UI kit compose the lockup as *symbol SVG + HTML wordmark* instead — same result, correct type. Outline the text in the SVGs if you need self-contained files.
2. **Icon CDN:** `@clr/icons@13` is the deprecated-but-published UMD build of Clarity Icons — the modern `@cds/core` icons are ESM-only. Pin a local copy if unpkg is a concern.
3. **Metropolis OTFs** from the earlier round remain in `assets/fonts/metropolis/` but are no longer referenced — Archivo replaced them per the brand kit. Delete if you want them gone.
4. Light theme is newly derived (the brand kit specifies dark-leaning values); check contrast on status tints in real screens and tell me what to adjust.
5. Component recreations replicate ng-clarity anatomy, not its Angular behavior (focus traps, virtual scroll, a11y wiring are simplified).
