// Helpers shared by every resource page: error text and metadata drafts.

import { ApiError } from '../../api/client.ts';
import type { Schemas } from '../../api/client.ts';

export type Metadata = Schemas['Metadata'];

export function problem(error: unknown): string {
  return error instanceof ApiError ? error.message : 'Request failed.';
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
