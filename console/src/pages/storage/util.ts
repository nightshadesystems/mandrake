// Storage-specific helpers; the metadata and error helpers are shared.

export { emptyMetadata, metadataBody, problem, type MetadataDraft } from '../common/util.ts';

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
