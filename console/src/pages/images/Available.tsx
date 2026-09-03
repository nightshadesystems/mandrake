import { useState } from 'react';

import { useAvailable, useImportImage, useSources, type CatalogueEntry } from '../../api/images.ts';
import {
  Alert,
  Button,
  Datagrid,
  FormField,
  Input,
  Label,
  Modal,
  Select,
  Spinner,
} from '../../design/index.tsx';
import { bytes, timestamp } from '../../fmt.ts';
import { problem } from '../common/util.ts';
import { TYPE_LABEL } from './util.ts';

function ImportEntryModal({ entry, onClose }: { entry: CatalogueEntry; onClose: () => void }) {
  const importImage = useImportImage();
  const [pool, setPool] = useState('');
  return (
    <Modal
      open
      size="sm"
      title={`Import ${entry.name}@${entry.version}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={importImage.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            loading={importImage.isPending}
            onClick={() => {
              importImage.mutate(
                {
                  source_id: entry.source_id,
                  name: entry.name,
                  version: entry.version,
                  ...(pool.trim() ? { pool: pool.trim() } : {}),
                },
                { onSuccess: onClose },
              );
            }}
          >
            Import
          </Button>
        </>
      }
    >
      <div className="form-stack">
        {importImage.error && (
          <Alert status="danger" sm>
            {problem(importImage.error)}
          </Alert>
        )}
        <dl className="kv">
          <dt>Type</dt>
          <dd>{TYPE_LABEL[entry.type]}</dd>
          <dt>Size</dt>
          <dd className="mono">{bytes(entry.size_bytes)}</dd>
          <dt>Source</dt>
          <dd>{entry.source_name}</dd>
          <dt>URL</dt>
          <dd className="mono">{entry.url}</dd>
        </dl>
        <FormField label="Pool" helper="Empty: the data pool with the most free space">
          <Input
            value={pool}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setPool(e.target.value);
            }}
          />
        </FormField>
      </div>
    </Modal>
  );
}

export function Available({ canWrite }: { canWrite: boolean }) {
  const available = useAvailable();
  const sources = useSources();
  const [source, setSource] = useState('');
  const [importing, setImporting] = useState<CatalogueEntry | null>(null);
  const verified = new Set(sources.data?.items.filter((s) => s.verified).map((s) => s.id) ?? []);
  const rows = (available.data?.items ?? []).filter((e) => source === '' || e.source_id === source);

  return (
    <>
      <div className="toolbar">
        <Select
          value={source}
          options={[
            { value: '', label: 'All sources' },
            ...(sources.data?.items.map((s) => ({ value: s.id, label: s.name })) ?? []),
          ]}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
            setSource(e.target.value);
          }}
        />
        <span className="spacer" />
      </div>
      {available.isError && (
        <Alert status="danger" closable>
          {problem(available.error)}
        </Alert>
      )}
      {available.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<CatalogueEntry>
          rows={rows}
          placeholder="Nothing offered. Add a source, or refresh one under Sources."
          footerText={`${String(rows.length)} available`}
          columns={[
            {
              key: 'name',
              label: 'Image',
              sortable: true,
              render: (e) => <span className="cell-mono">{`${e.name}@${e.version}`}</span>,
            },
            { key: 'type', label: 'Type', sortable: true, render: (e) => TYPE_LABEL[e.type] },
            { key: 'os', label: 'OS', render: (e) => e.os ?? '-' },
            { key: 'description', label: 'Description', render: (e) => e.description ?? '' },
            {
              key: 'size_bytes',
              label: 'Size',
              sortable: true,
              render: (e) => <span className="cell-mono">{bytes(e.size_bytes)}</span>,
            },
            {
              key: 'published_at',
              label: 'Published',
              sortable: true,
              render: (e) => <span className="cell-mono">{timestamp(e.published_at)}</span>,
            },
            {
              key: 'source_name',
              label: 'Source',
              sortable: true,
              render: (e) => (
                <span className="name-cell">
                  {e.source_name}
                  {!verified.has(e.source_id) && <Label status="warning">UNVERIFIED</Label>}
                </span>
              ),
            },
            {
              key: 'imported',
              label: '',
              width: 120,
              render: (e) =>
                e.imported ? (
                  <Label status="success">IMPORTED</Label>
                ) : canWrite && verified.has(e.source_id) ? (
                  <Button
                    sm
                    icon="download"
                    onClick={() => {
                      setImporting(e);
                    }}
                  >
                    Import
                  </Button>
                ) : null,
            },
          ]}
        />
      )}
      {importing && (
        <ImportEntryModal
          entry={importing}
          onClose={() => {
            setImporting(null);
          }}
        />
      )}
    </>
  );
}
