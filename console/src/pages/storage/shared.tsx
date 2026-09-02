// Components every storage tab uses. Plain helpers live in util.ts.

import type { Metadata, Pool } from '../../api/storage.ts';
import { FormField, Input, Label } from '../../design/index.tsx';
import type { MetadataDraft } from './util.ts';

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

export function MetadataFields({
  value,
  onChange,
}: {
  value: MetadataDraft;
  onChange: (next: MetadataDraft) => void;
}) {
  return (
    <>
      <FormField label="Display name" helper="Shown instead of the name where there is room">
        <Input
          value={value.display_name}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            onChange({ ...value, display_name: e.target.value });
          }}
        />
      </FormField>
      <FormField label="Description">
        <Input
          value={value.description}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            onChange({ ...value, description: e.target.value });
          }}
        />
      </FormField>
    </>
  );
}

/** A name with its display name beside it when one is set. */
export function NameCell({
  name,
  metadata,
}: {
  name: string;
  metadata?: Metadata | null | undefined;
}) {
  const display = metadata?.display_name;
  return (
    <span className="name-cell">
      <span className="cell-mono">{name}</span>
      {display && <span className="name-display">{display}</span>}
    </span>
  );
}
