import { useState, type SyntheticEvent } from 'react';

import {
  useCreateDataset,
  useDatasets,
  useDestroyDataset,
  usePools,
  useUpdateDataset,
  type Dataset,
  type DatasetCreate,
  type DatasetKind,
  type DatasetUpdate,
} from '../../api/storage.ts';
import {
  Alert,
  Button,
  Checkbox,
  Datagrid,
  Dropdown,
  type DropdownItem,
  FormField,
  Input,
  Label,
  Modal,
  Select,
  Spinner,
} from '../../design/index.tsx';
import { bytes, timestamp } from '../../fmt.ts';
import { MetadataFields, NameCell } from './shared.tsx';
import { emptyMetadata, metadataBody, parseSize, problem, sizeFieldError } from './util.ts';

const COMPRESSION = ['inherit', 'lz4', 'zstd', 'gzip', 'off'];
const RECORDSIZE = ['inherit', '4K', '8K', '16K', '32K', '64K', '128K', '256K', '512K', '1M'];
const VOLBLOCKSIZE = ['inherit', '4K', '8K', '16K', '32K', '64K', '128K'];

function validDatasetName(name: string): boolean {
  return /^[a-zA-Z][a-zA-Z0-9_.:-]*(\/[a-zA-Z0-9_.:-]+)+$/.test(name);
}

function sizeOrUndefined(text: string): number | undefined {
  return text.trim() === '' ? undefined : parseSize(text);
}

// ------------------------------------------------------------ create

function CreateDatasetModal({
  kind,
  pools,
  parent,
  onClose,
}: {
  kind: DatasetKind;
  pools: string[];
  parent?: string;
  onClose: () => void;
}) {
  const create = useCreateDataset();
  const volume = kind === 'volume';
  const [name, setName] = useState(parent ? `${parent}/` : (pools[0] ?? '') + '/');
  const [volsize, setVolsize] = useState('');
  const [volblocksize, setVolblocksize] = useState('inherit');
  const [sparse, setSparse] = useState(false);
  const [compression, setCompression] = useState('inherit');
  const [recordsize, setRecordsize] = useState('inherit');
  const [quota, setQuota] = useState('');
  const [reservation, setReservation] = useState('');
  const [mountpoint, setMountpoint] = useState('');
  const [atime, setAtime] = useState(true);
  const [createParents, setCreateParents] = useState(false);
  const [meta, setMeta] = useState(emptyMetadata());

  const volsizeError =
    volume && volsize.trim() === '' ? 'Volumes need a size' : sizeFieldError(volsize);
  const quotaError = sizeFieldError(quota);
  const reservationError = sizeFieldError(reservation);
  const valid =
    validDatasetName(name) &&
    volsizeError === undefined &&
    quotaError === undefined &&
    reservationError === undefined;

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    const q = sizeOrUndefined(quota);
    const r = sizeOrUndefined(reservation);
    const vs = sizeOrUndefined(volsize);
    const vbs = volblocksize === 'inherit' ? undefined : parseSize(volblocksize);
    const rs = recordsize === 'inherit' ? undefined : parseSize(recordsize);
    const body: DatasetCreate = {
      name,
      kind,
      create_parents: createParents,
      ...(compression !== 'inherit' ? { compression } : {}),
      ...(q !== undefined ? { quota_bytes: q } : {}),
      ...(r !== undefined ? { reservation_bytes: r } : {}),
      ...(volume && vs !== undefined ? { volsize_bytes: vs, sparse } : {}),
      ...(volume && vbs !== undefined ? { volblocksize_bytes: vbs } : {}),
      ...(!volume && rs !== undefined ? { recordsize_bytes: rs } : {}),
      ...(!volume && mountpoint.trim() ? { mountpoint: mountpoint.trim() } : {}),
      ...(!volume && !atime ? { atime: false } : {}),
      ...(metadata ? { metadata } : {}),
    };
    create.mutate(body, { onSuccess: onClose });
  };

  return (
    <Modal
      open
      title={volume ? 'New volume' : 'New dataset'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="dataset-form"
            type="submit"
            loading={create.isPending}
            disabled={!valid}
          >
            {volume ? 'Create volume' : 'Create dataset'}
          </Button>
        </>
      }
    >
      <form id="dataset-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <FormField
          label="Name"
          required
          helper="pool/path; the pool must exist"
          {...(name && !validDatasetName(name) ? { error: 'Not a valid dataset name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        {volume ? (
          <>
            <div className="form-row">
              <FormField
                label="Size"
                required
                helper="10G, 512M, ..."
                {...(volsize && volsizeError ? { error: volsizeError } : {})}
              >
                <Input
                  value={volsize}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                    setVolsize(e.target.value);
                  }}
                />
              </FormField>
              <FormField label="Block size">
                <Select
                  value={volblocksize}
                  options={VOLBLOCKSIZE}
                  onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                    setVolblocksize(e.target.value);
                  }}
                />
              </FormField>
            </div>
            <Checkbox
              label="Sparse (thin provisioned, no reservation)"
              checked={sparse}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setSparse(e.target.checked);
              }}
            />
          </>
        ) : (
          <>
            <div className="form-row">
              <FormField label="Mountpoint" helper="Default: inherited from the parent">
                <Input
                  value={mountpoint}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                    setMountpoint(e.target.value);
                  }}
                />
              </FormField>
              <FormField label="Record size">
                <Select
                  value={recordsize}
                  options={RECORDSIZE}
                  onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                    setRecordsize(e.target.value);
                  }}
                />
              </FormField>
            </div>
            <Checkbox
              label="Update access times (atime)"
              checked={atime}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setAtime(e.target.checked);
              }}
            />
          </>
        )}
        <div className="form-row">
          <FormField label="Compression">
            <Select
              value={compression}
              options={COMPRESSION}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setCompression(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Quota" {...(quotaError ? { error: quotaError } : {})}>
            <Input
              value={quota}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setQuota(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Reservation" {...(reservationError ? { error: reservationError } : {})}>
            <Input
              value={reservation}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setReservation(e.target.value);
              }}
            />
          </FormField>
        </div>
        <Checkbox
          label="Create missing parents"
          checked={createParents}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setCreateParents(e.target.checked);
          }}
        />
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------ edit

