// Helpers every storage tab uses: error text, size parsing, metadata
// drafts. Components live in shared.tsx so fast refresh stays intact.

import { ApiError } from '../../api/client.ts';
import type { Metadata } from '../../api/storage.ts';

export function problem(error: unknown): string {
  return error instanceof ApiError ? error.message : 'Request failed.';
}

const UNITS: Record<string, number> = {
  k: 1024,
  m: 1024 ** 2,
  g: 1024 ** 3,
  t: 1024 ** 4,
  p: 1024 ** 5,
};

/** `10G`, `512MiB`, `1.5T`, or plain bytes to bytes; undefined when not a size. */
export function parseSize(text: string): number | undefined {
  const m = /^\s*(\d+(?:\.\d+)?)\s*([kmgtp])?(?:i?b)?\s*$/i.exec(text);
  if (!m) return undefined;
  const value = Number(m[1] ?? '');
  const unit = (m[2] ?? '').toLowerCase();
  const mult = unit === '' ? 1 : (UNITS[unit] ?? 1);
  return Math.round(value * mult);
}

/** A size field: empty means unset, otherwise it must parse. */
export function sizeFieldError(text: string): string | undefined {
  if (text.trim() === '') return undefined;
  return parseSize(text) === undefined ? 'Use a size such as 10G, 512M, or bytes' : undefined;
}

export interface MetadataDraft {
  display_name: string;
  description: string;
}

export function emptyMetadata(m?: Metadata | null): MetadataDraft {
  return { display_name: m?.display_name ?? '', description: m?.description ?? '' };
}

/** The draft as a request body, or undefined when nothing is set. */
export function metadataBody(d: MetadataDraft): Metadata | undefined {
  const body: Metadata = {
    ...(d.display_name.trim() ? { display_name: d.display_name.trim() } : {}),
    ...(d.description.trim() ? { description: d.description.trim() } : {}),
  };
  return Object.keys(body).length > 0 ? body : undefined;
}
