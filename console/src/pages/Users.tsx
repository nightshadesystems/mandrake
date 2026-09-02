import { useState, type SyntheticEvent } from 'react';
import { useOutletContext } from 'react-router';

import { ApiError } from '../api/client.ts';
import {
  useCreateUser,
  useDeleteUser,
  useSetPassword,
  useUpdateUser,
  useUsers,
  type Session,
  type User,
} from '../api/hooks.ts';
import {
  Alert,
  Button,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Label,
  Modal,
  Password,
  Select,
  Spinner,
} from '../design/index.tsx';
import { timestamp } from '../fmt.ts';

type Role = User['role'];
const ROLES: Role[] = ['admin', 'operator', 'viewer'];
const MIN_PASSWORD = 12;

function problem(error: unknown): string {
  return error instanceof ApiError ? error.message : 'Request failed.';
}

function StateLabel({ user }: { user: User }) {
  if (user.disabled) return <Label status="danger">DISABLED</Label>;
  if (user.locked_until) return <Label status="warning">LOCKED</Label>;
  return <Label status="success">ACTIVE</Label>;
}

interface EditState {
  mode: 'create' | 'edit';
  user?: User;
}

function UserModal({ state, onClose }: { state: EditState; onClose: () => void }) {
  const create = useCreateUser();
  const update = useUpdateUser();
  const editing = state.user;
  const [username, setUsername] = useState(editing?.username ?? '');
  const [displayName, setDisplayName] = useState(editing?.display_name ?? '');
  const [role, setRole] = useState<Role>(editing?.role ?? 'viewer');
  const [password, setPassword] = useState('');
  const pending = create.isPending || update.isPending;
  const error = create.error ?? update.error;

  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    if (editing) {
      update.mutate(
        { id: editing.id, body: { role, display_name: displayName } },
        { onSuccess: onClose },
      );
    } else {
      create.mutate(
        { username, password, role, ...(displayName ? { display_name: displayName } : {}) },
        { onSuccess: onClose },
      );
    }
  };

  const valid = editing ? true : username !== '' && password.length >= MIN_PASSWORD;

  return (
    <Modal
      open
      title={editing ? `Edit ${editing.username}` : 'New user'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={pending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="user-form"
            type="submit"
            loading={pending}
            disabled={!valid}
          >
            {editing ? 'Save' : 'Create user'}
          </Button>
        </>
      }
    >
      <form id="user-form" className="form-stack" onSubmit={submit}>
        {error && (
          <Alert status="danger" sm>
            {problem(error)}
          </Alert>
        )}
        <FormField
          label="Username"
          required
          {...(editing ? {} : { helper: 'Lowercase letters, digits, _ . -; up to 32 characters' })}
        >
          <Input
            value={username}
            disabled={Boolean(editing)}
            autoFocus={!editing}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setUsername(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Display name">
          <Input
            value={displayName}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setDisplayName(e.target.value);
            }}
          />
        </FormField>
        <FormField
          label="Role"
          helper="viewer reads · operator changes infrastructure · admin manages users"
        >
          <Select
            value={role}
            options={ROLES}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setRole(e.target.value as Role);
            }}
          />
        </FormField>
        {!editing && (
          <FormField
            label="Password"
            required
            helper={`At least ${String(MIN_PASSWORD)} characters`}
          >
            <Password
              value={password}
              autoComplete="new-password"
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setPassword(e.target.value);
              }}
            />
          </FormField>
        )}
      </form>
    </Modal>
  );
}

