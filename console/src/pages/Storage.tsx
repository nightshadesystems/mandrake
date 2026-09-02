import { useOutletContext } from 'react-router';

import type { Session } from '../api/hooks.ts';
import { Tabs } from '../design/index.tsx';
import { Datasets } from './storage/Datasets.tsx';
import { Devices } from './storage/Devices.tsx';
import { Pools } from './storage/Pools.tsx';
import { Snapshots } from './storage/Snapshots.tsx';

export function Storage() {
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canAdmin = actor.role === 'admin';
  const canWrite = canAdmin || actor.role === 'operator';
  return (
    <>
      <div className="page-header">
        <h1>Storage</h1>
      </div>
      <Tabs
        tabs={[
          { label: 'Pools', content: <Pools canWrite={canWrite} canAdmin={canAdmin} /> },
          { label: 'Datasets', content: <Datasets kind="filesystem" canWrite={canWrite} /> },
          { label: 'Volumes', content: <Datasets kind="volume" canWrite={canWrite} /> },
          { label: 'Snapshots', content: <Snapshots canWrite={canWrite} /> },
          { label: 'Devices', content: <Devices /> },
        ]}
      />
    </>
  );
}
