// React Query hooks for the network family (links, addresses, routes).
// One invalidation for the whole family after any mutation: the topology
// and protection flags depend on all three lists.

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';

import { api, unwrap, type Schemas } from './client';

export type Link = Schemas['Link'];
export type LinkKind = Schemas['LinkKind'];
export type LinkUpdate = Schemas['LinkUpdate'];
export type AggrCreate = Schemas['AggrCreate'];
export type VlanCreate = Schemas['VlanCreate'];
export type VnicCreate = Schemas['VnicCreate'];
export type Address = Schemas['Address'];
export type AddressCreate = Schemas['AddressCreate'];
export type AddressKind = Schemas['AddressKind'];
export type Route = Schemas['Route'];
export type RouteCreate = Schemas['RouteCreate'];

export interface EtherstubCreate {
  name: string;
  metadata?: Schemas['Metadata'];
}

export const networkKeys = {
  all: ['network'] as const,
  links: ['network', 'links'] as const,
  addresses: ['network', 'addresses'] as const,
  routes: ['network', 'routes'] as const,
};

function useInvalidateNetwork() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: networkKeys.all });
}

export function useLinks() {
  return useQuery({
    queryKey: networkKeys.links,
    queryFn: () => unwrap(api.GET('/network/links')),
    refetchInterval: 30_000,
  });
}

export function useAddresses() {
  return useQuery({
    queryKey: networkKeys.addresses,
    queryFn: () => unwrap(api.GET('/network/addresses')),
    refetchInterval: 30_000,
  });
}

export function useRoutes() {
  return useQuery({
    queryKey: networkKeys.routes,
    queryFn: () => unwrap(api.GET('/network/routes')),
    refetchInterval: 30_000,
  });
}

export function useUpdateLink() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: ({ id, body }: { id: string; body: LinkUpdate }) =>
      unwrap(api.PATCH('/network/links/{id}', { params: { path: { id } }, body })),
    onSuccess: invalidate,
  });
}

export function useCreateAggr() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (body: AggrCreate) => unwrap(api.POST('/network/aggrs', { body })),
    onSuccess: invalidate,
  });
}

export function useCreateVlan() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (body: VlanCreate) => unwrap(api.POST('/network/vlans', { body })),
    onSuccess: invalidate,
  });
}

export function useCreateEtherstub() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (body: EtherstubCreate) => unwrap(api.POST('/network/etherstubs', { body })),
    onSuccess: invalidate,
  });
}

export function useCreateVnic() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (body: VnicCreate) => unwrap(api.POST('/network/vnics', { body })),
    onSuccess: invalidate,
  });
}

/** Delete a link through the endpoint for its kind. */
export function useDeleteLink() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: ({ kind, id }: { kind: LinkKind; id: string }) => {
      const params = { params: { path: { id } } };
      switch (kind) {
        case 'aggr':
          return unwrap(api.DELETE('/network/aggrs/{id}', params));
        case 'vlan':
          return unwrap(api.DELETE('/network/vlans/{id}', params));
        case 'etherstub':
          return unwrap(api.DELETE('/network/etherstubs/{id}', params));
        case 'vnic':
          return unwrap(api.DELETE('/network/vnics/{id}', params));
        default:
          return Promise.reject(new Error(`${kind} links cannot be deleted`));
      }
    },
    onSuccess: invalidate,
  });
}

export function useCreateAddress() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (body: AddressCreate) => unwrap(api.POST('/network/addresses', { body })),
    onSuccess: invalidate,
  });
}

export function useDeleteAddress() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE('/network/addresses/{id}', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}

export function useCreateRoute() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (body: RouteCreate) => unwrap(api.POST('/network/routes', { body })),
    onSuccess: invalidate,
  });
}

export function useDeleteRoute() {
  const invalidate = useInvalidateNetwork();
  return useMutation({
    mutationFn: (id: string) =>
      unwrap(api.DELETE('/network/routes/{id}', { params: { path: { id } } })),
    onSuccess: invalidate,
  });
}
