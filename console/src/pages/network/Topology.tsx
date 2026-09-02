// The link topology: physical ports at the bottom, everything that sits
// on them stacked above, drawn from GET /network/links alone (ADR-0011).

import { useMemo, useState } from 'react';

import { useAddresses, useLinks, type Address, type Link } from '../../api/network.ts';
import { Alert, Button, Card, CardBlock, Label, Spinner, StackView } from '../../design/index.tsx';
import { problem } from '../common/util.ts';
import { CreateLinkModal, DeleteLinkModal, EditLinkModal } from './modals.tsx';
import { KIND_LABEL, type CreatableKind } from './util.ts';

const NODE_W = 172;
const NODE_H = 60;
const GAP_X = 28;
const GAP_Y = 64;
const PAD = 16;

interface Node {
  link: Link;
  x: number;
  y: number;
}

interface Layout {
  nodes: Node[];
  width: number;
  height: number;
}

/** How many links sit beneath one; physical links are 0. */
function depths(links: Link[]): Map<string, number> {
  const byName = new Map(links.map((l) => [l.name, l]));
  const memo = new Map<string, number>();
  const visiting = new Set<string>();
  const depth = (name: string): number => {
    const known = memo.get(name);
    if (known !== undefined) return known;
    const link = byName.get(name);
    if (!link || visiting.has(name)) return 0;
    visiting.add(name);
    const over = link.over ?? [];
    const d = over.length === 0 ? 0 : 1 + Math.max(...over.map(depth));
    visiting.delete(name);
    memo.set(name, d);
    return d;
  };
  links.forEach((l) => depth(l.name));
  return memo;
}

function layout(links: Link[]): Layout {
  const depthOf = depths(links);
  const maxDepth = Math.max(0, ...depthOf.values());
  const rows: Link[][] = Array.from({ length: maxDepth + 1 }, () => []);
  links.forEach((l) => {
    rows[depthOf.get(l.name) ?? 0]?.push(l);
  });
  const widest = Math.max(1, ...rows.map((r) => r.length));
  const width = widest * NODE_W + (widest - 1) * GAP_X + 2 * PAD;
  const nodes: Node[] = [];
  const xOf = new Map<string, number>();
  // Bottom row first so each row can sit under its parents.
  rows.forEach((row, depth) => {
    const ordered = [...row].sort((a, b) => {
      const ax = parentX(a, xOf);
      const bx = parentX(b, xOf);
      if (ax !== bx) return ax - bx;
      return a.name.localeCompare(b.name);
    });
    const rowWidth = ordered.length * NODE_W + (ordered.length - 1) * GAP_X;
    const start = (width - rowWidth) / 2;
    const y = PAD + (maxDepth - depth) * (NODE_H + GAP_Y);
    ordered.forEach((link, i) => {
      const x = start + i * (NODE_W + GAP_X);
      xOf.set(link.name, x);
      nodes.push({ link, x, y });
    });
  });
  const height = (maxDepth + 1) * NODE_H + maxDepth * GAP_Y + 2 * PAD;
  return { nodes, width, height };
}

function parentX(link: Link, xOf: Map<string, number>): number {
  const xs = (link.over ?? []).map((o) => xOf.get(o)).filter((x): x is number => x !== undefined);
  if (xs.length === 0) return Number.MAX_SAFE_INTEGER;
  return xs.reduce((a, b) => a + b, 0) / xs.length;
}

function edgePath(child: Node, parent: Node): string {
  const x1 = child.x + NODE_W / 2;
  const y1 = child.y + NODE_H;
  const x2 = parent.x + NODE_W / 2;
  const y2 = parent.y;
  const mid = (y1 + y2) / 2;
  return `M ${String(x1)} ${String(y1)} C ${String(x1)} ${String(mid)}, ${String(x2)} ${String(mid)}, ${String(x2)} ${String(y2)}`;
}

function subtitle(link: Link, addresses: Address[]): string {
  const parts: string[] = [];
  if (link.vid !== undefined) parts.push(`VID ${String(link.vid)}`);
  if (link.kind === 'aggr' && link.aggr) parts.push(`${link.aggr.policy} · ${link.aggr.lacp_mode}`);
  if (link.kind === 'phys' && link.speed_mbps !== undefined) {
    parts.push(
      link.speed_mbps >= 1000
        ? `${String(link.speed_mbps / 1000)} Gb`
        : `${String(link.speed_mbps)} Mb`,
    );
  }
  if (link.zone) parts.push(`zone ${link.zone}`);
  const ips = addresses.filter((a) => a.interface === link.name && a.address).map((a) => a.address);
  if (ips.length > 0)
    parts.push(ips.slice(0, 2).join(' ') + (ips.length > 2 ? ` +${String(ips.length - 2)}` : ''));
  return parts.join(' · ');
}

