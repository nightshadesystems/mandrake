import { useState, type SyntheticEvent } from 'react';

import {
  useCreatePool,
  useDestroyPool,
  useDevices,
  usePools,
  useStartScrub,
  useStopScrub,
  useUpdatePool,
  type Device,
  type Pool,
  type Vdev,
  type VdevSpecType,
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
  ProgressBar,
  Select,
  Spinner,
} from '../../design/index.tsx';
import { bytes, timestamp } from '../../fmt.ts';
import { HealthLabel, MetadataFields, NameCell } from './shared.tsx';
import { emptyMetadata, metadataBody, problem } from './util.ts';

const VDEV_TYPES: { value: VdevSpecType; label: string; min: number }[] = [
  { value: 'mirror', label: 'Mirror', min: 2 },
  { value: 'stripe', label: 'Stripe (no redundancy)', min: 1 },
  { value: 'raidz1', label: 'RAID-Z1', min: 3 },
  { value: 'raidz2', label: 'RAID-Z2', min: 4 },
  { value: 'raidz3', label: 'RAID-Z3', min: 5 },
  { value: 'log', label: 'Log (SLOG)', min: 1 },
  { value: 'cache', label: 'Cache (L2ARC)', min: 1 },
  { value: 'spare', label: 'Hot spare', min: 1 },
];
const AUX: VdevSpecType[] = ['log', 'cache', 'spare'];
const COMPRESSION = ['lz4', 'zstd', 'gzip', 'off'];
const ASHIFT = [
  { value: '', label: 'Auto' },
  { value: '9', label: '9 (512 B sectors)' },
  { value: '12', label: '12 (4 KiB sectors)' },
  { value: '13', label: '13 (8 KiB sectors)' },
];

function validPoolName(name: string): boolean {
  return /^[a-zA-Z][a-zA-Z0-9_.-]{0,254}$/.test(name) && name !== 'rpool';
}

// ------------------------------------------------------------ detail

function VdevNode({ vdev, depth }: { vdev: Vdev; depth: number }) {
  const errors = [vdev.read_errors, vdev.write_errors, vdev.checksum_errors];
  const hasErrors = errors.some((e) => (e ?? 0) > 0);
  return (
    <>
      <div className="vdev-row" style={{ paddingLeft: depth * 20 }}>
        <span className="mono">{vdev.name}</span>
        <span className="vdev-type">{vdev.type}</span>
        <HealthLabel health={vdev.state} />
        <span className={hasErrors ? 'mono vdev-errors' : 'mono'}>
          {errors.map((e) => String(e ?? 0)).join(' / ')}
        </span>
        {vdev.note && <span className="vdev-note">{vdev.note}</span>}
      </div>
      {vdev.children.map((c) => (
        <VdevNode key={c.name} vdev={c} depth={depth + 1} />
      ))}
    </>
  );
}

function ScanBlock({ pool }: { pool: Pool }) {
  const scan = pool.scan;
  if (!scan) return null;
  const running = scan.state === 'in_progress';
  return (
    <div className="scan-block">
      <div className="toolbar">
        <strong>{scan.function === 'scrub' ? 'Scrub' : 'Resilver'}</strong>
        <Label status={running ? 'info' : scan.state === 'finished' ? 'success' : 'warning'}>
          {scan.state.replace('_', ' ').toUpperCase()}
        </Label>
        {scan.errors !== undefined && scan.errors > 0 && (
          <Label status="danger">{`${String(scan.errors)} ERRORS`}</Label>
        )}
        <span className="spacer" />
        <span className="mono">
          {running ? `started ${timestamp(scan.started_at)}` : timestamp(scan.finished_at)}
        </span>
      </div>
      {running && (
        <ProgressBar
          value={scan.progress ?? 0}
          max={100}
          showValue
          {...(scan.rate_bytes_per_second !== undefined
            ? { label: `${bytes(scan.rate_bytes_per_second)}/s` }
            : {})}
        />
      )}
      <p className="mono scan-summary">{scan.summary}</p>
    </div>
  );
}

function PoolDetail({ pool }: { pool: Pool }) {
  return (
    <div className="pool-detail">
      <div className="vdev-row vdev-head">
        <span>Device</span>
        <span>Type</span>
        <span>State</span>
        <span>Errors r / w / c</span>
        <span />
      </div>
      <VdevNode vdev={pool.vdevs} depth={0} />
      <ScanBlock pool={pool} />
      {pool.status_text && <p className="status-text">{pool.status_text}</p>}
      {pool.metadata?.description && <p>{pool.metadata.description}</p>}
    </div>
  );
}

// ------------------------------------------------------------ create

interface Group {
  type: VdevSpecType;
  devices: string[];
}

function groupError(group: Group): string | undefined {
  const spec = VDEV_TYPES.find((t) => t.value === group.type);
  if (!spec) return undefined;
  if (group.devices.length < spec.min) {
    return `${spec.label} needs at least ${String(spec.min)} device${spec.min > 1 ? 's' : ''}`;
  }
  return undefined;
}

