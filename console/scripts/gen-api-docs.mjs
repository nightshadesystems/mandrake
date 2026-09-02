#!/usr/bin/env node
// Render api/openapi.yaml to docs/api.md (ADR-0008).
//
//   node scripts/gen-api-docs.mjs            # write docs/api.md
//   node scripts/gen-api-docs.mjs --check    # exit 1 if docs/api.md is stale
//
// Deliberately small: one Markdown section per tag with its operations,
// then every component schema as a property table. No external renderer.

import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'yaml';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '..', '..');
const specPath = resolve(repo, 'api', 'openapi.yaml');
const outPath = resolve(repo, 'docs', 'api.md');

const spec = parse(readFileSync(specPath, 'utf8'));
const METHODS = ['get', 'post', 'put', 'patch', 'delete'];

const refName = (ref) => ref.split('/').pop();
const code = (s) => '`' + s + '`';
const oneLine = (s) => (s ?? '').trim().replace(/\s+/g, ' ');

function deref(node) {
  if (node && node.$ref) {
    const parts = node.$ref.replace(/^#\//, '').split('/');
    return parts.reduce((acc, key) => acc[key], spec);
  }
  return node;
}

function typeOf(schema) {
  if (!schema) return '';
  if (schema.$ref) return code(refName(schema.$ref));
  if (schema.allOf) return schema.allOf.map(typeOf).join(' + ');
  if (schema.enum) return schema.enum.map((v) => code(String(v))).join(' \\| ');
  if (schema.const !== undefined) return code(String(schema.const));
  const t = Array.isArray(schema.type) ? schema.type.join(' \\| ') : (schema.type ?? 'any');
  if (t === 'array') return `array of ${typeOf(schema.items ?? {})}`;
  return schema.format ? `${t} (${schema.format})` : t;
}

function bodySchema(content) {
  const media = content?.['application/json'] ?? content?.['application/problem+json'];
  return media?.schema ? typeOf(media.schema) : '';
}

const out = [];
const w = (s = '') => out.push(s);

w('# Mandrake API reference');
w();
w('<!-- GENERATED FILE. Do not edit by hand. Regenerate with `just gen-api-docs`');
w('     from api/openapi.yaml (console/scripts/gen-api-docs.mjs). -->');
w();
w(`Version ${spec.info.version}. Source of truth: [api/openapi.yaml](../api/openapi.yaml).`);
w();
w(spec.info.description.trim());
w();

// Operations grouped by tag, in tag order.
const ops = [];
for (const [path, item] of Object.entries(spec.paths)) {
  const shared = item.parameters ?? [];
  for (const m of METHODS) {
    if (item[m])
      ops.push({
        path,
        method: m,
        op: item[m],
        params: [...shared, ...(item[m].parameters ?? [])],
      });
  }
}

w('## Endpoints');
w();
w('| Method | Path | Summary |');
w('|---|---|---|');
for (const { path, method, op } of ops) {
  w(
    `| ${method.toUpperCase()} | ${code(path)} | [${op.summary}](#${op.operationId.toLowerCase()}) |`,
  );
}
w();

for (const tag of spec.tags) {
  const mine = ops.filter(({ op }) => (op.tags ?? []).includes(tag.name));
  if (mine.length === 0) continue;
  w(`## ${tag.name}`);
  w();
  w(tag.description.trim());
  w();
  for (const { path, method, op, params } of mine) {
    w(`### ${op.operationId}`);
    w();
    w(`${code(method.toUpperCase() + ' ' + path)}: ${op.summary}.`);
    w();
    if (op.description) {
      w(op.description.trim());
      w();
    }
    if (op.security && op.security.length === 0) {
      w('No authentication.');
      w();
    }
    if (params.length > 0) {
      w('| Parameter | In | Type | Description |');
      w('|---|---|---|---|');
      for (const raw of params) {
        const p = deref(raw);
        const req = p.required ? ' (required)' : '';
        w(`| ${code(p.name)}${req} | ${p.in} | ${typeOf(p.schema)} | ${oneLine(p.description)} |`);
      }
      w();
    }
    if (op.requestBody) {
      const body = deref(op.requestBody);
      w(`Request body: ${bodySchema(body.content)}`);
      w();
    }
    w('| Status | Body | Description |');
    w('|---|---|---|');
    for (const [status, raw] of Object.entries(op.responses)) {
      const r = deref(raw);
      w(`| ${status} | ${bodySchema(r.content)} | ${oneLine(r.description)} |`);
    }
    w();
  }
}

// Schemas.
w('## Schemas');
w();
for (const [name, schema] of Object.entries(spec.components.schemas)) {
  w(`### ${name}`);
  w();
  if (schema.description) {
    w(oneLine(schema.description));
    w();
  }
  let props = schema.properties;
  let required = schema.required ?? [];
  if (schema.allOf) {
    const parts = schema.allOf.map((s) => (s.$ref ? refName(s.$ref) : null)).filter(Boolean);
    if (parts.length > 0) {
      w(`Extends ${parts.map(code).join(', ')}.`);
      w();
    }
    props = Object.assign({}, ...schema.allOf.map((s) => s.properties ?? {}));
    required = schema.allOf.flatMap((s) => s.required ?? []);
  }
  if (props && Object.keys(props).length > 0) {
    w('| Field | Type | Description |');
    w('|---|---|---|');
    for (const [field, raw] of Object.entries(props)) {
      const req = required.includes(field) ? ' (required)' : '';
      w(`| ${code(field)}${req} | ${typeOf(raw)} | ${oneLine(raw.description)} |`);
    }
    w();
  } else {
    w(`Type: ${typeOf(schema)}`);
    w();
  }
}

const rendered = out.join('\n').replace(/\n{3,}/g, '\n\n') + '\n';

if (process.argv.includes('--check')) {
  const current = readFileSync(outPath, 'utf8');
  if (current !== rendered) {
    console.error('docs/api.md is stale; run `just gen-api-docs`');
    process.exit(1);
  }
  console.log('docs/api.md is up to date');
} else {
  writeFileSync(outPath, rendered);
  console.log(`wrote ${outPath}`);
}
