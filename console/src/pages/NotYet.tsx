import { useLocation } from 'react-router';

const PHASES: Record<string, { name: string; phase: number }> = {
  vms: { name: 'VMs', phase: 5 },
  zones: { name: 'Zones', phase: 4 },
  images: { name: 'Images', phase: 4 },
  network: { name: 'Network', phase: 3 },
  storage: { name: 'Storage', phase: 3 },
  host: { name: 'Host settings', phase: 6 },
  updates: { name: 'Updates and boot environments', phase: 7 },
};

export function NotYet() {
  const { pathname } = useLocation();
  const key = pathname.split('/').filter(Boolean).pop() ?? '';
  const known = PHASES[key];
  return (
    <div className="empty">
      <clr-icon shape="wrench" size="32"></clr-icon>
      {known ? (
        <p>
          {known.name} arrives in phase {known.phase}. The API and this page ship together.
        </p>
      ) : (
        <p>There is nothing at {pathname}.</p>
      )}
    </div>
  );
}
