import { createBrowserRouter, RouterProvider } from 'react-router';

import { Audit } from './pages/Audit.tsx';
import { Dashboard } from './pages/Dashboard.tsx';
import { Images } from './pages/Images.tsx';
import { Login } from './pages/Login.tsx';
import { Network } from './pages/Network.tsx';
import { NotYet } from './pages/NotYet.tsx';
import { Shell } from './pages/Shell.tsx';
import { Storage } from './pages/Storage.tsx';
import { Users } from './pages/Users.tsx';
import { Vms } from './pages/Vms.tsx';
import { VmDetail } from './pages/vms/VmDetail.tsx';
import { Zones } from './pages/Zones.tsx';
import { ZoneDetail } from './pages/zones/ZoneDetail.tsx';

const router = createBrowserRouter([
  { path: '/login', Component: Login },
  {
    path: '/',
    Component: Shell,
    children: [
      { index: true, Component: Dashboard },
      { path: 'vms', Component: Vms },
      { path: 'vms/:id', Component: VmDetail },
      { path: 'zones', Component: Zones },
      { path: 'zones/:id', Component: ZoneDetail },
      { path: 'images', Component: Images },
      { path: 'network', Component: Network },
      { path: 'storage', Component: Storage },
      { path: 'system/users', Component: Users },
      { path: 'system/audit', Component: Audit },
      { path: '*', Component: NotYet },
    ],
  },
]);

export function App() {
  return <RouterProvider router={router} />;
}
