// React Query hooks for the image catalogue: imported images, what the
// sources offer, and the sources themselves.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api, unwrap, type Schemas } from './client';

export type Image = Schemas['Image'];
export type ImageType = Schemas['ImageType'];
export type ImageState = Schemas['ImageState'];
export type ImageImport = Schemas['ImageImport'];
export type CatalogueEntry = Schemas['CatalogueEntry'];
export type ImageSource = Schemas['ImageSource'];
export type ImageSourceCreate = Schemas['ImageSourceCreate'];
export type ImageSourceUpdate = Schemas['ImageSourceUpdate'];
export type Metadata = Schemas['Metadata'];

export const imageKeys = {
  all: ['images'] as const,
  list: ['images', 'list'] as const,
  available: ['images', 'available'] as const,
  sources: ['images', 'sources'] as const,
};

function useInvalidateImages() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: imageKeys.all });
}

const IN_FLIGHT: ImageState[] = ['pending', 'downloading', 'verifying', 'importing'];

export function isInFlight(state: ImageState): boolean {
  return IN_FLIGHT.includes(state);
}

export function useImages() {
  return useQuery({
    queryKey: imageKeys.list,
    queryFn: () => unwrap(api.GET('/images', { params: { query: { limit: 500 } } })),
    // Poll while an import runs so progress moves even without events.
    refetchInterval: (query) =>
      query.state.data?.items.some((i) => isInFlight(i.state)) ? 2_000 : 30_000,
  });
}

export function useAvailable() {
  return useQuery({
    queryKey: imageKeys.available,
    queryFn: () => unwrap(api.GET('/images/available')),
  });
}

export function useSources() {
  return useQuery({
    queryKey: imageKeys.sources,
    queryFn: () => unwrap(api.GET('/images/sources')),
  });
}

export function useImportImage() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: (body: ImageImport) => unwrap(api.POST('/images/import', { body })),
    onSuccess: invalidate,
  });
}

export function useUpdateImage() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: Metadata }) =>
      unwrap(api.PATCH('/images/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDeleteImage() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE('/images/{id}', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}

export function useCreateSource() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: (body: ImageSourceCreate) => unwrap(api.POST('/images/sources', { body })),
    onSuccess: invalidate,
  });
}

export function useUpdateSource() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: ImageSourceUpdate }) =>
      unwrap(api.PATCH('/images/sources/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDeleteSource() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE('/images/sources/{id}', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}

export function useRefreshSource() {
  const invalidate = useInvalidateImages();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.POST('/images/sources/{id}/refresh', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}
