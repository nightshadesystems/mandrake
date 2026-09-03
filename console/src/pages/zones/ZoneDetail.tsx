// One zone: properties, lifecycle actions, edit, delete, and the console.

import { useCallback, useState, type SyntheticEvent } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useOutletContext, useParams } from 'react-router';

import { useEvents } from '../../api/events.ts';
import type { Event, Session } from '../../api/hooks.ts';
import {
  useDeleteZone,
  useUpdateZone,
  useZone,
  useZoneAction,
  zoneKeys,
  type Zone,
  type ZoneNic,
} from '../../api/zones.ts';
import {
  Alert,
  Button,
  Checkbox,
  Dropdown,
  FormField,
  Input,
  Label,
  Modal,
  Spinner,
  StackView,
  Tabs,
} from '../../design/index.tsx';
import { bytes } from '../../fmt.ts';
import { MetadataFields } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody, problem } from '../common/util.ts';
import { parseSize } from '../storage/util.ts';
import { NicEditor } from './NicEditor.tsx';
import { nicErrors } from './util.ts';
import { ConsoleTerminal } from './Terminal.tsx';

export function ZoneStateLabel({ state }: { state: Zone['state'] }) {
  switch (state) {
    case 'running':
      return <Label status="success">RUNNING</Label>;
    case 'installed':
    case 'down':
      return <Label>{state.toUpperCase()}</Label>;
    case 'configured':
    case 'incomplete':
      return <Label status="warning">{state.toUpperCase()}</Label>;
    case 'ready':
    case 'shutting_down':
      return <Label status="info">{state.replace('_', ' ').toUpperCase()}</Label>;
    default:
      return <Label status="danger">{state.toUpperCase()}</Label>;
  }
}

function EditZoneModal({ zone, onClose }: { zone: Zone; onClose: () => void }) {
  const update = useUpdateZone();
  const [cpuCap, setCpuCap] = useState(zone.cpu_cap === undefined ? '' : String(zone.cpu_cap));
  const [memory, setMemory] = useState(
    zone.memory_cap_bytes === undefined ? '' : String(zone.memory_cap_bytes),
  );
  const [autoboot, setAutoboot] = useState(zone.autoboot);
  const [hostname, setHostname] = useState(zone.hostname ?? '');
  const [resolvers, setResolvers] = useState((zone.resolvers ?? []).join(', '));
  const [nics, setNics] = useState<ZoneNic[]>(zone.nics);
  const [meta, setMeta] = useState(emptyMetadata(zone.metadata));
  const memoryBytes = memory.trim() ? parseSize(memory) : undefined;
  const errors = [
    ...(cpuCap.trim() && !(Number(cpuCap) > 0) ? ['CPU cap must be a positive number'] : []),
    ...(memory.trim() && memoryBytes === undefined ? ['Memory cap must be a size'] : []),
    ...nicErrors(nics),
  ];
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    const resolverList = resolvers
      .split(/[\s,]+/)
      .map((s) => s.trim())
      .filter((s) => s !== '');
    update.mutate(
      {
        id: zone.id,
        body: {
          nics,
          cpu_cap: cpuCap.trim() ? Number(cpuCap) : null,
          memory_cap_bytes: memoryBytes ?? null,
          autoboot,
          ...(hostname.trim() ? { hostname: hostname.trim() } : {}),
          resolvers: resolverList,
          ...(metadata ? { metadata } : {}),
        },
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      size="lg"
      title={`Edit ${zone.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={update.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="zone-edit"
            type="submit"
            loading={update.isPending}
            disabled={errors.length > 0}
          >
            Save
          </Button>
        </>
      }
    >
      <form id="zone-edit" className="form-stack" onSubmit={submit}>
        {update.error && (
          <Alert status="danger" sm>
            {problem(update.error)}
          </Alert>
        )}
        {zone.state === 'running' && (
          <Alert status="info" sm>
            NIC and cap changes apply at the next boot; autoboot and metadata at once.
          </Alert>
        )}
        <div className="form-row">
          <FormField label="CPU cap" helper="Empty removes it">
            <Input
              value={cpuCap}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setCpuCap(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Memory cap" helper="e.g. 2G; empty removes it">
            <Input
              value={memory}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setMemory(e.target.value);
              }}
            />
          </FormField>
        </div>
        <Checkbox
          label="Boot with the host (autoboot)"
          checked={autoboot}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setAutoboot(e.target.checked);
          }}
        />
        <div className="form-row">
          <FormField label="Hostname">
            <Input
              value={hostname}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setHostname(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Resolvers">
            <Input
              value={resolvers}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setResolvers(e.target.value);
              }}
            />
          </FormField>
        </div>
        <NicEditor nics={nics} onChange={setNics} />
        {errors.length > 0 && (
          <Alert
            status="warning"
            items={errors.map((e) => (
              <span key={e}>{e}</span>
            ))}
          />
        )}
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function DeleteZoneModal({ zone, onClose }: { zone: Zone; onClose: () => void }) {
  const remove = useDeleteZone();
  const navigate = useNavigate();
  const [purge, setPurge] = useState(false);
  return (
    <Modal
      open
      size="sm"
      title={`Delete ${zone.name}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={remove.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={remove.isPending}
            onClick={() => {
              remove.mutate(
                { id: zone.id, purge },
                {
                  onSuccess: () => {
                    void navigate('/zones');
                  },
                },
              );
            }}
          >
            Delete
          </Button>
        </>
      }
    >
      <div className="form-stack">
        {remove.error && (
          <Alert status="danger" sm>
            {problem(remove.error)}
          </Alert>
        )}
        <p>
          The zone is halted, uninstalled, and its configuration removed. Its datasets stay unless
          you purge them.
        </p>
        <Checkbox
          label={`Also destroy ${zone.dataset ?? 'its datasets'}`}
          checked={purge}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setPurge(e.target.checked);
          }}
        />
      </div>
    </Modal>
  );
}

