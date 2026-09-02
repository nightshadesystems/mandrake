// Storage-specific components; metadata fields and the name cell are shared.

import type { Pool } from '../../api/storage.ts';
import { Label } from '../../design/index.tsx';

export { MetadataFields, NameCell } from '../common/Metadata.tsx';

type Health = Pool['health'];

export function HealthLabel({ health }: { health: Health }) {
  const status =
    health === 'ONLINE' || health === 'AVAIL'
      ? 'success'
      : health === 'DEGRADED' || health === 'INUSE'
        ? 'warning'
        : 'danger';
  return <Label status={status}>{health}</Label>;
}
