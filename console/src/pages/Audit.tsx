import { useState } from 'react';

import { useAudit, type AuditEntry } from '../api/hooks.ts';
import { Button, Datagrid, Input, Label, Spinner } from '../design/index.tsx';
import { timestamp } from '../fmt.ts';

const PAGE = 50;

function ResultLabel({ result }: { result: AuditEntry['result'] }) {
  const status = result === 'ok' ? 'success' : result === 'denied' ? 'warning' : 'danger';
  return <Label status={status}>{result.toUpperCase()}</Label>;
}

function objectText(o: AuditEntry['object']): string {
  if (o.name) return `${o.kind} ${o.name}`;
  if (o.id) return `${o.kind} ${o.id}`;
  return o.kind;
}

export function Audit() {
  const [action, setAction] = useState('');
  const [applied, setApplied] = useState('');
  const [cursors, setCursors] = useState<string[]>([]);
  const cursor = cursors[cursors.length - 1];
  const page = useAudit({
    limit: PAGE,
    ...(applied ? { action: applied } : {}),
    ...(cursor ? { cursor } : {}),
  });
  const rows = page.data?.items ?? [];

  return (
    <>
      <div className="page-header">
        <h1>Audit log</h1>
      </div>
      <form
        className="toolbar"
        onSubmit={(e) => {
          e.preventDefault();
          setCursors([]);
          setApplied(action.trim());
        }}
      >
        <Input
          placeholder="Filter by action, e.g. user.create"
          value={action}
          style={{ width: 280 }}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setAction(e.target.value);
          }}
        />
        <Button type="submit" sm icon="filter">
          Apply
        </Button>
        {applied && (
          <Button
            type="button"
            sm
            variant="link"
            onClick={() => {
              setAction('');
              setApplied('');
              setCursors([]);
            }}
          >
            Clear
          </Button>
        )}
        <span className="spacer" />
        <Button
          type="button"
          sm
          disabled={cursors.length === 0}
          onClick={() => {
            setCursors((c) => c.slice(0, -1));
          }}
        >
          Newer
        </Button>
        <Button
          type="button"
          sm
          disabled={!page.data?.next_cursor}
          onClick={() => {
            const next = page.data?.next_cursor;
            if (next) setCursors((c) => [...c, next]);
          }}
        >
          Older
        </Button>
      </form>
      {page.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<AuditEntry>
          compact
          rows={rows}
          placeholder="No audit entries match."
          footerText={`${String(rows.length)} entries${page.data?.next_cursor ? ', more available' : ''}`}
          columns={[
            {
              key: 'at',
              label: 'When',
              width: 180,
              render: (e) => <span className="cell-mono">{timestamp(e.at)}</span>,
            },
            {
              key: 'actor',
              label: 'Actor',
              render: (e) => (
                <span>
                  {e.actor.username}{' '}
                  <span
                    className="cell-mono"
                    style={{ color: 'var(--cds-alias-typography-color-300)' }}
                  >
                    {e.actor.via}
                  </span>
                </span>
              ),
            },
            {
              key: 'action',
              label: 'Action',
              render: (e) => <span className="cell-mono">{e.action}</span>,
            },
            { key: 'object', label: 'Object', render: (e) => objectText(e.object) },
            { key: 'result', label: 'Result', render: (e) => <ResultLabel result={e.result} /> },
            { key: 'detail', label: 'Detail', render: (e) => e.detail ?? '' },
            {
              key: 'source',
              label: 'Source',
              render: (e) => <span className="cell-mono">{e.source ?? ''}</span>,
            },
          ]}
        />
      )}
    </>
  );
}
