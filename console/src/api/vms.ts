// React Query hooks for VMs and their disks, media, and snapshots.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api, unwrap, type Schemas } from './client';
import { isTransitional } from './zones';

export type Vm = Schemas['Vm'];
export type VmCreate = Schemas['VmCreate'];
export type VmUpdate = Schemas['VmUpdate'];
export type VmDisk = Schemas['VmDisk'];
export type VmDiskSpec = Schemas['VmDiskSpec'];
export type VmCdrom = Schemas['VmCdrom'];
export type VmSnapshot = Schemas['VmSnapshot'];
export type Bootrom = Schemas['Bootrom'];

export const vmKeys = {
  all: ['vms'] as const,
  list: ['vms', 'list'] as const,
  one: (id: string) => ['vms', 'one', id] as const,
  snapshots: (id: string) => ['vms', 'snapshots', id] as const,
};

function useInvalidateVms() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: vmKeys.all });
}

export function useVms() {
  return useQuery({
    queryKey: vmKeys.list,
    queryFn: () => unwrap(api.GET('/vms', { params: { query: { limit: 500 } } })),
    refetchInterval: (query) =>
      query.state.data?.items.some((v) => isTransitional(v.state)) ? 3_000 : 30_000,
  });
}

export function useVm(id: string) {
  return useQuery({
    queryKey: vmKeys.one(id),
    queryFn: () => unwrap(api.GET('/vms/{id}', { params: { path: { id } } })),
    refetchInterval: (query) =>
      query.state.data && isTransitional(query.state.data.state) ? 3_000 : 30_000,
  });
}

export function useCreateVm() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: (body: VmCreate) => unwrap(api.POST('/vms', { body })),
    onSuccess: invalidate,
  });
}

export function useUpdateVm() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: VmUpdate }) =>
      unwrap(api.PATCH('/vms/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDeleteVm() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, purge }: { id: string; purge: boolean }) =>
      unwrap(api.DELETE('/vms/{id}', { params: { path: { id }, query: { purge } } })),
    onSuccess: invalidate,
  });
}

export type VmAction = 'start' | 'stop' | 'restart' | 'reset';

export function useVmAction() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, action, force }: { id: string; action: VmAction; force?: boolean }) => {
      const params = { params: { path: { id } } };
      switch (action) {
        case 'start':
          return unwrap(api.POST('/vms/{id}/start', params));
        case 'stop':
          return unwrap(api.POST('/vms/{id}/stop', { ...params, body: { force: force ?? false } }));
        case 'restart':
          return unwrap(api.POST('/vms/{id}/restart', params));
        case 'reset':
          return unwrap(api.POST('/vms/{id}/reset', params));
      }
    },
    onSuccess: invalidate,
  });
}

// ------------------------------------------------------------ disks and media

export function useAddDisk() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: Schemas['VmDiskAdd'] }) =>
      unwrap(api.POST('/vms/{id}/disks', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useResizeDisk() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, index, sizeBytes }: { id: string; index: number; sizeBytes: number }) =>
      unwrap(
        api.PATCH('/vms/{id}/disks/{index}', {
          params: { path: { id, index } },
          body: { size_bytes: sizeBytes },
        }),
      ),
    onSuccess: invalidate,
  });
}

export function useRemoveDisk() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, index, purge }: { id: string; index: number; purge: boolean }) =>
      unwrap(
        api.DELETE('/vms/{id}/disks/{index}', {
          params: { path: { id, index }, query: { purge } },
        }),
      ),
    onSuccess: invalidate,
  });
}

export function useAttachCdrom() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, imageId }: { id: string; imageId: string }) =>
      unwrap(
        api.POST('/vms/{id}/cdroms', { params: { path: { id } }, body: { image_id: imageId } }),
      ),
    onSuccess: invalidate,
  });
}

export function useDetachCdrom() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, index }: { id: string; index: number }) =>
      unwrap(api.DELETE('/vms/{id}/cdroms/{index}', { params: { path: { id, index } } })),
    onSuccess: invalidate,
  });
}

// ------------------------------------------------------------ snapshots

export function useVmSnapshots(id: string) {
  return useQuery({
    queryKey: vmKeys.snapshots(id),
    queryFn: () => unwrap(api.GET('/vms/{id}/snapshots', { params: { path: { id } } })),
  });
}

export function useCreateVmSnapshot() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: Schemas['VmSnapshotCreate'] }) =>
      unwrap(api.POST('/vms/{id}/snapshots', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDeleteVmSnapshot() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, snapshot }: { id: string; snapshot: string }) =>
      unwrap(api.DELETE('/vms/{id}/snapshots/{snapshot}', { params: { path: { id, snapshot } } })),
    onSuccess: invalidate,
  });
}

export function useRollbackVmSnapshot() {
  const invalidate = useInvalidateVms();
  return useMutation({
    mutationFn: ({ id, snapshot }: { id: string; snapshot: string }) =>
      unwrap(
        api.POST('/vms/{id}/snapshots/{snapshot}/rollback', {
          params: { path: { id, snapshot } },
        }),
      ),
    onSuccess: invalidate,
  });
}
