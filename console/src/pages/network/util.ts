// Constants shared by the network tabs and dialogs.

import type { LinkKind } from '../../api/network.ts';

/** Kinds the API can create; physical links are observed only. */
export type CreatableKind = Exclude<LinkKind, 'phys' | 'other'>;

export const KIND_LABEL: Record<LinkKind, string> = {
  phys: 'Physical',
  aggr: 'Aggregation',
  vlan: 'VLAN',
  etherstub: 'Etherstub',
  vnic: 'VNIC',
  other: 'Other',
};
