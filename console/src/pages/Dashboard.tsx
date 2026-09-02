import { useCallback, useState } from 'react';
import { useQueryClient } from '@tanstack/react-query';

import { useEvents } from '../api/events.ts';
import { keys, useAudit, useResources, useSystem, type Event } from '../api/hooks.ts';
import { Card, CardBlock, ProgressBar, Skeleton } from '../design/index.tsx';
import { bytes, duration, percent, relative, timestamp } from '../fmt.ts';

function HostCard() {
  const system = useSystem();
  if (!system.data) {
    return (
      <Card header="Host">
        <CardBlock>
          <Skeleton height={80} />
        </CardBlock>
      </Card>
    );
  }
  const s = system.data;
  return (
    <Card header="Host">
      <CardBlock>
        <dl className="kv">
          <dt>Hostname</dt>
          <dd className="mono">{s.hostname}</dd>
          <dt>Mandrake</dt>
          <dd className="mono">{s.version}</dd>
          <dt>OmniOS</dt>
          <dd className="mono">{s.omnios_release}</dd>
          <dt>Boot environment</dt>
          <dd className="mono">{s.boot_environment}</dd>
          <dt>Uptime</dt>
          <dd className="mono">{duration(s.uptime_seconds)}</dd>
          <dt>Time</dt>
          <dd className="mono">{timestamp(s.time)}</dd>
        </dl>
      </CardBlock>
    </Card>
  );
}

function ResourcesCard() {
  const resources = useResources();
  if (!resources.data) {
    return (
      <Card header="Resources">
        <CardBlock>
          <Skeleton height={80} />
        </CardBlock>
      </Card>
    );
  }
  const r = resources.data;
  const used = r.memory.total_bytes - r.memory.free_bytes;
  const memPct = percent(used, r.memory.total_bytes);
  const load1 = r.load_avg[0] ?? 0;
  const loadPct = Math.min(100, percent(load1 * 100, r.cpus * 100));
  return (
    <Card header="Resources">
      <CardBlock>
        <div className="gauge">
          <ProgressBar
            value={loadPct}
            label={`Load ${load1.toFixed(2)} on ${String(r.cpus)} CPUs`}
            showValue
            {...(loadPct >= 90 ? { status: 'danger' } : loadPct >= 70 ? { status: 'warning' } : {})}
          />
          <ProgressBar
            value={memPct}
            label={`Memory ${bytes(used)} of ${bytes(r.memory.total_bytes)}`}
            showValue
            {...(memPct >= 90 ? { status: 'danger' } : memPct >= 80 ? { status: 'warning' } : {})}
          />
          <div className="mono" style={{ color: 'var(--cds-alias-typography-color-300)' }}>
            load {r.load_avg.map((l) => l.toFixed(2)).join(' ')} · sampled {relative(r.sampled_at)}
          </div>
        </div>
      </CardBlock>
    </Card>
  );
}

function PlaceholderCard({ title, phase }: { title: string; phase: number }) {
  return (
    <Card header={title}>
      <CardBlock>
        <div className="empty" style={{ padding: 24 }}>
          <p>Arrives in phase {phase}.</p>
        </div>
      </CardBlock>
    </Card>
  );
}

function describe(kind: string, object: Event['object'], actor: Event['actor']): string {
  const who = actor?.username ?? 'someone';
  const what = object?.name ? `${object.kind} ${object.name}` : (object?.kind ?? '');
  const [, verb = kind] = kind.split('.');
  const verbs: Record<string, string> = {
    create: 'created',
    update: 'updated',
    delete: 'deleted',
    login: 'signed in',
    logout: 'signed out',
    password: 'changed the password of',
    revoke: 'revoked',
  };
  const past = verbs[verb] ?? verb;
  if (verb === 'login' || verb === 'logout') return `${who} ${past}`;
  return `${who} ${past} ${what}`.trim();
}

function ActivityCard() {
  const client = useQueryClient();
  const recent = useAudit({ limit: 10 });
  const [live, setLive] = useState<Event[]>([]);

  useEvents(
    useCallback(
      (event: Event) => {
        setLive((prev) => [event, ...prev].slice(0, 10));
        void client.invalidateQueries({ queryKey: keys.users });
        void client.invalidateQueries({ queryKey: ['audit'] });
      },
      [client],
    ),
  );

  const rows: { id: string; at: string; text: string }[] = [
    ...live.map((e) => ({ id: `e${e.id}`, at: e.at, text: describe(e.kind, e.object, e.actor) })),
    ...(recent.data?.items ?? []).map((a) => ({
      id: `a${a.id}`,
      at: a.at,
      text: describe(a.action, a.object, a.actor) + (a.result === 'ok' ? '' : ` (${a.result})`),
    })),
  ]
    .filter((row, i, all) => all.findIndex((r) => r.at === row.at && r.text === row.text) === i)
    .slice(0, 12);

  return (
    <Card header="Recent activity">
      <CardBlock>
        {rows.length === 0 ? (
          <div className="empty" style={{ padding: 24 }}>
            <p>Nothing yet.</p>
          </div>
        ) : (
          <div className="activity">
            {rows.map((r) => (
              <div key={r.id} className="activity-row">
                <span className="when mono" title={timestamp(r.at)}>
                  {relative(r.at)}
                </span>
                <span>{r.text}</span>
              </div>
            ))}
          </div>
        )}
      </CardBlock>
    </Card>
  );
}

export function Dashboard() {
  return (
    <>
      <div className="page-header">
        <h1>Dashboard</h1>
      </div>
      <div className="card-grid">
        <HostCard />
        <ResourcesCard />
        <PlaceholderCard title="Virtual machines" phase={5} />
        <PlaceholderCard title="Zones" phase={4} />
        <ActivityCard />
      </div>
    </>
  );
}
