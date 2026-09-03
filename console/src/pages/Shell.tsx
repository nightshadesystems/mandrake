import { Navigate, Outlet, useLocation, useNavigate } from 'react-router';

import symbolOnDark from '../../design/assets/logos/nightshade-symbol-on-dark.svg';
import { isUnauthorized } from '../api/client.ts';
import { useLogout, useSession } from '../api/hooks.ts';
import {
  Dropdown,
  Header,
  HeaderAction,
  HeaderDivider,
  Spinner,
  Subnav,
  VerticalNav,
  type NavGroup,
} from '../design/index.tsx';
import { useTheme } from '../theme.ts';

interface Section {
  label: string;
  path: string;
  /** Spec phase that ships the section; undefined means it exists now. */
  phase?: number;
  nav?: NavGroup;
}

const DASHBOARD: Section = { label: 'Dashboard', path: '/' };

const SECTIONS: Section[] = [
  DASHBOARD,
  { label: 'VMs', path: '/vms', phase: 5 },
  { label: 'Zones', path: '/zones', phase: 4 },
  { label: 'Images', path: '/images' },
  { label: 'Network', path: '/network' },
  { label: 'Storage', path: '/storage' },
  {
    label: 'System',
    path: '/system/users',
    nav: {
      label: 'System',
      items: [
        { id: '/system/users', label: 'Users', icon: 'users' },
        { id: '/system/audit', label: 'Audit log', icon: 'clipboard' },
        { id: '/system/host', label: 'Host', icon: 'cog' },
        { id: '/system/updates', label: 'Updates', icon: 'download' },
      ],
    },
  },
];

function sectionFor(pathname: string): Section {
  if (pathname === '/') return DASHBOARD;
  const match = SECTIONS.find(
    (s) => s.path !== '/' && pathname.startsWith(s.path.split('/').slice(0, 2).join('/')),
  );
  return match ?? DASHBOARD;
}

export function Shell() {
  const session = useSession();
  const logout = useLogout();
  const navigate = useNavigate();
  const location = useLocation();
  const [theme, toggleTheme] = useTheme();

  if (session.isPending) {
    return (
      <div className="main-container">
        <div className="empty">
          <Spinner />
        </div>
      </div>
    );
  }
  if (session.isError) {
    if (isUnauthorized(session.error)) {
      return <Navigate to="/login" replace />;
    }
    return (
      <div className="main-container">
        <div className="empty">
          <clr-icon shape="disconnect" size="32"></clr-icon>
          <p>The daemon is not reachable. Retrying.</p>
        </div>
      </div>
    );
  }

  const actor = session.data.actor;
  const section = sectionFor(location.pathname);

  return (
    <div className="main-container">
      <Header
        logo={symbolOnDark}
        title="Mandrake"
        actions={
          <>
            <HeaderAction
              icon={theme === 'dark' ? 'sun' : 'moon'}
              label="Toggle theme"
              onClick={toggleTheme}
            />
            <Dropdown
              variant="link-neutral"
              right
              trigger={
                <span style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>
                  <clr-icon shape="user" size="16"></clr-icon>
                  {actor.username}
                </span>
              }
              items={[
                { header: `${actor.username} · ${actor.role}` },
                {
                  label: 'Sign out',
                  icon: 'logout',
                  onClick: () => {
                    logout.mutate();
                  },
                },
              ]}
            />
          </>
        }
      >
        <HeaderDivider />
      </Header>
      <Subnav
        items={SECTIONS.map((s) => ({ label: s.label, name: s.path, active: s === section }))}
        onNavigate={(item) => {
          if (item.name) void navigate(item.name);
        }}
      />
      <div className="content-container">
        {section.nav && (
          <VerticalNav
            collapsible
            activeId={location.pathname}
            groups={[section.nav]}
            onNavigate={(item) => {
              if (item.id) void navigate(item.id);
            }}
          />
        )}
        <main className="content-area">
          <Outlet context={{ actor }} />
        </main>
      </div>
    </div>
  );
}
