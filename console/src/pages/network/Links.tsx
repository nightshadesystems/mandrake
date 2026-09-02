import { useState } from 'react';

import { useLinks, type Link } from '../../api/network.ts';
import {
  Alert,
  Datagrid,
  Dropdown,
  type DropdownItem,
  Label,
  Spinner,
} from '../../design/index.tsx';
import { NameCell } from '../common/Metadata.tsx';
import { problem } from '../common/util.ts';
import { CreateLinkModal, DeleteLinkModal, EditLinkModal } from './modals.tsx';
import { KIND_LABEL, type CreatableKind } from './util.ts';

function StateLabel({ link }: { link: Link }) {
  if (link.state === 'up') return <Label status="success">UP</Label>;
  if (link.state === 'down') return <Label status="danger">DOWN</Label>;
  return <Label>UNKNOWN</Label>;
}

export function Links({ canWrite }: { canWrite: boolean }) {
  const links = useLinks();
  const [creating, setCreating] = useState<{ kind: CreatableKind; over?: string } | null>(null);
  const [editing, setEditing] = useState<Link | null>(null);
  const [deleting, setDeleting] = useState<Link | null>(null);
  const rows = links.data?.items ?? [];

  return (
    <>
      <div className="toolbar">
        <span className="spacer" />
        {canWrite && (
          <Dropdown
            variant="primary"
            trigger="New link"
            right
            items={[
              {
                label: 'Aggregation',
                icon: 'link',
                onClick: () => {
                  setCreating({ kind: 'aggr' });
                },
              },
              {
                label: 'VLAN',
                icon: 'tag',
                onClick: () => {
                  setCreating({ kind: 'vlan' });
                },
              },
              {
                label: 'Etherstub',
                icon: 'network-switch',
                onClick: () => {
                  setCreating({ kind: 'etherstub' });
                },
              },
              {
                label: 'VNIC',
                icon: 'network-settings',
                onClick: () => {
                  setCreating({ kind: 'vnic' });
                },
              },
            ]}
          />
        )}
      </div>
      {links.isError && (
        <Alert status="danger" closable>
          {problem(links.error)}
        </Alert>
      )}
      {links.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Link>
          rows={rows}
          placeholder="No datalinks."
          footerText={`${String(rows.length)} links`}
          columns={[
            {
              key: 'name',
              label: 'Link',
              sortable: true,
              render: (l) => (
                <span className="name-cell">
                  <NameCell name={l.name} metadata={l.metadata} />
                  {l.protected && <Label>PROTECTED</Label>}
                </span>
              ),
            },
            { key: 'kind', label: 'Kind', sortable: true, render: (l) => KIND_LABEL[l.kind] },
            { key: 'state', label: 'State', render: (l) => <StateLabel link={l} /> },
            {
              key: 'over',
              label: 'Over',
              render: (l) => <span className="cell-mono">{(l.over ?? []).join(', ') || '-'}</span>,
            },
            {
              key: 'mac',
              label: 'MAC',
              render: (l) => (
                <span className="cell-mono">
                  {l.mac ?? '-'}
                  {l.mac_mode ? ` (${l.mac_mode})` : ''}
                </span>
              ),
            },
            {
              key: 'vid',
              label: 'VID',
              render: (l) => (
                <span className="cell-mono">{l.vid === undefined ? '-' : String(l.vid)}</span>
              ),
            },
            {
              key: 'mtu',
              label: 'MTU',
              render: (l) => (
                <span className="cell-mono">{l.mtu === undefined ? '-' : String(l.mtu)}</span>
              ),
            },
            {
              key: 'speed_mbps',
              label: 'Speed',
              render: (l) => (
                <span className="cell-mono">
                  {l.speed_mbps === undefined ? '-' : `${String(l.speed_mbps)} Mb/s`}
                  {l.duplex && l.duplex !== 'unknown' ? ` ${l.duplex}` : ''}
                </span>
              ),
            },
            { key: 'zone', label: 'Zone', render: (l) => l.zone ?? '' },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (l) => {
                if (!canWrite) return null;
                const items: DropdownItem[] = [
                  {
                    label: 'Edit',
                    icon: 'pencil',
                    onClick: () => {
                      setEditing(l);
                    },
                  },
                ];
                if (l.kind === 'phys' || l.kind === 'aggr' || l.kind === 'etherstub') {
                  items.push({
                    label: 'New VNIC on it',
                    icon: 'plus',
                    onClick: () => {
                      setCreating({ kind: 'vnic', over: l.name });
                    },
                  });
                }
                if (l.kind === 'phys' || l.kind === 'aggr') {
                  items.push({
                    label: 'New VLAN on it',
                    icon: 'plus',
                    onClick: () => {
                      setCreating({ kind: 'vlan', over: l.name });
                    },
                  });
                }
                if (l.kind !== 'phys' && l.kind !== 'other') {
                  items.push({ divider: true });
                  items.push({
                    label: 'Delete',
                    icon: 'trash',
                    disabled: l.protected,
                    onClick: () => {
                      setDeleting(l);
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
        <CreateLinkModal
          kind={creating.kind}
          links={rows}
          {...(creating.over ? { over: creating.over } : {})}
          onClose={() => {
            setCreating(null);
          }}
        />
      )}
      {editing && (
        <EditLinkModal
          link={editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
      {deleting && (
        <DeleteLinkModal
          link={deleting}
          links={rows}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
      {canWrite && (
        <p className="field-note">
          Physical links cannot be deleted. A link that carries the management address, and
          everything beneath it, is protected.
        </p>
      )}
    </>
  );
}