function PasswordModal({
  user,
  self,
  onClose,
}: {
  user: User;
  self: boolean;
  onClose: () => void;
}) {
  const set = useSetPassword();
  const [current, setCurrent] = useState('');
  const [next, setNext] = useState('');
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    set.mutate(
      { id: user.id, body: { new_password: next, ...(self ? { current_password: current } : {}) } },
      { onSuccess: onClose },
    );
  };
  const valid = next.length >= MIN_PASSWORD && (!self || current !== '');
  return (
    <Modal
      open
      size="sm"
      title={`Set password for ${user.username}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={set.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="password-form"
            type="submit"
            loading={set.isPending}
            disabled={!valid}
          >
            Set password
          </Button>
        </>
      }
    >
      <form id="password-form" className="form-stack" onSubmit={submit}>
        {set.error && (
          <Alert status="danger" sm>
            {problem(set.error)}
          </Alert>
        )}
        {self && (
          <FormField label="Current password" required>
            <Password
              value={current}
              autoComplete="current-password"
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setCurrent(e.target.value);
              }}
            />
          </FormField>
        )}
        <FormField
          label="New password"
          required
          helper={`At least ${String(MIN_PASSWORD)} characters`}
        >
          <Password
            value={next}
            autoComplete="new-password"
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setNext(e.target.value);
            }}
          />
        </FormField>
        {!self && <p style={{ fontSize: 13 }}>Every other session of this user ends.</p>}
      </form>
    </Modal>
  );
}

function DeleteModal({ user, onClose }: { user: User; onClose: () => void }) {
  const remove = useDeleteUser();
  return (
    <Modal
      open
      size="sm"
      title={`Delete ${user.username}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={remove.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={remove.isPending}
            onClick={() => {
              remove.mutate(user.id, { onSuccess: onClose });
            }}
          >
            Delete
          </Button>
        </>
      }
    >
      {remove.error && (
        <Alert status="danger" sm>
          {problem(remove.error)}
        </Alert>
      )}
      <p>Sessions end and tokens are revoked immediately. Audit entries keep the username.</p>
    </Modal>
  );
}

export function Users() {
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const users = useUsers();
  const update = useUpdateUser();
  const [edit, setEdit] = useState<EditState | null>(null);
  const [passwordFor, setPasswordFor] = useState<User | null>(null);
  const [deleting, setDeleting] = useState<User | null>(null);
  const isAdmin = actor.role === 'admin';

  const rows = users.data?.items ?? [];

  return (
    <>
      <div className="page-header">
        <h1>Users</h1>
        <span className="spacer" />
        {isAdmin && (
          <Button
            variant="primary"
            icon="plus-circle"
            onClick={() => {
              setEdit({ mode: 'create' });
            }}
          >
            New user
          </Button>
        )}
      </div>
      {update.error && (
        <Alert status="danger" closable>
          {problem(update.error)}
        </Alert>
      )}
      {users.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<User>
          rows={rows}
          placeholder="No users. Create one, or use mandrakectl as root on the host."
          footerText={`${String(rows.length)} users`}
          columns={[
            {
              key: 'username',
              label: 'Username',
              sortable: true,
              render: (u) => <span className="cell-mono">{u.username}</span>,
            },
            { key: 'display_name', label: 'Display name', render: (u) => u.display_name ?? '' },
            { key: 'role', label: 'Role', sortable: true },
            { key: 'state', label: 'State', render: (u) => <StateLabel user={u} /> },
            {
              key: 'last_login_at',
              label: 'Last login',
              sortable: true,
              render: (u) => <span className="cell-mono">{timestamp(u.last_login_at)}</span>,
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (u) => {
                const self = u.id === actor.id;
                const items = [];
                if (isAdmin) {
                  items.push({
                    label: 'Edit',
                    icon: 'pencil',
                    onClick: () => {
                      setEdit({ mode: 'edit', user: u });
                    },
                  });
                }
                if (isAdmin || self) {
                  items.push({
                    label: 'Set password',
                    icon: 'key',
                    onClick: () => {
                      setPasswordFor(u);
                    },
                  });
                }
                if (isAdmin && !self) {
                  items.push({
                    label: u.disabled ? 'Enable' : 'Disable',
                    icon: u.disabled ? 'check' : 'ban',
                    onClick: () => {
                      update.mutate({ id: u.id, body: { disabled: !u.disabled } });
                    },
                  });
                  items.push({ divider: true });
                  items.push({
                    label: 'Delete',
                    icon: 'trash',
                    onClick: () => {
                      setDeleting(u);
                    },
                  });
                }
                return items.length > 0 ? (
                  <Dropdown trigger="" variant="link-neutral" sm right items={items} />
                ) : null;
              },
            },
          ]}
        />
      )}
      {edit && (
        <UserModal
          state={edit}
          onClose={() => {
            setEdit(null);
          }}
        />
      )}
      {passwordFor && (
        <PasswordModal
          user={passwordFor}
          self={passwordFor.id === actor.id}
          onClose={() => {
            setPasswordFor(null);
          }}
        />
      )}
      {deleting && (
        <DeleteModal
          user={deleting}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
    </>
  );
}