function Overview({ zone }: { zone: Zone }) {
  return (
    <StackView
      blocks={[
        { key: 'Brand', value: zone.brand },
        { key: 'State', value: <ZoneStateLabel state={zone.state} /> },
        ...(zone.image_id
          ? [
              {
                key: 'Image',
                value: <Link to="/images">{zone.image_id}</Link>,
              },
            ]
          : []),
        { key: 'Zonepath', value: <span className="mono">{zone.zonepath}</span> },
        ...(zone.dataset
          ? [{ key: 'Dataset', value: <span className="mono">{zone.dataset}</span> }]
          : []),
        { key: 'CPU cap', value: zone.cpu_cap === undefined ? 'none' : String(zone.cpu_cap) },
        {
          key: 'Memory cap',
          value: zone.memory_cap_bytes === undefined ? 'none' : bytes(zone.memory_cap_bytes),
        },
        { key: 'Autoboot', value: zone.autoboot ? 'yes' : 'no' },
        ...(zone.hostname ? [{ key: 'Hostname', value: zone.hostname }] : []),
        ...(zone.resolvers && zone.resolvers.length > 0
          ? [{ key: 'Resolvers', value: zone.resolvers.join(', ') }]
          : []),
        {
          key: 'NICs',
          value: String(zone.nics.length),
          expanded: true,
          children: zone.nics.map((n) => ({
            key: n.name,
            value: `over ${n.over}${n.vid !== undefined ? ` · vid ${String(n.vid)}` : ''}${
              n.mac ? ` · ${n.mac}` : ''
            }${n.address ? ` · ${n.address}` : ''}${n.gateway ? ` via ${n.gateway}` : ''}`,
          })),
        },
        ...(zone.metadata?.description
          ? [{ key: 'Description', value: zone.metadata.description }]
          : []),
        ...(zone.metadata?.tags && zone.metadata.tags.length > 0
          ? [{ key: 'Tags', value: zone.metadata.tags.join(', ') }]
          : []),
        ...(zone.metadata?.notes ? [{ key: 'Notes', value: zone.metadata.notes }] : []),
      ]}
    />
  );
}

export function ZoneDetail() {
  const { id = '' } = useParams();
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canWrite = actor.role === 'admin' || actor.role === 'operator';
  const zone = useZone(id);
  const action = useZoneAction();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);

  useEvents(
    useCallback(
      (event: Event) => {
        if (event.kind.startsWith('zone.') || event.object?.kind === 'job') {
          void client.invalidateQueries({ queryKey: zoneKeys.all });
        }
      },
      [client],
    ),
  );

  if (zone.isPending) {
    return (
      <div className="empty">
        <Spinner />
      </div>
    );
  }
  if (zone.isError) {
    return (
      <>
        <div className="page-header">
          <h1>Zone</h1>
        </div>
        <Alert status="danger">{problem(zone.error)}</Alert>
        <p>
          <Link to="/zones">Back to zones</Link>
        </p>
      </>
    );
  }
  const z = zone.data;
  const running = z.state === 'running';
  const canStart = z.state === 'installed' || z.state === 'down';
  const consoleReady = z.state !== 'configured' && z.state !== 'incomplete';

  return (
    <>
      <div className="page-header">
        <h1>
          <Link to="/zones">Zones</Link> / {z.metadata?.display_name ?? z.name}
        </h1>
        <ZoneStateLabel state={z.state} />
        <span className="spacer" />
        {canWrite && (
          <>
            {canStart && (
              <Button
                variant="primary"
                icon="play"
                loading={action.isPending}
                onClick={() => {
                  action.mutate({ id: z.id, action: 'start' });
                }}
              >
                Start
              </Button>
            )}
            {running && (
              <Button
                icon="stop"
                loading={action.isPending}
                onClick={() => {
                  action.mutate({ id: z.id, action: 'stop' });
                }}
              >
                Stop
              </Button>
            )}
            <Dropdown
              trigger=""
              variant="link-neutral"
              right
              items={[
                {
                  label: 'Restart',
                  icon: 'refresh',
                  disabled: !running,
                  onClick: () => {
                    action.mutate({ id: z.id, action: 'restart' });
                  },
                },
                {
                  label: 'Halt (force)',
                  icon: 'power',
                  disabled: !running,
                  onClick: () => {
                    action.mutate({ id: z.id, action: 'stop', force: true });
                  },
                },
                {
                  label: 'Edit',
                  icon: 'pencil',
                  onClick: () => {
                    setEditing(true);
                  },
                },
                { divider: true },
                {
                  label: 'Delete',
                  icon: 'trash',
                  onClick: () => {
                    setDeleting(true);
                  },
                },
              ]}
            />
          </>
        )}
      </div>
      {action.error && (
        <Alert status="danger" closable>
          {problem(action.error)}
        </Alert>
      )}
      <Tabs
        tabs={[
          { label: 'Overview', content: <Overview zone={z} /> },
          {
            label: 'Console',
            disabled: !canWrite || !consoleReady,
            content: consoleReady ? (
              <ConsoleTerminal kind="zone" id={z.id} />
            ) : (
              <p className="field-note">The console is available once the zone is installed.</p>
            ),
          },
        ]}
      />
      {editing && (
        <EditZoneModal
          zone={z}
          onClose={() => {
            setEditing(false);
          }}
        />
      )}
      {deleting && (
        <DeleteZoneModal
          zone={z}
          onClose={() => {
            setDeleting(false);
          }}
        />
      )}
    </>
  );
}
