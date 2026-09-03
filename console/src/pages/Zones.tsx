import { useCallback, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useOutletContext } from 'react-router';

import { useEvents } from '../api/events.ts';
import type { Event, Session } from '../api/hooks.ts';
import { useZoneAction, useZones, zoneKeys, type Zone } from '../api/zones.ts';
import { Alert, Button, Datagrid, Dropdown, Spinner } from '../design/index.tsx';
import { bytes } from '../fmt.ts';
import { NameCell } from './common/Metadata.tsx';
import { problem } from './common/util.ts';
import { CreateZone } from './zones/CreateZone.tsx';
import { ZoneStateLabel } from './zones/ZoneDetail.tsx';

export function Zones() {
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canWrite = actor.role === 'admin' || actor.role === 'operator';
  const zones = useZones();
  const action = useZoneAction();
  const client = useQueryClient();
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);

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

  const rows = zones.data?.items ?? [];

  return (
    <>
      <div className="page-header">
        <h1>Zones</h1>
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="plus-circle"
            onClick={() => {
              setCreating(true);
            }}
          >
            New zone
          </Button>
        )}
      </div>
      {action.error && (
        <Alert status="danger" closable>
          {problem(action.error)}
        </Alert>
      )}
      {zones.isError && (
        <Alert status="danger" closable>
          {problem(zones.error)}
        </Alert>
      )}
      {zones.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Zone>
          rows={rows}
          placeholder="No zones. Create one from an lx image, or a native zone from the host packages."
          footerText={`${String(rows.length)} zones`}
          columns={[
            {
              key: 'name',
              label: 'Zone',
              sortable: true,
              render: (z) => (
                <Link to={`/zones/${z.id}`}>
                  <NameCell name={z.name} metadata={z.metadata} />
                </Link>
              ),
            },
            { key: 'brand', label: 'Brand', sortable: true },
            { key: 'state', label: 'State', render: (z) => <ZoneStateLabel state={z.state} /> },
            {
              key: 'nics',
              label: 'Network',
              render: (z) => (
                <span className="cell-mono">
                  {z.nics
                    .map((n) => `${n.name}@${n.over}${n.address ? ` ${n.address}` : ''}`)
                    .join(', ') || '-'}
                </span>
              ),
            },
            {
              key: 'caps',
              label: 'Caps',
              render: (z) => (
                <span className="cell-mono">
                  {z.cpu_cap === undefined ? '-' : `${String(z.cpu_cap)} cpu`} /{' '}
                  {z.memory_cap_bytes === undefined ? '-' : bytes(z.memory_cap_bytes)}
                </span>
              ),
            },
            {
              key: 'autoboot',
              label: 'Autoboot',
              render: (z) => (z.autoboot ? 'yes' : 'no'),
            },
            {
              key: 'zonepath',
              label: 'Zonepath',
              render: (z) => <span className="cell-mono">{z.zonepath}</span>,
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (z) => {
                if (!canWrite) return null;
                const running = z.state === 'running';
                return (
                  <Dropdown
                    trigger=""
                    variant="link-neutral"
                    sm
                    right
                    items={[
                      {
                        label: 'Open',
                        icon: 'eye',
                        onClick: () => {
                          void navigate(`/zones/${z.id}`);
                        },
                      },
                      {
                        label: running ? 'Stop' : 'Start',
                        icon: running ? 'stop' : 'play',
                        disabled: !running && z.state !== 'installed' && z.state !== 'down',
                        onClick: () => {
                          action.mutate({ id: z.id, action: running ? 'stop' : 'start' });
                        },
                      },
                      {
                        label: 'Restart',
                        icon: 'refresh',
                        disabled: !running,
                        onClick: () => {
                          action.mutate({ id: z.id, action: 'restart' });
                        },
                      },
                    ]}
                  />
                );
              },
            },
          ]}
        />
      )}
      {creating && (
        <CreateZone
          onClose={() => {
            setCreating(false);
          }}
          onCreated={(id) => {
            setCreating(false);
            void navigate(`/zones/${id}`);
          }}
        />
      )}
    </>
  );
}
