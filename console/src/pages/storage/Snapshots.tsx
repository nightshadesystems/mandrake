import { useState, type SyntheticEvent } from 'react';

import {
  useCloneSnapshot,
  useCreateSnapshot,
  useDatasets,
  useDestroySnapshot,
  useRollbackSnapshot,
  useSnapshots,
  type Snapshot,
} from '../../api/storage.ts';
import {
  Alert,
  Button,
  Checkbox,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Modal,
  Select,
  Spinner,
} from '../../design/index.tsx';
import { bytes, timestamp } from '../../fmt.ts';
import { MetadataFields, NameCell } from './shared.tsx';
import { emptyMetadata, metadataBody, problem } from './util.ts';

function validSnapshotName(name: string): boolean {
  return /^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,254}$/.test(name);
}

function CreateSnapshotModal({
  datasets,
  initial,
  onClose,
}: {
  datasets: string[];
  initial: string;
  onClose: () => void;
}) {
  const create = useCreateSnapshot();
  const [dataset, setDataset] = useState(initial || (datasets[0] ?? ''));
  const [name, setName] = useState(() =>
    new Date().toISOString().slice(0, 16).replace(/[-:T]/g, ''),
  );
  const [recursive, setRecursive] = useState(false);
  const [meta, setMeta] = useState(emptyMetadata());
  const valid = dataset !== '' && validSnapshotName(name);
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      { dataset, name, recursive, ...(metadata ? { metadata } : {}) },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="New snapshot"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="snapshot-form"
            type="submit"
            loading={create.isPending}
            disabled={!valid}
          >
            Create snapshot
          </Button>
        </>
      }
    >
      <form id="snapshot-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <FormField label="Dataset" required>
          <Select
            value={dataset}
            options={datasets}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setDataset(e.target.value);
            }}
          />
        </FormField>
        <FormField
          label="Name"
          required
          helper="The part after @"
          {...(name && !validSnapshotName(name) ? { error: 'Not a valid snapshot name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <Checkbox
          label="Recursive: snapshot every descendant with the same name"
          checked={recursive}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setRecursive(e.target.checked);
          }}
        />
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function RollbackModal({ snapshot, onClose }: { snapshot: Snapshot; onClose: () => void }) {
  const rollback = useRollbackSnapshot();
  const [discardNewer, setDiscardNewer] = useState(false);
  return (
    <Modal
      open
      size="sm"
      title={`Roll back to ${snapshot.short_name}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={rollback.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={rollback.isPending}
            onClick={() => {
              rollback.mutate({ id: snapshot.id, discardNewer }, { onSuccess: onClose });
            }}
          >
            Roll back
          </Button>
        </>
      }
    >
      <div className="form-stack">
        {rollback.error && (
          <Alert status="danger" sm>
            {problem(rollback.error)}
          </Alert>
        )}
        <p>
          <span className="mono">{snapshot.dataset}</span> returns to the state it had at{' '}
          {timestamp(snapshot.created_at)}. Every change since is lost.
        </p>
        <Checkbox
          label="Also destroy snapshots newer than this one"
          checked={discardNewer}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setDiscardNewer(e.target.checked);
          }}
        />
      </div>
    </Modal>
  );
}

function CloneModal({ snapshot, onClose }: { snapshot: Snapshot; onClose: () => void }) {
  const clone = useCloneSnapshot();
  const [name, setName] = useState(`${snapshot.dataset}-${snapshot.short_name}`);
  const valid = /^[a-zA-Z][a-zA-Z0-9_.:-]*(\/[a-zA-Z0-9_.:-]+)+$/.test(name);
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    clone.mutate({ id: snapshot.id, name }, { onSuccess: onClose });
  };
  return (
    <Modal
      open
      title={`Clone ${snapshot.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={clone.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="clone-form"
            type="submit"
            loading={clone.isPending}
            disabled={!valid}
          >
            Clone
          </Button>
        </>
      }
    >
      <form id="clone-form" className="form-stack" onSubmit={submit}>
        {clone.error && (
          <Alert status="danger" sm>
            {problem(clone.error)}
          </Alert>
        )}
        <FormField
          label="New dataset"
          required
          helper="Full name in the same pool; the clone depends on this snapshot"
          {...(name && !valid ? { error: 'Not a valid dataset name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
      </form>
    </Modal>
  );
}

function DestroySnapshotModal({ snapshot, onClose }: { snapshot: Snapshot; onClose: () => void }) {
  const destroy = useDestroySnapshot();
  const clones = snapshot.clones ?? [];
  return (
    <Modal
      open
      size="sm"
      title={`Destroy ${snapshot.name}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={destroy.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={destroy.isPending}
            disabled={clones.length > 0}
            onClick={() => {
              destroy.mutate(snapshot.id, { onSuccess: onClose });
            }}
          >
            Destroy
          </Button>
        </>
      }
    >
      <div className="form-stack">
        {destroy.error && (
          <Alert status="danger" sm>
            {problem(destroy.error)}
          </Alert>
        )}
        {clones.length > 0 ? (
          <p>Refused: clones depend on it ({clones.join(', ')}). Destroy or promote them first.</p>
        ) : (
          <p>{bytes(snapshot.used_bytes)} is freed. This cannot be undone.</p>
        )}
      </div>
    </Modal>
  );
}

export function Snapshots({ canWrite }: { canWrite: boolean }) {
  const datasets = useDatasets({});
  const [dataset, setDataset] = useState('');
  const snapshots = useSnapshots(dataset);
  const [creating, setCreating] = useState(false);
  const [rollingBack, setRollingBack] = useState<Snapshot | null>(null);
  const [cloning, setCloning] = useState<Snapshot | null>(null);
  const [destroying, setDestroying] = useState<Snapshot | null>(null);
  const names = datasets.data?.items.filter((d) => !d.protected).map((d) => d.name) ?? [];
  const allNames = datasets.data?.items.map((d) => d.name) ?? [];
  const rows = snapshots.data?.items ?? [];

  return (
    <>
      <div className="toolbar">
        <Select
          value={dataset}
          options={[{ value: '', label: 'All datasets' }, ...allNames]}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
            setDataset(e.target.value);
          }}
        />
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="camera"
            disabled={names.length === 0}
            onClick={() => {
              setCreating(true);
            }}
          >
            New snapshot
          </Button>
        )}
      </div>
      {snapshots.isError && (
        <Alert status="danger" closable>
          {problem(snapshots.error)}
        </Alert>
      )}
      {snapshots.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Snapshot>
          rows={rows}
          placeholder="No snapshots."
          footerText={`${String(rows.length)} snapshots`}
          columns={[
            {
              key: 'dataset',
              label: 'Dataset',
              sortable: true,
              render: (s) => <span className="cell-mono">{s.dataset}</span>,
            },
            {
              key: 'short_name',
              label: 'Snapshot',
              sortable: true,
              render: (s) => <NameCell name={s.short_name} metadata={s.metadata} />,
            },
            {
              key: 'created_at',
              label: 'Created',
              sortable: true,
              render: (s) => <span className="cell-mono">{timestamp(s.created_at)}</span>,
            },
            {
              key: 'used_bytes',
              label: 'Used',
              sortable: true,
              render: (s) => <span className="cell-mono">{bytes(s.used_bytes)}</span>,
            },
            {
              key: 'referenced_bytes',
              label: 'Referenced',
              render: (s) => <span className="cell-mono">{bytes(s.referenced_bytes)}</span>,
            },
            {
              key: 'clones',
              label: 'Clones',
              render: (s) => (
                <span className="cell-mono">{(s.clones ?? []).join(', ') || '-'}</span>
              ),
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (s) =>
                canWrite ? (
                  <Dropdown
                    trigger=""
                    variant="link-neutral"
                    sm
                    right
                    items={[
                      {
                        label: 'Roll back',
                        icon: 'undo',
                        onClick: () => {
                          setRollingBack(s);
                        },
                      },
                      {
                        label: 'Clone',
                        icon: 'copy',
                        onClick: () => {
                          setCloning(s);
                        },
                      },
                      { divider: true },
                      {
                        label: 'Destroy',
                        icon: 'trash',
                        onClick: () => {
                          setDestroying(s);
                        },
                      },
                    ]}
                  />
                ) : null,
            },
          ]}
        />
      )}
      {creating && (
        <CreateSnapshotModal
          datasets={names}
          initial={dataset}
          onClose={() => {
            setCreating(false);
          }}
        />
      )}
      {rollingBack && (
        <RollbackModal
          snapshot={rollingBack}
          onClose={() => {
            setRollingBack(null);
          }}
        />
      )}
      {cloning && (
        <CloneModal
          snapshot={cloning}
          onClose={() => {
            setCloning(null);
          }}
        />
      )}
      {destroying && (
        <DestroySnapshotModal
          snapshot={destroying}
          onClose={() => {
            setDestroying(null);
          }}
        />
      )}
    </>
  );
}
