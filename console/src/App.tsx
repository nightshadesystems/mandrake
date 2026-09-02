import { createBrowserRouter, RouterProvider } from 'react-router';

import { Audit } from './pages/Audit.tsx';
import { Dashboard } from './pages/Dashboard.tsx';
import { Login } from './pages/Login.tsx';
import { Network } from './pages/Network.tsx';
import { NotYet } from './pages/NotYet.tsx';
import { Shell } from './pages/Shell.tsx';
import { Storage } from './pages/Storage.tsx';
import { Users } from './pages/Users.tsx';

const router = createBrowserRouter([
  { path: '/login', Component: Login },
  {
    path: '/',
    Component: Shell,
    children: [
      { index: true, Component: Dashboard },
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