function EditDatasetModal({ dataset, onClose }: { dataset: Dataset; onClose: () => void }) {
  const update = useUpdateDataset();
  const volume = dataset.kind === 'volume';
  const [quota, setQuota] = useState(dataset.quota_bytes ? String(dataset.quota_bytes) : '');
  const [reservation, setReservation] = useState(
    dataset.reservation_bytes ? String(dataset.reservation_bytes) : '',
  );
  const [volsize, setVolsize] = useState(
    dataset.volsize_bytes ? String(dataset.volsize_bytes) : '',
  );
  const [compression, setCompression] = useState(dataset.compression ?? 'inherit');
  const [mountpoint, setMountpoint] = useState(dataset.mountpoint ?? '');
  const [atime, setAtime] = useState(dataset.atime ?? true);
  const [meta, setMeta] = useState(emptyMetadata(dataset.metadata));
  const quotaError = sizeFieldError(quota);
  const reservationError = sizeFieldError(reservation);
  const volsizeError = sizeFieldError(volsize);
  const valid =
    quotaError === undefined && reservationError === undefined && volsizeError === undefined;

  /** Absent when unchanged, null when cleared, a number when set. */
  const sizePatch = (
    text: string,
    current: number | null | undefined,
  ): number | null | undefined => {
    const next = sizeOrUndefined(text);
    if (next === undefined) return current ? null : undefined;
    return next === current ? undefined : next;
  };

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const q = sizePatch(quota, dataset.quota_bytes);
    const r = sizePatch(reservation, dataset.reservation_bytes);
    const vs = sizeOrUndefined(volsize);
    const metadata = metadataBody(meta);
    const body: DatasetUpdate = {
      ...(q !== undefined ? { quota_bytes: q } : {}),
      ...(r !== undefined ? { reservation_bytes: r } : {}),
      ...(compression !== (dataset.compression ?? 'inherit') && compression !== 'inherit'
        ? { compression }
        : {}),
      ...(volume && vs !== undefined && vs !== dataset.volsize_bytes ? { volsize_bytes: vs } : {}),
      ...(!volume && mountpoint.trim() && mountpoint.trim() !== dataset.mountpoint
        ? { mountpoint: mountpoint.trim() }
        : {}),
      ...(!volume && atime !== (dataset.atime ?? true) ? { atime } : {}),
      ...(metadata ? { metadata } : {}),
    };
    update.mutate({ id: dataset.id, body }, { onSuccess: onClose });
  };

  return (
    <Modal
      open
      title={`Edit ${dataset.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={update.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="dataset-edit"
            type="submit"
            loading={update.isPending}
            disabled={!valid}
          >
            Save
          </Button>
        </>
      }
    >
      <form id="dataset-edit" className="form-stack" onSubmit={submit}>
        {update.error && (
          <Alert status="danger" sm>
            {problem(update.error)}
          </Alert>
        )}
        {dataset.protected && (
          <Alert status="info" sm>
            This dataset is protected; only its metadata can change.
          </Alert>
        )}
        {!dataset.protected && (
          <>
            {volume ? (
              <FormField
                label="Size"
                helper="Volumes only grow"
                {...(volsizeError ? { error: volsizeError } : {})}
              >
                <Input
                  value={volsize}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                    setVolsize(e.target.value);
                  }}
                />
              </FormField>
            ) : (
              <>
                <FormField label="Mountpoint">
                  <Input
                    value={mountpoint}
                    onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                      setMountpoint(e.target.value);
                    }}
                  />
                </FormField>
                <Checkbox
                  label="Update access times (atime)"
                  checked={atime}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                    setAtime(e.target.checked);
                  }}
                />
              </>
            )}
            <div className="form-row">
              <FormField label="Compression">
                <Select
                  value={compression}
                  options={
                    COMPRESSION.includes(compression) ? COMPRESSION : [compression, ...COMPRESSION]
                  }
                  onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                    setCompression(e.target.value);
                  }}
                />
              </FormField>
              <FormField
                label="Quota"
                helper="Empty removes it"
                {...(quotaError ? { error: quotaError } : {})}
              >
                <Input
                  value={quota}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                    setQuota(e.target.value);
                  }}
                />
              </FormField>
              <FormField
                label="Reservation"
                helper="Empty removes it"
                {...(reservationError ? { error: reservationError } : {})}
              >
                <Input
                  value={reservation}
                  onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                    setReservation(e.target.value);
                  }}
                />
              </FormField>
            </div>
          </>
        )}
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------ destroy

function DestroyDatasetModal({ dataset, onClose }: { dataset: Dataset; onClose: () => void }) {
  const destroy = useDestroyDataset();
  const [recursive, setRecursive] = useState(false);
  return (
    <Modal
      open
      size="sm"
      title={`Destroy ${dataset.name}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={destroy.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={destroy.isPending}
            onClick={() => {
              destroy.mutate({ id: dataset.id, recursive }, { onSuccess: onClose });
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
        <p>
          {bytes(dataset.used_bytes)} in use. Without the recursive option the destroy is refused
          while children or snapshots exist.
        </p>
        <Checkbox
          label="Also destroy every child dataset and snapshot"
          checked={recursive}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setRecursive(e.target.checked);
          }}
        />
      </div>
    </Modal>
  );
}

// ------------------------------------------------------------ list

export function Datasets({ kind, canWrite }: { kind: DatasetKind; canWrite: boolean }) {
  const pools = usePools();
  const [pool, setPool] = useState('');
  const datasets = useDatasets({ kind, ...(pool ? { pool } : {}) });
  const [creating, setCreating] = useState<{ parent?: string } | null>(null);
  const [editing, setEditing] = useState<Dataset | null>(null);
  const [destroying, setDestroying] = useState<Dataset | null>(null);
  const volume = kind === 'volume';
  const poolNames = pools.data?.items.map((p) => p.name) ?? [];
  const rows = datasets.data?.items ?? [];

  return (
    <>
      <div className="toolbar">
        <Select
          value={pool}
          options={[{ value: '', label: 'All pools' }, ...poolNames]}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
            setPool(e.target.value);
          }}
        />
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="plus-circle"
            disabled={poolNames.length === 0}
            onClick={() => {
              setCreating({});
            }}
          >
            {volume ? 'New volume' : 'New dataset'}
          </Button>
        )}
      </div>
      {datasets.isError && (
        <Alert status="danger" closable>
          {problem(datasets.error)}
        </Alert>
      )}
      {datasets.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Dataset>
          rows={rows}
          placeholder={volume ? 'No volumes.' : 'No datasets.'}
          footerText={`${String(rows.length)} ${volume ? 'volumes' : 'datasets'}`}
          columns={[
            {
              key: 'name',
              label: 'Name',
              sortable: true,
              render: (d) => (
                <span className="name-cell">
                  <NameCell name={d.name} metadata={d.metadata} />
                  {d.protected && <Label>PROTECTED</Label>}
                </span>
              ),
            },
            ...(volume
              ? [
                  {
                    key: 'volsize_bytes',
                    label: 'Size',
                    sortable: true,
                    render: (d: Dataset) => (
                      <span className="cell-mono">
                        {d.volsize_bytes ? bytes(d.volsize_bytes) : '-'}
                      </span>
                    ),
                  },
                ]
              : [
                  {
                    key: 'mountpoint',
                    label: 'Mountpoint',
                    render: (d: Dataset) => (
                      <span className="cell-mono">
                        {d.mountpoint ?? '-'}
                        {d.mountpoint && !d.mounted ? ' (not mounted)' : ''}
                      </span>
                    ),
                  },
                ]),
            {
              key: 'used_bytes',
              label: 'Used',
              sortable: true,
              render: (d) => <span className="cell-mono">{bytes(d.used_bytes)}</span>,
            },
            {
              key: 'available_bytes',
              label: 'Available',
              sortable: true,
              render: (d) => <span className="cell-mono">{bytes(d.available_bytes)}</span>,
            },
            {
              key: 'quota_bytes',
              label: 'Quota / Reservation',
              render: (d) => (
                <span className="cell-mono">
                  {d.quota_bytes ? bytes(d.quota_bytes) : '-'} /{' '}
                  {d.reservation_bytes ? bytes(d.reservation_bytes) : '-'}
                </span>
              ),
            },
            {
              key: 'compression',
              label: 'Compression',
              render: (d) => (
                <span className="cell-mono">
                  {d.compression ?? '-'}
                  {d.compress_ratio !== undefined ? ` (${d.compress_ratio.toFixed(2)}x)` : ''}
                </span>
              ),
            },
            {
              key: 'created_at',
              label: 'Created',
              sortable: true,
              render: (d) => <span className="cell-mono">{timestamp(d.created_at)}</span>,
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (d) => {
                if (!canWrite) return null;
                const items: DropdownItem[] = [
                  {
                    label: 'Edit',
                    icon: 'pencil',
                    onClick: () => {
                      setEditing(d);
                    },
                  },
                ];
                if (!volume) {
                  items.push({
                    label: 'New child dataset',
                    icon: 'plus',
                    onClick: () => {
                      setCreating({ parent: d.name });
                    },
                  });
                }
                items.push({ divider: true });
                items.push({
                  label: 'Destroy',
                  icon: 'trash',
                  disabled: d.protected,
                  onClick: () => {
                    setDestroying(d);
                  },
                });
                return <Dropdown trigger="" variant="link-neutral" sm right items={items} />;
              },
            },
          ]}
        />
      )}
      {creating && (
        <CreateDatasetModal
          kind={kind}
          pools={poolNames}
          {...(creating.parent ? { parent: creating.parent } : {})}
          onClose={() => {
            setCreating(null);
          }}
        />
      )}
      {editing && (
        <EditDatasetModal
          dataset={editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
      {destroying && (
        <DestroyDatasetModal
          dataset={destroying}
          onClose={() => {
            setDestroying(null);
          }}
        />
      )}
    </>
  );
}
