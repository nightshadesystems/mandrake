import { useCallback, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useOutletContext } from 'react-router';

import { useEvents } from '../api/events.ts';
import type { Event, Session } from '../api/hooks.ts';
import { useVmAction, useVms, vmKeys, type Vm } from '../api/vms.ts';
import { Alert, Button, Datagrid, Dropdown, Spinner } from '../design/index.tsx';
import { bytes } from '../fmt.ts';
import { NameCell } from './common/Metadata.tsx';
import { problem } from './common/util.ts';
import { CreateVm } from './vms/CreateVm.tsx';
import { canStart } from './vms/util.ts';
import { ZoneStateLabel } from './zones/ZoneDetail.tsx';

export function Vms() {
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canWrite = actor.role === 'admin' || actor.role === 'operator';
  const vms = useVms();
  const action = useVmAction();
  const client = useQueryClient();
  const navigate = useNavigate();
  const [creating, setCreating] = useState(false);

  useEvents(
    useCallback(
      (event: Event) => {
        if (event.kind.startsWith('vm.') || event.object?.kind === 'job') {
          void client.invalidateQueries({ queryKey: vmKeys.all });
        }
      },
      [client],
    ),
  );

  const rows = vms.data?.items ?? [];

  return (
    <>
      <div className="page-header">
        <h1>VMs</h1>
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="plus-circle"
            onClick={() => {
              setCreating(true);
            }}
          >
            New VM
          </Button>
        )}
      </div>
      {action.error && (
        <Alert status="danger" closable>
          {problem(action.error)}
        </Alert>
      )}
      {vms.isError && (
        <Alert status="danger" closable>
          {problem(vms.error)}
        </Alert>
      )}
      {vms.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Vm>
          rows={rows}
          placeholder="No VMs. Create one from a VM image, or install from an ISO."
          footerText={`${String(rows.length)} VMs`}
          columns={[
            {
              key: 'name',
              label: 'VM',
              sortable: true,
              render: (v) => (
                <Link to={`/vms/${v.id}`}>
                  <NameCell name={v.name} metadata={v.metadata} />
                </Link>
              ),
            },
            { key: 'state', label: 'State', render: (v) => <ZoneStateLabel state={v.state} /> },
            {
              key: 'sizing',
              label: 'vCPU / memory',
              render: (v) => (
                <span className="cell-mono">
                  {String(v.vcpus)} / {bytes(v.memory_bytes)}
                </span>
              ),
            },
            {
              key: 'disks',
              label: 'Disks',
              render: (v) => (
                <span className="cell-mono">
                  {v.disks.map((d) => bytes(d.size_bytes)).join(', ') || '-'}
                </span>
              ),
            },
            {
              key: 'nics',
              label: 'Network',
              render: (v) => (
                <span className="cell-mono">
                  {v.nics
                    .map((n) => `${n.name}@${n.over}${n.address ? ` ${n.address}` : ''}`)
                    .join(', ') || '-'}
                </span>
              ),
            },
            { key: 'vnc', label: 'VNC', render: (v) => (v.vnc ? 'on' : 'off') },
            {
              key: 'autoboot',
              label: 'Autoboot',
              render: (v) => (v.autoboot ? 'yes' : 'no'),
            },
            {
              key: 'pool',
              label: 'Pool',
              render: (v) => <span className="cell-mono">{v.pool ?? '-'}</span>,
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (v) => {
                if (!canWrite) return null;
                const running = v.state === 'running';
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
                          void navigate(`/vms/${v.id}`);
                        },
                      },
                      {
                        label: running ? 'Shut down' : 'Start',
                        icon: running ? 'stop' : 'play',
                        disabled: !running && !canStart(v),
                        onClick: () => {
                          action.mutate({ id: v.id, action: running ? 'stop' : 'start' });
                        },
                      },
                      {
                        label: 'Restart',
                        icon: 'refresh',
                        disabled: !running,
                        onClick: () => {
                          action.mutate({ id: v.id, action: 'restart' });
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
        <CreateVm
          onClose={() => {
            setCreating(false);
          }}
          onCreated={(id) => {
            setCreating(false);
            void navigate(`/vms/${id}`);
          }}
        />
      )}
    </>
  );
}
