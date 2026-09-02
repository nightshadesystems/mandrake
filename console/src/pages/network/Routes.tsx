import { useState, type SyntheticEvent } from 'react';

import { useCreateRoute, useDeleteRoute, useRoutes, type Route } from '../../api/network.ts';
import {
  Alert,
  Button,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Label,
  Modal,
  Spinner,
} from '../../design/index.tsx';
import { problem } from '../common/util.ts';

function validDestination(text: string): boolean {
  if (text === 'default') return true;
  const [ip, prefix, ...rest] = text.split('/');
  if (rest.length > 0 || ip === undefined || prefix === undefined) return false;
  const v4 = /^(\d{1,3}\.){3}\d{1,3}$/.test(ip);
  const v6 = !v4 && ip.includes(':') && /^[0-9a-fA-F:.]+$/.test(ip);
  return (v4 || v6) && /^\d+$/.test(prefix) && Number(prefix) <= (v4 ? 32 : 128);
}

function validGateway(text: string): boolean {
  return (
    /^(\d{1,3}\.){3}\d{1,3}$/.test(text) || (text.includes(':') && /^[0-9a-fA-F:.]+$/.test(text))
  );
}

function CreateRouteModal({ onClose }: { onClose: () => void }) {
  const create = useCreateRoute();
  const [destination, setDestination] = useState('default');
  const [gateway, setGateway] = useState('');
  const destOk = validDestination(destination.trim());
  const gwOk = validGateway(gateway.trim());
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    create.mutate(
      { destination: destination.trim(), gateway: gateway.trim() },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="New static route"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="route-form"
            type="submit"
            loading={create.isPending}
            disabled={!destOk || !gwOk}
          >
            Add route
          </Button>
        </>
      }
    >
      <form id="route-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <FormField
          label="Destination"
          required
          helper="default, or a network with prefix length such as 10.20.0.0/16"
          {...(destination && !destOk ? { error: 'default or network/prefix' } : {})}
        >
          <Input
            value={destination}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setDestination(e.target.value);
            }}
          />
        </FormField>
        <FormField
          label="Gateway"
          required
          helper="Must be reachable on a directly connected network"
          {...(gateway && !gwOk ? { error: 'An IPv4 or IPv6 address' } : {})}
        >
          <Input
            value={gateway}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setGateway(e.target.value);
            }}
          />
        </FormField>
        <p className="field-note">Added with route -p, so it survives reboot.</p>
      </form>
    </Modal>
  );
}

function DeleteRouteModal({ route, onClose }: { route: Route; onClose: () => void }) {
  const remove = useDeleteRoute();
  return (
    <Modal
      open
      size="sm"
      title={`Remove route to ${route.destination}?`}
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
              remove.mutate(route.id, { onSuccess: onClose });
            }}
          >
            Remove
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
          {route.destination} via {route.gateway ?? '-'} is removed now and from the persistent
          configuration.
          {route.destination === 'default' &&
            ' Hosts beyond the local networks become unreachable.'}
        </p>
      </div>
    </Modal>
  );
}

export function Routes({ canWrite }: { canWrite: boolean }) {
  const routes = useRoutes();
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Route | null>(null);
  const rows = routes.data?.items ?? [];

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
            New route
          </Button>
        )}
      </div>
      {routes.isError && (
        <Alert status="danger" closable>
          {problem(routes.error)}
        </Alert>
      )}
      {routes.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Route>
          rows={rows}
          placeholder="No routes."
          footerText={`${String(rows.length)} routes`}
          columns={[
            {
              key: 'destination',
              label: 'Destination',
              sortable: true,
              render: (r) => <span className="cell-mono">{r.destination}</span>,
            },
            {
              key: 'gateway',
              label: 'Gateway',
              render: (r) => <span className="cell-mono">{r.gateway ?? '-'}</span>,
            },
            { key: 'family', label: 'Family', sortable: true },
            {
              key: 'interface',
              label: 'Interface',
              render: (r) => <span className="cell-mono">{r.interface ?? '-'}</span>,
            },
            {
              key: 'kind',
              label: 'Kind',
              sortable: true,
              render: (r) =>
                r.kind === 'static' ? (
                  <Label status="info">STATIC</Label>
                ) : (
                  <Label>{r.kind.toUpperCase()}</Label>
                ),
            },
            {
              key: 'flags',
              label: 'Flags',
              render: (r) => <span className="cell-mono">{r.flags ?? ''}</span>,
            },
            {
              key: 'persistent',
              label: 'Persistent',
              render: (r) => (r.persistent ? 'yes' : 'no'),
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (r) =>
                canWrite && r.kind === 'static' ? (
                  <Dropdown
                    trigger=""
                    variant="link-neutral"
                    sm
                    right
                    items={[
                      {
                        label: 'Remove',
                        icon: 'trash',
                        onClick: () => {
                          setDeleting(r);
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
        <CreateRouteModal
          onClose={() => {
            setCreating(false);
          }}
        />
      )}
      {deleting && (
        <DeleteRouteModal
          route={deleting}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
    </>
  );
}