function NodeView({
  node,
  addresses,
  selected,
  onSelect,
}: {
  node: Node;
  addresses: Address[];
  selected: boolean;
  onSelect: () => void;
}) {
  const { link, x, y } = node;
  const sub = subtitle(link, addresses);
  return (
    <g
      className={`topo-node kind-${link.kind}${selected ? ' selected' : ''}`}
      transform={`translate(${String(x)}, ${String(y)})`}
      onClick={onSelect}
      role="button"
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') onSelect();
      }}
    >
      <rect width={NODE_W} height={NODE_H} rx={6} />
      <circle className={`state-${link.state}`} cx={14} cy={18} r={4} />
      <text className="name" x={26} y={22}>
        {link.metadata?.display_name ?? link.name}
      </text>
      <text className="kind" x={NODE_W - 10} y={22} textAnchor="end">
        {link.kind.toUpperCase()}
      </text>
      <text className="sub" x={14} y={42}>
        {link.metadata?.display_name ? `${link.name}${sub ? ' · ' : ''}${sub}` : sub}
      </text>
      {link.protected && (
        <g transform={`translate(${String(NODE_W - 48)}, 30)`}>
          <rect className="mgmt" width={38} height={16} rx={8} />
          <text className="mgmt" x={19} y={12} textAnchor="middle">
            MGMT
          </text>
        </g>
      )}
    </g>
  );
}

