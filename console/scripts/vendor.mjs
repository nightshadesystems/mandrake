#!/usr/bin/env node
// Copy the Clarity icons UMD bundle into public/vendor/ so index.html can
// load it from the appliance itself (ADR-0008: nothing from a CDN). Run
// before dev and build; public/vendor is not committed.

import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const root = resolve(here, '..');
const src = resolve(root, 'node_modules', '@clr', 'icons');
const dst = resolve(root, 'public', 'vendor');

mkdirSync(dst, { recursive: true });
for (const file of ['clr-icons.min.js', 'clr-icons.min.css']) {
  copyFileSync(resolve(src, file), resolve(dst, file));
}
console.log(`vendored @clr/icons into ${dst}`);
