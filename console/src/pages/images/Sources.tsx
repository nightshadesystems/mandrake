import { useState, type SyntheticEvent } from 'react';

import {
  useCreateSource,
  useDeleteSource,
  useRefreshSource,
  useSources,
  useUpdateSource,
  type ImageSource,
} from '../../api/images.ts';
import {
  Alert,
  Button,
  Checkbox,
  Datagrid,
  Dropdown,
  type DropdownItem,
  FormField,
  Input,
  Label,
  Modal,
  Spinner,
} from '../../design/index.tsx';
import { timestamp } from '../../fmt.ts';
import { problem } from '../common/util.ts';

const URL_OK = /^https?:\/\/\S+$/;
const KEY_OK = /^[A-Za-z0-9+/]{43}=$/;

function SourceModal({ source, onClose }: { source?: ImageSource; onClose: () => void }) {
  const create = useCreateSource();
  const update = useUpdateSource();
  const [name, setName] = useState(source?.name ?? '');
  const [url, setUrl] = useState(source?.url ?? '');
  const [key, setKey] = useState(source?.public_key ?? '');
  const [enabled, setEnabled] = useState(source?.enabled ?? true);
  const pending = create.isPending || update.isPending;
  const error = create.error ?? update.error;
  const builtin = source?.builtin ?? false;
  const urlOk = URL_OK.test(url.trim());
  const keyOk = key.trim() === '' || KEY_OK.test(key.trim());
  const valid = name.trim() !== '' && urlOk && keyOk;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const trimmedKey = key.trim();
    if (source) {
      update.mutate(
        {
          id: source.id,
          body: {
            ...(builtin ? {} : { name: name.trim(), url: url.trim() }),
            public_key: trimmedKey === '' ? null : trimmedKey,
            enabled,
          },
        },
        { onSuccess: onClose },
      );
    } else {
      create.mutate(
        {
          name: name.trim(),
          url: url.trim(),
          ...(trimmedKey ? { public_key: trimmedKey } : {}),
          enabled,
        },
        { onSuccess: onClose },
      );
    }
  };
  return (
    <Modal
      open
      title={source ? `Edit ${source.name}` : 'Add an image source'}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={pending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="source-form"
            type="submit"
            loading={pending}
            disabled={!valid}
          >
            {source ? 'Save' : 'Add source'}
          </Button>
        </>
      }
    >
      <form id="source-form" className="form-stack" onSubmit={submit}>
        {error && (
          <Alert status="danger" sm>
            {problem(error)}
          </Alert>
        )}
        <FormField label="Name" required>
          <Input
            value={name}
            disabled={builtin}
            autoFocus={!source}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <FormField
          label="Index URL"
          required
          helper="The index.json; its signature is fetched from index.json.sig beside it"
          {...(url && !urlOk ? { error: 'http:// or https://' } : {})}
        >
          <Input
            value={url}
            disabled={builtin}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setUrl(e.target.value);
            }}
          />
        </FormField>
        <FormField
          label="Public key"
          helper="Base64 Ed25519 key from the publisher. Without one the source lists but cannot be imported from."
          {...(!keyOk ? { error: 'A base64 Ed25519 public key is 44 characters' } : {})}
        >
          <Input
            value={key}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setKey(e.target.value);
            }}
          />
        </FormField>
        <Checkbox
          label="Enabled"
          checked={enabled}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setEnabled(e.target.checked);
          }}
        />
      </form>
    </Modal>
  );
}

function DeleteSourceModal({ source, onClose }: { source: ImageSource; onClose: () => void }) {
  const remove = useDeleteSource();
  return (
    <Modal
      open
      size="sm"
      title={`Remove ${source.name}?`}
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
              remove.mutate(source.id, { onSuccess: onClose });
            }}
          >
            Remove
          </Button>
        </>
      }
    >
      <div className="form-stack">
        {remove.error && (
          <Alert status="danger" sm>
            {problem(remove.error)}
          </Alert>
        )}
        <p>Its catalogue disappears from Available. Images already imported stay.</p>
      </div>
    </Modal>
  );
}

export function Sources({ canWrite }: { canWrite: boolean }) {
  const sources = useSources();
  const refresh = useRefreshSource();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<ImageSource | null>(null);
  const [deleting, setDeleting] = useState<ImageSource | null>(null);
  const rows = sources.data?.items ?? [];

  return (
    <>
      <div className="toolbar">
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="plus-circle"
            onClick={() => {
              setAdding(true);
            }}
          >
            Add source
          </Button>
        )}
      </div>
      {refresh.error && (
        <Alert status="danger" closable>
          {problem(refresh.error)}
        </Alert>
      )}
      {sources.isError && (
        <Alert status="danger" closable>
          {problem(sources.error)}
        </Alert>
      )}
      {sources.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<ImageSource>
          rows={rows}
          placeholder="No sources."
          footerText={`${String(rows.length)} sources`}
          columns={[
            {
              key: 'name',
              label: 'Source',
              sortable: true,
              render: (s) => (
                <span className="name-cell">
                  {s.name}
                  {s.builtin && <Label>BUILT-IN</Label>}
                </span>
              ),
            },
            {
              key: 'url',
              label: 'Index',
              render: (s) => <span className="cell-mono">{s.url}</span>,
            },
            {
              key: 'state',
              label: 'State',
              render: (s) =>
                !s.enabled ? (
                  <Label>DISABLED</Label>
                ) : s.verified ? (
                  <Label status="success">VERIFIED</Label>
                ) : s.public_key ? (
                  <Label status="danger">NOT VERIFIED</Label>
                ) : (
                  <Label status="warning">NO KEY</Label>
                ),
            },
            {
              key: 'image_count',
              label: 'Images',
              render: (s) => <span className="cell-mono">{String(s.image_count)}</span>,
            },
            {
              key: 'last_refreshed_at',
              label: 'Refreshed',
              render: (s) => <span className="cell-mono">{timestamp(s.last_refreshed_at)}</span>,
            },
            {
              key: 'last_error',
              label: 'Last error',
              render: (s) =>
                s.last_error ? <span className="field-error">{s.last_error}</span> : '',
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (s) => {
                if (!canWrite) return null;
                const items: DropdownItem[] = [
                  {
                    label: 'Refresh now',
                    icon: 'refresh',
                    onClick: () => {
                      refresh.mutate(s.id);
                    },
                  },
                  {
                    label: 'Edit',
                    icon: 'pencil',
                    onClick: () => {
                      setEditing(s);
                    },
                  },
                ];
                if (!s.builtin) {
                  items.push({ divider: true });
                  items.push({
                    label: 'Remove',
                    icon: 'trash',
                    onClick: () => {
                      setDeleting(s);
                    },
                  });
                }
                return <Dropdown trigger="" variant="link-neutral" sm right items={items} />;
              },
            },
          ]}
        />
      )}
      {adding && (
        <SourceModal
          onClose={() => {
            setAdding(false);
          }}
        />
      )}
      {editing && (
        <SourceModal
          source={editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
      {deleting && (
        <DeleteSourceModal
          source={deleting}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
    </>
  );
}