function Details({
  link,
  links,
  addresses,
  canWrite,
  onCreate,
  onEdit,
  onDelete,
}: {
  link: Link;
  links: Link[];
  addresses: Address[];
  canWrite: boolean;
  onCreate: (kind: CreatableKind) => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const own = addresses.filter((a) => a.interface === link.name);
  const dependents = links.filter((l) => (l.over ?? []).includes(link.name));
  const blocks = [
    { key: 'Kind', value: KIND_LABEL[link.kind] },
    { key: 'State', value: link.state },
    ...(link.over && link.over.length > 0 ? [{ key: 'Over', value: link.over.join(', ') }] : []),
    ...(link.mtu !== undefined ? [{ key: 'MTU', value: String(link.mtu) }] : []),
    ...(link.mac
      ? [{ key: 'MAC', value: link.mac + (link.mac_mode ? ` (${link.mac_mode})` : '') }]
      : []),
    ...(link.vid !== undefined ? [{ key: 'VLAN id', value: String(link.vid) }] : []),
    ...(link.speed_mbps !== undefined
      ? [
          {
            key: 'Speed',
            value: `${String(link.speed_mbps)} Mb/s${link.duplex ? ` ${link.duplex}` : ''}`,
          },
        ]
      : []),
    ...(link.device ? [{ key: 'Device', value: link.device }] : []),
    ...(link.media ? [{ key: 'Media', value: link.media }] : []),
    ...(link.aggr
      ? [
          {
            key: 'Aggregation',
            value: `${link.aggr.policy} · LACP ${link.aggr.lacp_mode} ${link.aggr.lacp_timer}`,
            expanded: true,
            children: link.aggr.ports.map((p) => ({
              key: p.name,
              value: `${p.state}${p.speed_mbps !== undefined ? ` · ${String(p.speed_mbps)} Mb/s` : ''}`,
            })),
          },
        ]
      : []),
    ...(link.zone ? [{ key: 'Zone', value: link.zone }] : []),
    ...(own.length > 0
      ? [
          {
            key: 'Addresses',
            value: String(own.length),
            expanded: true,
            children: own.map((a) => ({
              key: a.name,
              value: `${a.address ?? '(pending)'} · ${a.kind}${a.protected ? ' · management' : ''}`,
            })),
          },
        ]
      : []),
    ...(dependents.length > 0
      ? [{ key: 'Carries', value: dependents.map((d) => d.name).join(', ') }]
      : []),
    ...(link.metadata?.description
      ? [{ key: 'Description', value: link.metadata.description }]
      : []),
  ];
  const canVnic = link.kind === 'phys' || link.kind === 'aggr' || link.kind === 'etherstub';
  const canVlan = link.kind === 'phys' || link.kind === 'aggr';
  const canDelete = link.kind !== 'phys' && link.kind !== 'other' && !link.protected;
  return (
    <Card
      header={
        <span className="name-cell">
          <span className="mono">{link.name}</span>
          {link.protected && <Label>PROTECTED</Label>}
        </span>
      }
    >
      <CardBlock>
        <StackView blocks={blocks} />
      </CardBlock>
      {canWrite && (
        <CardBlock>
          <div className="button-column">
            {canVnic && (
              <Button
                sm
                icon="plus"
                onClick={() => {
                  onCreate('vnic');
                }}
              >
                New VNIC on {link.name}
              </Button>
            )}
            {canVlan && (
              <Button
                sm
                icon="plus"
                onClick={() => {
                  onCreate('vlan');
                }}
              >
                New VLAN on {link.name}
              </Button>
            )}
            <Button sm icon="pencil" onClick={onEdit}>
              Edit
            </Button>
            {canDelete && (
              <Button sm variant="danger" icon="trash" onClick={onDelete}>
                Delete
              </Button>
            )}
          </div>
        </CardBlock>
      )}
    </Card>
  );
}

export function Topology({ canWrite }: { canWrite: boolean }) {
  const links = useLinks();
  const addresses = useAddresses();
  const [selectedName, setSelectedName] = useState<string | null>(null);
  const [creating, setCreating] = useState<{ kind: CreatableKind; over?: string } | null>(null);
  const [editing, setEditing] = useState<Link | null>(null);
  const [deleting, setDeleting] = useState<Link | null>(null);
  const items = useMemo(() => links.data?.items ?? [], [links.data]);
  const placed = useMemo(() => layout(items), [items]);
  const byName = new Map(placed.nodes.map((n) => [n.link.name, n]));
  const selected = selectedName ? items.find((l) => l.name === selectedName) : undefined;
  const addressItems = addresses.data?.items ?? [];

  if (links.isPending) {
    return (
      <div className="empty">
        <Spinner />
      </div>
    );
  }

  return (
    <>
      <div className="toolbar">
        <span className="topo-legend">
          <span className="legend-dot state-up" /> up
          <span className="legend-dot state-down" /> down
          <span className="legend-pill">MGMT</span> carries the management address
        </span>
        <span className="spacer" />
        {canWrite && (
          <>
            <Button
              icon="plus"
              onClick={() => {
                setCreating({ kind: 'etherstub' });
              }}
            >
              New etherstub
            </Button>
            <Button
              icon="plus"
              onClick={() => {
                setCreating({ kind: 'aggr' });
              }}
            >
              New aggregation
            </Button>
            <Button
              icon="plus"
              onClick={() => {
                setCreating({ kind: 'vlan' });
              }}
            >
              New VLAN
            </Button>
            <Button
              variant="primary"
              icon="plus-circle"
              onClick={() => {
                setCreating({ kind: 'vnic' });
              }}
            >
              New VNIC
            </Button>
          </>
        )}
      </div>
      {links.isError && (
        <Alert status="danger" closable>
          {problem(links.error)}
        </Alert>
      )}
      <div className="topology-layout">
        <div className="topology-canvas">
          {items.length === 0 ? (
            <div className="empty">No datalinks.</div>
          ) : (
            <svg
              width={placed.width}
              height={placed.height}
              viewBox={`0 0 ${String(placed.width)} ${String(placed.height)}`}
              role="img"
              aria-label="Link topology"
            >
              {placed.nodes.map((n) =>
                (n.link.over ?? []).map((o) => {
                  const parent = byName.get(o);
                  return parent ? (
                    <path
                      key={`${n.link.name}-${o}`}
                      className={`topo-edge${
                        selectedName === n.link.name || selectedName === o ? ' selected' : ''
                      }`}
                      d={edgePath(n, parent)}
                    />
                  ) : null;
                }),
              )}
              {placed.nodes.map((n) => (
                <NodeView
                  key={n.link.name}
                  node={n}
                  addresses={addressItems}
                  selected={selectedName === n.link.name}
                  onSelect={() => {
                    setSelectedName(selectedName === n.link.name ? null : n.link.name);
                  }}
                />
              ))}
            </svg>
          )}
        </div>
        <div className="topology-side">
          {selected ? (
            <Details
              link={selected}
              links={items}
              addresses={addressItems}
              canWrite={canWrite}
              onCreate={(kind) => {
                setCreating({ kind, over: selected.name });
              }}
              onEdit={() => {
                setEditing(selected);
              }}
              onDelete={() => {
                setDeleting(selected);
              }}
            />
          ) : (
            <Card header="Details">
              <CardBlock>
                <p className="field-note">Select a link to see its properties and actions.</p>
              </CardBlock>
            </Card>
          )}
        </div>
      </div>
      {creating && (
        <CreateLinkModal
          kind={creating.kind}
          links={items}
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
          links={items}
          onClose={() => {
            setDeleting(null);
            setSelectedName(null);
          }}
        />
      )}
    </>
  );
}
