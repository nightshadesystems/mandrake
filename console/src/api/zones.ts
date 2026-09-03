// React Query hooks for zones, and the console WebSocket address.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api, unwrap, websocketUrl, type Schemas } from './client';

export type Zone = Schemas['Zone'];
export type ZoneBrand = Schemas['ZoneBrand'];
export type ZoneState = Schemas['ZoneState'];
export type ZoneNic = Schemas['ZoneNic'];
export type ZoneCreate = Schemas['ZoneCreate'];
export type ZoneUpdate = Schemas['ZoneUpdate'];
export type Job = Schemas['Job'];

export const zoneKeys = {
  all: ['zones'] as const,
  list: ['zones', 'list'] as const,
  one: (id: string) => ['zones', 'one', id] as const,
};

const TRANSITIONAL: ZoneState[] = ['configured', 'incomplete', 'ready', 'shutting_down', 'down'];

export function isTransitional(state: ZoneState): boolean {
  return TRANSITIONAL.includes(state);
}

function useInvalidateZones() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: zoneKeys.all });
}

export function useZones() {
  return useQuery({
    queryKey: zoneKeys.list,
    queryFn: () => unwrap(api.GET('/zones', { params: { query: { limit: 500 } } })),
    refetchInterval: (query) =>
      query.state.data?.items.some((z) => isTransitional(z.state)) ? 3_000 : 30_000,
  });
}

export function useZone(id: string) {
  return useQuery({
    queryKey: zoneKeys.one(id),
    queryFn: () => unwrap(api.GET('/zones/{id}', { params: { path: { id } } })),
    refetchInterval: (query) =>
      query.state.data && isTransitional(query.state.data.state) ? 3_000 : 30_000,
  });
}

export function useCreateZone() {
  const invalidate = useInvalidateZones();
  return useMutation({
    mutationFn: (body: ZoneCreate) => unwrap(api.POST('/zones', { body })),
    onSuccess: invalidate,
  });
}

export function useUpdateZone() {
  const invalidate = useInvalidateZones();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: ZoneUpdate }) =>
      unwrap(api.PATCH('/zones/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useDeleteZone() {
  const invalidate = useInvalidateZones();
  return useMutation({
    mutationFn: ({ id, purge }: { id: string; purge: boolean }) =>
      unwrap(api.DELETE('/zones/{id}', { params: { path: { id }, query: { purge } } })),
    onSuccess: invalidate,
  });
}

export type ZoneAction = 'start' | 'stop' | 'restart';

export function useZoneAction() {
  const invalidate = useInvalidateZones();
  return useMutation({
    mutationFn: ({ id, action, force }: { id: string; action: ZoneAction; force?: boolean }) => {
      const params = { params: { path: { id } } };
      switch (action) {
        case 'start':
          return unwrap(api.POST('/zones/{id}/start', params));
        case 'stop':
          return unwrap(
            api.POST('/zones/{id}/stop', { ...params, body: { force: force ?? false } }),
          );
        case 'restart':
          return unwrap(api.POST('/zones/{id}/restart', params));
      }
    },
    onSuccess: invalidate,
  });
}

/** The console WebSocket address for a zone. */
export function consoleUrl(id: string, cols: number, rows: number): string {
  return websocketUrl(`/zones/${encodeURIComponent(id)}/console`, {
    cols: String(cols),
    rows: String(rows),
  });
}
