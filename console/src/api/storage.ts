// React Query hooks for the storage family (pools, datasets, volumes,
// snapshots, devices). Every mutation invalidates the whole family: the
// daemon's own list cache is two seconds, and one change often moves
// several lists (a pool creates a dataset, a clone is a dataset).

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api, unwrap, type Schemas } from './client';

export type Pool = Schemas['Pool'];
export type PoolCreate = Schemas['PoolCreate'];
export type Vdev = Schemas['Vdev'];
export type VdevSpec = Schemas['VdevSpec'];
export type VdevSpecType = VdevSpec['type'];
export type Device = Schemas['Device'];
export type Dataset = Schemas['Dataset'];
export type DatasetKind = Schemas['DatasetKind'];
export type DatasetCreate = Schemas['DatasetCreate'];
export type DatasetUpdate = Schemas['DatasetUpdate'];
export type Snapshot = Schemas['Snapshot'];
export type SnapshotCreate = Schemas['SnapshotCreate'];
export type Job = Schemas['Job'];
export type Metadata = Schemas['Metadata'];

export const storageKeys = {
  all: ['storage'] as const,
  devices: ['storage', 'devices'] as const,
  pools: ['storage', 'pools'] as const,
  datasets: (pool: string, kind: string) => ['storage', 'datasets', pool, kind] as const,
  snapshots: (dataset: string) => ['storage', 'snapshots', dataset] as const,
};

function useInvalidateStorage() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: storageKeys.all });
}

export function useDevices() {
  return useQuery({
    queryKey: storageKeys.devices,
    queryFn: () => unwrap(api.GET('/storage/devices')),
  });
}

export function usePools() {
  return useQuery({
    queryKey: storageKeys.pools,
    queryFn: () => unwrap(api.GET('/storage/pools', { params: { query: { limit: 500 } } })),
    // Poll faster while a scrub or resilver runs so progress moves.
    refetchInterval: (query) =>
      query.state.data?.items.some((p) => p.scan?.state === 'in_progress') ? 3_000 : 30_000,
  });
}

export function useCreatePool() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: (body: PoolCreate) => unwrap(api.POST('/storage/pools', { body })),
    onSuccess: invalidate,
  });
}

export function useUpdatePool() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: Metadata }) =>
      unwrap(api.PATCH('/storage/pools/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDestroyPool() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      unwrap(api.DELETE('/storage/pools/{id}', { params: { path: { id } }, body: { name } })),
    onSuccess: invalidate,
  });
}

export function useStartScrub() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.POST('/storage/pools/{id}/scrub', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}

export function useStopScrub() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE('/storage/pools/{id}/scrub', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}

export interface DatasetFilter {
  pool?: string;
  kind?: DatasetKind;
}

export function useDatasets(filter: DatasetFilter) {
  return useQuery({
    queryKey: storageKeys.datasets(filter.pool ?? '', filter.kind ?? ''),
    queryFn: () =>
      unwrap(
        api.GET('/storage/datasets', {
          params: {
            query: {
              ...(filter.pool ? { pool: filter.pool } : {}),
              ...(filter.kind ? { kind: filter.kind } : {}),
              limit: 500,
            },
          },
        }),
      ),
  });
}

export function useCreateDataset() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: (body: DatasetCreate) => unwrap(api.POST('/storage/datasets', { body })),
    onSuccess: invalidate,
  });
}

export function useUpdateDataset() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: DatasetUpdate }) =>
      unwrap(api.PATCH('/storage/datasets/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDestroyDataset() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: ({ id, recursive }: { id: string; recursive: boolean }) =>
      unwrap(
        api.DELETE('/storage/datasets/{id}', {
          params: { path: { id }, query: { recursive } },
        }),
      ),
    onSuccess: invalidate,
  });
}

export function useSnapshots(dataset: string) {
  return useQuery({
    queryKey: storageKeys.snapshots(dataset),
    queryFn: () =>
      unwrap(
        api.GET('/storage/snapshots', {
          params: {
            query: { ...(dataset ? { dataset, recursive: true } : {}), limit: 500 },
          },
        }),
      ),
  });
}

export function useCreateSnapshot() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: (body: SnapshotCreate) => unwrap(api.POST('/storage/snapshots', { body })),
    onSuccess: invalidate,
  });
}

export function useDestroySnapshot() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE('/storage/snapshots/{id}', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}

export function useRollbackSnapshot() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: ({ id, discardNewer }: { id: string; discardNewer: boolean }) =>
      unwrap(
        api.POST('/storage/snapshots/{id}/rollback', {
          params: { path: { id } },
          body: { discard_newer: discardNewer },
        }),
      ),
    onSuccess: invalidate,
  });
}

export function useCloneSnapshot() {
  const invalidate = useInvalidateStorage();
  return useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) =>
      unwrap(
        api.POST('/storage/snapshots/{id}/clone', { params: { path: { id } }, body: { name } }),
      ),
    onSuccess: invalidate,
  });
}
