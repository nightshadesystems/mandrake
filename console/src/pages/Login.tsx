import { useState, type SyntheticEvent } from 'react';
import { useNavigate } from 'react-router';

import symbolOnDark from '../../design/assets/logos/nightshade-symbol-on-dark.svg';
import symbolPrimary from '../../design/assets/logos/nightshade-symbol-primary.svg';
import { ApiError } from '../api/client.ts';
import { useLogin } from '../api/hooks.ts';
import { Alert, Button, FormField, Input, Password } from '../design/index.tsx';

function explain(error: unknown): string {
  if (error instanceof ApiError) {
    if (error.slug === 'invalid-credentials') return 'Invalid username or password.';
    if (error.slug === 'locked')
      return 'Too many failed attempts. This account is locked for 15 minutes.';
    if (error.status === 429)
      return 'Too many attempts from this address. Wait a minute and try again.';
    if (error.status === 0) return 'The daemon is not reachable.';
    return error.message;
  }
  return 'Sign-in failed.';
}

export function Login() {
  const navigate = useNavigate();
  const login = useLogin();
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    login.mutate(
      { username, password },
      {
        onSuccess: () => {
          void navigate('/', { replace: true });
        },
      },
    );
  };

  return (
    <div className="login-wrapper">
      <div className="login-brand">
        <img src={symbolOnDark} alt="Nightshade Systems" />
        <div className="login-brand-title">
          Every VM,
          <br />
          accounted for.
        </div>
        <div className="login-brand-sub">
          Mandrake runs bhyve virtual machines and illumos zones on a native ZFS root. Everything on
          this host is managed from here or from mandrakectl; both speak the same API.
        </div>
      </div>
      <form className="login" onSubmit={submit}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, marginBottom: 4 }}>
          <img src={symbolPrimary} alt="" style={{ height: 28 }} />
          <span
            className="wordmark"
            style={{ fontSize: 22, color: 'var(--cds-alias-typography-color-450)' }}
          >
            Mandrake
          </span>
        </div>
        <div className="title">Console</div>
        <div className="subtitle">Sign in with a local account</div>
        {login.isError && (
          <div className="error">
            <Alert status="danger" sm>
              {explain(login.error)}
            </Alert>
          </div>
        )}
        <FormField label="Username" htmlFor="username">
          <Input
            id="username"
            name="username"
            autoComplete="username"
            autoFocus
            value={username}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setUsername(e.target.value);
            }}
            style={{ maxWidth: 'none' }}
          />
        </FormField>
        <FormField label="Password" htmlFor="password">
          <Password
            id="password"
            name="password"
            autoComplete="current-password"
            value={password}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setPassword(e.target.value);
            }}
            style={{ maxWidth: 'none' }}
          />
        </FormField>
        <Button
          type="submit"
          variant="primary"
          block
          loading={login.isPending}
          disabled={login.isPending || username === '' || password === ''}
        >
          Sign in
        </Button>
        <div className="signup">
          Locked out? As root on the host, <span className="mono">mandrakectl users</span> can reset
          a password over the local socket.
        </div>
      </form>
    </div>
  );
}
