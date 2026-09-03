import { useState, type SyntheticEvent } from 'react';

import {
  isInFlight,
  useDeleteImage,
  useImages,
  useImportImage,
  useUpdateImage,
  type Image,
  type ImageType,
} from '../../api/images.ts';
import {
  Alert,
  Button,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Label,
  Modal,
  ProgressBar,
  Select,
  Spinner,
} from '../../design/index.tsx';
import { bytes, timestamp } from '../../fmt.ts';
import { MetadataFields, NameCell } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody, problem } from '../common/util.ts';
import { TYPE_LABEL } from './util.ts';

const TYPES: ImageType[] = ['zone-lx', 'zone-native', 'vm-raw', 'vm-iso'];

export function StateLabel({ image }: { image: Image }) {
  switch (image.state) {
    case 'ready':
      return <Label status="success">READY</Label>;
    case 'failed':
      return <Label status="danger">FAILED</Label>;
    default:
      return <Label status="info">{image.state.toUpperCase()}</Label>;
  }
}

function ImportUrlModal({ onClose }: { onClose: () => void }) {
  const importImage = useImportImage();
  const [name, setName] = useState('');
  const [version, setVersion] = useState('');
  const [type, setType] = useState<ImageType>('zone-lx');
  const [url, setUrl] = useState('');
  const [sha256, setSha256] = useState('');
  const [pool, setPool] = useState('');
  const [meta, setMeta] = useState(emptyMetadata());
  const shaOk = /^[0-9a-fA-F]{64}$/.test(sha256.trim());
  const urlOk = /^https?:\/\/\S+$/.test(url.trim());
  const valid = name.trim() !== '' && version.trim() !== '' && urlOk && shaOk;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    importImage.mutate(
      {
        name: name.trim(),
        version: version.trim(),
        type,
        url: url.trim(),
        sha256: sha256.trim().toLowerCase(),
        ...(pool.trim() ? { pool: pool.trim() } : {}),
        ...(metadata ? { metadata } : {}),
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="Import from a URL"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={importImage.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="import-form"
            type="submit"
            loading={importImage.isPending}
            disabled={!valid}
          >
            Import
          </Button>
        </>
      }
    >
      <form id="import-form" className="form-stack" onSubmit={submit}>
        {importImage.error && (
          <Alert status="danger" sm>
            {problem(importImage.error)}
          </Alert>
        )}
        <Alert status="info" sm>
          You vouch for the hash: the payload is verified against it, not against a signed index.
        </Alert>
        <div className="form-row">
          <FormField label="Name" required>
            <Input
              value={name}
              autoFocus
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setName(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Version" required>
            <Input
              value={version}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setVersion(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Type" required>
            <Select
              value={type}
              options={TYPES.map((t) => ({ value: t, label: TYPE_LABEL[t] }))}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setType(e.target.value as ImageType);
              }}
            />
          </FormField>
        </div>
        <FormField
          label="URL"
          required
          helper="gzip and xz payloads are decompressed on import"
          {...(url && !urlOk ? { error: 'http:// or https://' } : {})}
        >
          <Input
            value={url}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setUrl(e.target.value);
            }}
          />
        </FormField>
        <FormField
          label="SHA-256"
          required
          helper="64 hex characters, of the payload as published"
          {...(sha256 && !shaOk ? { error: 'Not a SHA-256 hex digest' } : {})}
        >
          <Input
            value={sha256}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setSha256(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Pool" helper="Empty: the data pool with the most free space">
          <Input
            value={pool}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setPool(e.target.value);
            }}
          />
        </FormField>
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function EditImageModal({ image, onClose }: { image: Image; onClose: () => void }) {
  const update = useUpdateImage();
  const [meta, setMeta] = useState(emptyMetadata(image.metadata));
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    update.mutate({ id: image.id, body: metadataBody(meta) ?? {} }, { onSuccess: onClose });
  };
  return (
    <Modal
      open
      title={`Edit ${image.name}@${image.version}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={update.isPending}>
            Cancel
          </Button>
          <Button variant="primary" form="image-edit" type="submit" loading={update.isPending}>
            Save
          </Button>
        </>
      }
    >
      <form id="image-edit" className="form-stack" onSubmit={submit}>
        {update.error && (
          <Alert status="danger" sm>
            {problem(update.error)}
          </Alert>
        )}
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function DeleteImageModal({ image, onClose }: { image: Image; onClose: () => void }) {
  const remove = useDeleteImage();
  const inUse = (image.in_use_by ?? 0) > 0;
  return (
    <Modal
      open
      size="sm"
      title={`Delete ${image.name}@${image.version}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={remove.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={remove.isPending}
            disabled={inUse}
            onClick={() => {
              remove.mutate(image.id, { onSuccess: onClose });
            }}
          >
            Delete
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
        {inUse ? (
          <p>Refused: {String(image.in_use_by)} zone(s) or VM(s) are cloned from this image.</p>
        ) : isInFlight(image.state) ? (
          <p>The import is cancelled and whatever it fetched is removed.</p>
        ) : (
          <p>The dataset or file is destroyed. Re-importing fetches it again.</p>
        )}
      </div>
    </Modal>
  );
}

export function Imported({ canWrite }: { canWrite: boolean }) {
  const images = useImages();
  const [importing, setImporting] = useState(false);
  const [editing, setEditing] = useState<Image | null>(null);
  const [deleting, setDeleting] = useState<Image | null>(null);
  const rows = images.data?.items ?? [];

  return (
    <>
      <div className="toolbar">
        <span className="spacer" />
        {canWrite && (
          <Button
            icon="download"
            onClick={() => {
              setImporting(true);
            }}
          >
            Import from URL
          </Button>
        )}
      </div>
      {images.isError && (
        <Alert status="danger" closable>
          {problem(images.error)}
        </Alert>
      )}
      {images.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Image>
          rows={rows}
          placeholder="No images yet. Import one from a source under Available, or from a URL."
          footerText={`${String(rows.length)} images`}
          columns={[
            {
              key: 'name',
              label: 'Image',
              sortable: true,
              render: (i) => <NameCell name={`${i.name}@${i.version}`} metadata={i.metadata} />,
            },
            { key: 'type', label: 'Type', sortable: true, render: (i) => TYPE_LABEL[i.type] },
            {
              key: 'state',
              label: 'State',
              render: (i) =>
                isInFlight(i.state) ? (
                  <div className="capacity-cell">
                    <ProgressBar value={Math.round((i.progress ?? 0) * 100)} max={100} sm />
                    <span className="cell-mono">{i.state}</span>
                  </div>
                ) : (
                  <span className="name-cell">
                    <StateLabel image={i} />
                    {i.error && <span className="field-error">{i.error}</span>}
                  </span>
                ),
            },
            { key: 'os', label: 'OS', render: (i) => i.os ?? '-' },
            {
              key: 'size_bytes',
              label: 'Size',
              sortable: true,
              render: (i) => <span className="cell-mono">{bytes(i.size_bytes)}</span>,
            },
            {
              key: 'dataset',
              label: 'Location',
              render: (i) => <span className="cell-mono">{i.dataset ?? i.path ?? '-'}</span>,
            },
            {
              key: 'in_use_by',
              label: 'Clones',
              render: (i) => <span className="cell-mono">{String(i.in_use_by ?? 0)}</span>,
            },
            {
              key: 'source_name',
              label: 'Source',
              render: (i) => i.source_name ?? 'URL',
            },
            {
              key: 'imported_at',
              label: 'Imported',
              sortable: true,
              render: (i) => <span className="cell-mono">{timestamp(i.imported_at)}</span>,
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (i) =>
                canWrite ? (
                  <Dropdown
                    trigger=""
                    variant="link-neutral"
                    sm
                    right
                    items={[
                      {
                        label: 'Edit',
                        icon: 'pencil',
                        onClick: () => {
                          setEditing(i);
                        },
                      },
                      { divider: true },
                      {
                        label: 'Delete',
                        icon: 'trash',
                        onClick: () => {
                          setDeleting(i);
                        },
                      },
                    ]}
                  />
                ) : null,
            },
          ]}
        />
      )}
      {importing && (
        <ImportUrlModal
          onClose={() => {
            setImporting(false);
          }}
        />
      )}
      {editing && (
        <EditImageModal
          image={editing}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
      {deleting && (
        <DeleteImageModal
          image={deleting}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
    </>
  );
}
