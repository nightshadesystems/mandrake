// React Query hooks, one per API call the Phase 2 pages need.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api, unwrap, type Schemas } from './client';

export type Session = Schemas['Session'];
export type User = Schemas['User'];
export type UserCreate = Schemas['UserCreate'];
export type UserUpdate = Schemas['UserUpdate'];
export type PasswordChange = Schemas['PasswordChange'];
export type AuditEntry = Schemas['AuditEntry'];
export type SystemInfo = Schemas['SystemInfo'];
export type SystemResources = Schemas['SystemResources'];
export type Event = Schemas['Event'];

export const keys = {
  session: ['session'] as const,
  system: ['system'] as const,
  resources: ['system', 'resources'] as const,
  users: ['users'] as const,
  audit: (filter: AuditFilter) => ['audit', filter] as const,
};

export function useSession() {
  return useQuery({
    queryKey: keys.session,
    queryFn: () => unwrap(api.GET('/auth/session')),
    retry: false,
    staleTime: 60_000,
  });
}

export function useLogin() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: Schemas['LoginRequest']) => unwrap(api.POST('/auth/login', { body })),
    onSuccess: (session) => {
      client.setQueryData(keys.session, session);
    },
  });
}

export function useLogout() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: () => unwrap(api.POST('/auth/logout')),
    onSettled: () => {
      client.clear();
    },
  });
}

export function useSystem() {
  return useQuery({
    queryKey: keys.system,
    queryFn: () => unwrap(api.GET('/system')),
    refetchInterval: 30_000,
  });
}

export function useResources() {
  return useQuery({
    queryKey: keys.resources,
    queryFn: () => unwrap(api.GET('/system/resources')),
    refetchInterval: 5_000,
  });
}

export function useUsers() {
  return useQuery({
    queryKey: keys.users,
    queryFn: () => unwrap(api.GET('/users', { params: { query: { limit: 500 } } })),
  });
}

export function useCreateUser() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (body: UserCreate) => unwrap(api.POST('/users', { body })),
    onSuccess: () => client.invalidateQueries({ queryKey: keys.users }),
  });
}

export function useUpdateUser() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: UserUpdate }) =>
      unwrap(api.PATCH('/users/{id}', { params: { path: { id } }, body })),
    onSuccess: () => client.invalidateQueries({ queryKey: keys.users }),
  });
}

export function useDeleteUser() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE('/users/{id}', { params: { path: { id } } })),
    onSuccess: () => client.invalidateQueries({ queryKey: keys.users }),
  });
}

export function useSetPassword() {
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: PasswordChange }) =>
      unwrap(api.PUT('/users/{id}/password', { params: { path: { id } }, body })),
  });
}

export interface AuditFilter {
  action?: string;
  limit?: number;
  cursor?: string;
}

export function useAudit(filter: AuditFilter) {
  return useQuery({
    queryKey: keys.audit(filter),
    queryFn: () =>
      unwrap(
        api.GET('/audit', {
          params: {
            query: {
              ...(filter.action ? { action: filter.action } : {}),
              ...(filter.cursor ? { cursor: filter.cursor } : {}),
              limit: filter.limit ?? 50,
            },
          },
        }),
      ),
  });
}
