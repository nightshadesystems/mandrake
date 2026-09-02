// Metadata form fields and the name cell, shared by every resource page.

import { FormField, Input } from '../../design/index.tsx';
import type { Metadata, MetadataDraft } from './util.ts';

export function MetadataFields({
  value,
  onChange,
}: {
  value: MetadataDraft;
  onChange: (next: MetadataDraft) => void;
}) {
  return (
    <>
      <FormField label="Display name" helper="Shown beside the name where there is room">
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