function CreatePoolModal({ devices, onClose }: { devices: Device[]; onClose: () => void }) {
  const create = useCreatePool();
  const [name, setName] = useState('');
  const [groups, setGroups] = useState<Group[]>([{ type: 'mirror', devices: [] }]);
  const [ashift, setAshift] = useState('');
  const [compression, setCompression] = useState('lz4');
  const [force, setForce] = useState(false);
  const [meta, setMeta] = useState(emptyMetadata());

  const free = devices.filter((d) => !d.pool);
  const taken = new Map<string, number>();
  groups.forEach((g, i) => {
    g.devices.forEach((d) => taken.set(d, i));
  });
  const hasData = groups.some((g) => !AUX.includes(g.type) && g.devices.length > 0);
  const errors = groups.map(groupError);
  const valid = validPoolName(name) && hasData && errors.every((e) => e === undefined);

  const setGroup = (i: number, next: Group) => {
    setGroups(groups.map((g, j) => (j === i ? next : g)));
  };

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      {
        name,
        vdevs: groups
          .filter((g) => g.devices.length > 0)
          .map((g) => ({ type: g.type, devices: g.devices })),
        ...(ashift ? { ashift: Number(ashift) } : {}),
        compression,
        force,
        ...(metadata ? { metadata } : {}),
      },
      { onSuccess: onClose },
    );
  };

  return (
    <Modal
      open
      size="lg"
      title="New pool"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="pool-form"
            type="submit"
            loading={create.isPending}
            disabled={!valid}
          >
            Create pool
          </Button>
        </>
      }
    >
      <form id="pool-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        {free.length === 0 && (
          <Alert status="warning" sm>
            Every disk on this host is already in a pool.
          </Alert>
        )}
        <FormField
          label="Name"
          required
          helper="Letters, digits, _ . -; rpool is reserved"
          {...(name && !validPoolName(name) ? { error: 'Not a valid pool name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <div className="vdev-groups">
          {groups.map((g, i) => (
            <div className="vdev-group" key={String(i)}>
              <div className="toolbar">
                <Select
                  value={g.type}
                  options={VDEV_TYPES.map((t) => ({ value: t.value, label: t.label }))}
                  onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                    setGroup(i, { ...g, type: e.target.value as VdevSpecType });
                  }}
                />
                <span className="spacer" />
                {groups.length > 1 && (
                  <Button
                    variant="link"
                    sm
                    icon="trash"
                    onClick={() => {
                      setGroups(groups.filter((_, j) => j !== i));
                    }}
                  >
                    Remove
                  </Button>
                )}
              </div>
              <div className="device-picker">
                {free.map((d) => {
                  const owner = taken.get(d.name);
                  const elsewhere = owner !== undefined && owner !== i;
                  return (
                    <Checkbox
                      key={d.name}
                      label={`${d.name} · ${bytes(d.size_bytes)}${d.product ? ` · ${d.product}` : ''}`}
                      checked={owner === i}
                      disabled={elsewhere}
                      onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                        const next = e.target.checked
                          ? [...g.devices, d.name]
                          : g.devices.filter((x) => x !== d.name);
                        setGroup(i, { ...g, devices: next });
                      }}
                    />
                  );
                })}
              </div>
              {errors[i] && g.devices.length > 0 && <p className="field-error">{errors[i]}</p>}
            </div>
          ))}
          <div>
            <Button
              sm
              icon="plus"
              onClick={() => {
                setGroups([...groups, { type: 'mirror', devices: [] }]);
              }}
            >
              Add vdev
            </Button>
          </div>
        </div>
        <div className="form-row">
          <FormField label="ashift">
            <Select
              value={ashift}
              options={ASHIFT}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setAshift(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Compression">
            <Select
              value={compression}
              options={COMPRESSION}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setCompression(e.target.value);
              }}
            />
          </FormField>
        </div>
        <Checkbox
          label="Force: overwrite disks that carry a foreign label or old pool data"
          checked={force}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setForce(e.target.checked);
          }}
        />
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

// ------------------------------------------------------------ edit / destroy

function EditPoolModal({ pool, onClose }: { pool: Pool; onClose: () => void }) {
  const update = useUpdatePool();
  const [meta, setMeta] = useState(emptyMetadata(pool.metadata));
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    update.mutate({ id: pool.id, body: metadataBody(meta) ?? {} }, { onSuccess: onClose });
  };
  return (
    <Modal
      open
      title={`Edit ${pool.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={update.isPending}>
            Cancel
          </Button>
          <Button variant="primary" form="pool-edit" type="submit" loading={update.isPending}>
            Save
          </Button>
        </>
      }
    >
      <form id="pool-edit" className="form-stack" onSubmit={submit}>
        {update.error && (
          <Alert status="danger" sm>
            {problem(update.error)}
          </Alert>
        )}
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function DestroyPoolModal({ pool, onClose }: { pool: Pool; onClose: () => void }) {
  const destroy = useDestroyPool();
  const [echo, setEcho] = useState('');
  return (
    <Modal
      open
      size="sm"
      title={`Destroy ${pool.name}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={destroy.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={destroy.isPending}
            disabled={echo !== pool.name}
            onClick={() => {
              destroy.mutate({ id: pool.id, name: pool.name }, { onSuccess: onClose });
            }}
          >
            Destroy pool
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
          Every dataset, volume, and snapshot on {pool.name} ({bytes(pool.allocated_bytes)} in use)
          is destroyed. This cannot be undone.
        </p>
        <FormField label={`Type ${pool.name} to confirm`} required>
          <Input
            value={echo}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setEcho(e.target.value);
            }}
          />
        </FormField>
      </div>
    </Modal>
  );
}

