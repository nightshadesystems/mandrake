import { useOutletContext } from 'react-router';

import type { Session } from '../api/hooks.ts';
import { Tabs } from '../design/index.tsx';
import { Addresses } from './network/Addresses.tsx';
import { Links } from './network/Links.tsx';
import { Routes } from './network/Routes.tsx';
import { Topology } from './network/Topology.tsx';

export function Network() {
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canWrite = actor.role === 'admin' || actor.role === 'operator';
  return (
    <>
      <div className="page-header">
        <h1>Network</h1>
      </div>
      <Tabs
        tabs={[
          { label: 'Topology', content: <Topology canWrite={canWrite} /> },
          { label: 'Links', content: <Links canWrite={canWrite} /> },
          { label: 'Addresses', content: <Addresses canWrite={canWrite} /> },
          { label: 'Routes', content: <Routes canWrite={canWrite} /> },
        ]}
      />
    </>
  );
}
