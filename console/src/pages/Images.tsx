import { useCallback } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { useOutletContext } from 'react-router';

import { useEvents } from '../api/events.ts';
import type { Event, Session } from '../api/hooks.ts';
import { imageKeys } from '../api/images.ts';
import { Tabs } from '../design/index.tsx';
import { Available } from './images/Available.tsx';
import { Imported } from './images/Imported.tsx';
import { Sources } from './images/Sources.tsx';

export function Images() {
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canWrite = actor.role === 'admin' || actor.role === 'operator';
  const client = useQueryClient();

  // Import jobs report through events; refresh the lists as they arrive.
  useEvents(
    useCallback(
      (event: Event) => {
        if (event.kind.startsWith('image.') || event.object?.kind === 'job') {
          void client.invalidateQueries({ queryKey: imageKeys.all });
        }
      },
      [client],
    ),
  );

  return (
    <>
      <div className="page-header">
        <h1>Images</h1>
      </div>
      <Tabs
        tabs={[
          { label: 'Imported', content: <Imported canWrite={canWrite} /> },
          { label: 'Available', content: <Available canWrite={canWrite} /> },
          { label: 'Sources', content: <Sources canWrite={canWrite} /> },
        ]}
      />
    </>
  );
}