// ------------------------------------------------------------ list

export function Pools({ canWrite, canAdmin }: { canWrite: boolean; canAdmin: boolean }) {
  const pools = usePools();
  const devices = useDevices();
  const startScrub = useStartScrub();
  const stopScrub = useStopScrub();
  const [creating, setCreating] = useState(false);
  const [editing, setEditing] = useState<Pool | null>(null);
  const [destroying, setDestroying] = useState<Pool | null>(null);
  const actionError = startScrub.error ?? stopScrub.error;
  const rows = pools.data?.items ?? [];

  return (
    <>
      <div className="toolbar">
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="plus-circle"
            onClick={() => {
              setCreating(true);
            }}
          >
            New pool
          </Button>
        )}
      </div>
      {actionError && (
        <Alert status="danger" closable>
          {problem(actionError)}
        </Alert>
      )}
      {pools.isError && (
        <Alert status="danger" closable>
          {problem(pools.error)}
        </Alert>
      )}
      {pools.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Pool>
          rows={rows}
          expandable
          renderDetail={(p: Pool) => <PoolDetail pool={p} />}
          placeholder="No pools."
          footerText={`${String(rows.length)} pools`}
          columns={[
            {
              key: 'name',
              label: 'Pool',
              sortable: true,
              render: (p) => (
                <span className="name-cell">
                  <NameCell name={p.name} metadata={p.metadata} />
                  {p.protected && <Label>PROTECTED</Label>}
                </span>
              ),
            },
            { key: 'health', label: 'Health', render: (p) => <HealthLabel health={p.health} /> },
            {
              key: 'capacity',
              label: 'Used',
              width: 220,
              render: (p) => (
                <div className="capacity-cell">
                  <ProgressBar
                    value={p.allocated_bytes}
                    max={p.size_bytes}
                    sm
                    {...((p.capacity_percent ?? 0) >= 90
                      ? { status: 'danger' }
                      : (p.capacity_percent ?? 0) >= 80
                        ? { status: 'warning' }
                        : {})}
                  />
                  <span className="cell-mono">
                    {bytes(p.allocated_bytes)} / {bytes(p.size_bytes)}
                  </span>
                </div>
              ),
            },
            {
              key: 'free_bytes',
              label: 'Free',
              sortable: true,
              render: (p) => <span className="cell-mono">{bytes(p.free_bytes)}</span>,
            },
            {
              key: 'fragmentation_percent',
              label: 'Frag',
              render: (p) => (
                <span className="cell-mono">
                  {p.fragmentation_percent === undefined
                    ? '-'
                    : `${String(p.fragmentation_percent)}%`}
                </span>
              ),
            },
            {
              key: 'scan',
              label: 'Scan',
              render: (p) =>
                p.scan?.state === 'in_progress' ? (
                  <Label status="info">
                    {`${p.scan.function.toUpperCase()} ${String(p.scan.progress ?? 0)}%`}
                  </Label>
                ) : (
                  <span className="cell-mono">
                    {p.scan ? `${p.scan.function} ${p.scan.state.replace('_', ' ')}` : '-'}
                  </span>
                ),
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (p) => {
                if (!canWrite) return null;
                const scanning = p.scan?.state === 'in_progress';
                const items: DropdownItem[] = [
                  scanning
                    ? {
                        label: 'Stop scrub',
                        icon: 'stop',
                        onClick: () => {
                          stopScrub.mutate(p.id);
                        },
                      }
                    : {
                        label: 'Scrub',
                        icon: 'shield-check',
                        onClick: () => {
                          startScrub.mutate(p.id);
                        },
                      },
                  {
                    label: 'Edit',
                    icon: 'pencil',
                    onClick: () => {
                      setEditing(p);
                    },
                  },
                ];
                if (canAdmin) {
                  items.push({ divider: true });
                  items.push({
                    label: 'Destroy',
                    icon: 'trash',
                    disabled: p.protected,
                    onClick: () => {
                      setDestroying(p);
                    },
                  });
                }
                return <Dropdown trigger="" variant="link-neutral" sm right items={items} />;
              },
            },
          ]}
        />
      )}
      {creating && (
        <CreatePoolModal
          devices={devices.data?.items ?? []}
          onClose={() => {
            setCreating(false);
          }}
        />
      )}
      {editing && (
        <EditPoolModal
          pool={editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
      {destroying && (
        <DestroyPoolModal
          pool={destroying}
          onClose={() => {
            setDestroying(null);
          }}
        />
      )}
    </>
  );
}
