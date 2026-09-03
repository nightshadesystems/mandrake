// The disks and media tabs of a VM: add, grow, detach; attach and eject ISOs.

import { useState, type SyntheticEvent } from 'react';

import { useImages, type Image } from '../../api/images.ts';
import {
  useAddDisk,
  useAttachCdrom,
  useDetachCdrom,
  useRemoveDisk,
  useResizeDisk,
  type Vm,
  type VmDisk,
} from '../../api/vms.ts';
import {
  Alert,
  Button,
  Checkbox,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Modal,
  Select,
} from '../../design/index.tsx';
import { bytes } from '../../fmt.ts';
import { problem } from '../common/util.ts';
import { diskSize } from './util.ts';

function imageLabel(i: Image | undefined, fallback: string): string {
  return i ? `${i.name}@${i.version}` : fallback;
}

function AddDiskModal({ vm, onClose }: { vm: Vm; onClose: () => void }) {
  const add = useAddDisk();
  const images = useImages();
  const [imageId, setImageId] = useState('');
  const [size, setSize] = useState('50G');
  const candidates = (images.data?.items ?? []).filter(
    (i) => i.state === 'ready' && i.type === 'vm-raw',
  );
  const sizeBytes = diskSize(size);
  const valid = imageId !== '' || sizeBytes !== undefined;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    add.mutate(
      {
        id: vm.id,
        body: imageId ? { image_id: imageId } : { size_bytes: sizeBytes ?? 0 },
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      size="sm"
      title="Add a disk"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={add.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="disk-add"
            type="submit"
            loading={add.isPending}
            disabled={!valid}
          >
            Add
          </Button>
        </>
      }
    >
      <form id="disk-add" className="form-stack" onSubmit={submit}>
        {add.error && (
          <Alert status="danger" sm>
            {problem(add.error)}
          </Alert>
        )}
        <FormField label="Clone an image" helper="Empty: a blank disk">
          <Select
            value={imageId}
            options={[
              { value: '', label: 'Blank disk' },
              ...candidates.map((i) => ({ value: i.id, label: imageLabel(i, i.id) })),
            ]}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setImageId(e.target.value);
            }}
          />
        </FormField>
        {imageId === '' && (
          <FormField label="Size" required helper="e.g. 50G">
            <Input
              value={size}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setSize(e.target.value);
              }}
            />
          </FormField>
        )}
        {vm.state === 'running' && (
          <p className="field-note">The guest sees the new disk at its next boot.</p>
        )}
      </form>
    </Modal>
  );
}

function GrowDiskModal({ vm, disk, onClose }: { vm: Vm; disk: VmDisk; onClose: () => void }) {
  const resize = useResizeDisk();
  const [size, setSize] = useState('');
  const sizeBytes = diskSize(size);
  const valid = sizeBytes !== undefined && sizeBytes > disk.size_bytes;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    if (sizeBytes === undefined) return;
    resize.mutate({ id: vm.id, index: disk.index, sizeBytes }, { onSuccess: onClose });
  };
  return (
    <Modal
      open
      size="sm"
      title={`Grow disk ${String(disk.index)}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={resize.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="disk-grow"
            type="submit"
            loading={resize.isPending}
            disabled={!valid}
          >
            Grow
          </Button>
        </>
      }
    >
      <form id="disk-grow" className="form-stack" onSubmit={submit}>
        {resize.error && (
          <Alert status="danger" sm>
            {problem(resize.error)}
          </Alert>
        )}
        <FormField
          label="New size"
          required
          helper={`Now ${bytes(disk.size_bytes)}; volumes only grow`}
          {...(size && !valid ? { error: 'Must be larger than the current size' } : {})}
        >
          <Input
            value={size}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setSize(e.target.value);
            }}
          />
        </FormField>
      </form>
    </Modal>
  );
}

function RemoveDiskModal({ vm, disk, onClose }: { vm: Vm; disk: VmDisk; onClose: () => void }) {
  const remove = useRemoveDisk();
  const [purge, setPurge] = useState(false);
  return (
    <Modal
      open
      size="sm"
      title={`Detach disk ${String(disk.index)}?`}
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
              remove.mutate({ id: vm.id, index: disk.index, purge }, { onSuccess: onClose });
            }}
          >
            Detach
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
        <p>The disk leaves the VM's configuration. Its volume stays unless you destroy it.</p>
        <Checkbox
          label={`Also destroy ${disk.dataset}`}
          checked={purge}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setPurge(e.target.checked);
          }}
        />
      </div>
    </Modal>
  );
}

export function DisksTab({ vm, canWrite }: { vm: Vm; canWrite: boolean }) {
  const images = useImages();
  const [adding, setAdding] = useState(false);
  const [growing, setGrowing] = useState<VmDisk | null>(null);
  const [removing, setRemoving] = useState<VmDisk | null>(null);
  const byId = new Map((images.data?.items ?? []).map((i) => [i.id, i]));
  return (
    <div className="form-stack">
      {canWrite && (
        <div>
          <Button
            icon="plus-circle"
            onClick={() => {
              setAdding(true);
            }}
          >
            Add disk
          </Button>
        </div>
      )}
      <Datagrid<VmDisk>
        rows={vm.disks}
        placeholder="No disks."
        columns={[
          { key: 'index', label: '#', width: 48, render: (d) => String(d.index) },
          {
            key: 'dataset',
            label: 'Volume',
            render: (d) => <span className="cell-mono">{d.dataset}</span>,
          },
          { key: 'size', label: 'Size', render: (d) => bytes(d.size_bytes) },
          { key: 'boot', label: 'Boot', render: (d) => (d.boot ? 'yes' : '') },
          {
            key: 'image',
            label: 'Image',
            render: (d) => (d.image_id ? imageLabel(byId.get(d.image_id), d.image_id) : '-'),
          },
          {
            key: 'actions',
            label: '',
            width: 48,
            render: (d) =>
              canWrite ? (
                <Dropdown
                  trigger=""
                  variant="link-neutral"
                  sm
                  right
                  items={[
                    {
                      label: 'Grow',
                      icon: 'resize',
                      onClick: () => {
                        setGrowing(d);
                      },
                    },
                    {
                      label: 'Detach',
                      icon: 'trash',
                      disabled: d.boot,
                      onClick: () => {
                        setRemoving(d);
                      },
                    },
                  ]}
                />
              ) : null,
          },
        ]}
      />
      {adding && (
        <AddDiskModal
          vm={vm}
          onClose={() => {
            setAdding(false);
          }}
        />
      )}
      {growing && (
        <GrowDiskModal
          vm={vm}
          disk={growing}
          onClose={() => {
            setGrowing(null);
          }}
        />
      )}
      {removing && (
        <RemoveDiskModal
          vm={vm}
          disk={removing}
          onClose={() => {
            setRemoving(null);
          }}
        />
      )}
    </div>
  );
}

function AttachIsoModal({ vm, onClose }: { vm: Vm; onClose: () => void }) {
  const attach = useAttachCdrom();
  const images = useImages();
  const [imageId, setImageId] = useState('');
  const attached = new Set(vm.cdroms.map((c) => c.image_id));
  const candidates = (images.data?.items ?? []).filter(
    (i) => i.state === 'ready' && i.type === 'vm-iso' && !attached.has(i.id),
  );
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    attach.mutate({ id: vm.id, imageId }, { onSuccess: onClose });
  };
  return (
    <Modal
      open
      size="sm"
      title="Attach an ISO"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={attach.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="iso-attach"
            type="submit"
            loading={attach.isPending}
            disabled={imageId === ''}
          >
            Attach
          </Button>
        </>
      }
    >
      <form id="iso-attach" className="form-stack" onSubmit={submit}>
        {attach.error && (
          <Alert status="danger" sm>
            {problem(attach.error)}
          </Alert>
        )}
        <FormField label="ISO" required>
          <Select
            value={imageId}
            options={[
              { value: '', label: 'Choose an ISO' },
              ...candidates.map((i) => ({ value: i.id, label: imageLabel(i, i.id) })),
            ]}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setImageId(e.target.value);
            }}
          />
        </FormField>
        {vm.state === 'running' && (
          <p className="field-note">The guest sees the change at its next boot.</p>
        )}
      </form>
    </Modal>
  );
}

export function MediaTab({ vm, canWrite }: { vm: Vm; canWrite: boolean }) {
  const images = useImages();
  const detach = useDetachCdrom();
  const [attaching, setAttaching] = useState(false);
  const byId = new Map((images.data?.items ?? []).map((i) => [i.id, i]));
  return (
    <div className="form-stack">
      {canWrite && (
        <div>
          <Button
            icon="plus-circle"
            onClick={() => {
              setAttaching(true);
            }}
          >
            Attach ISO
          </Button>
        </div>
      )}
      {detach.error && (
        <Alert status="danger" closable>
          {problem(detach.error)}
        </Alert>
      )}
      <Datagrid<Vm['cdroms'][number]>
        rows={vm.cdroms}
        placeholder="No ISOs attached."
        columns={[
          { key: 'index', label: '#', width: 48, render: (c) => String(c.index) },
          {
            key: 'image',
            label: 'Image',
            render: (c) => imageLabel(byId.get(c.image_id), c.image_id),
          },
          {
            key: 'path',
            label: 'File',
            render: (c) => <span className="cell-mono">{c.path}</span>,
          },
          {
            key: 'actions',
            label: '',
            width: 48,
            render: (c) =>
              canWrite ? (
                <Button
                  variant="link-neutral"
                  icon="eject"
                  sm
                  loading={detach.isPending}
                  onClick={() => {
                    detach.mutate({ id: vm.id, index: c.index });
                  }}
                >
                  Eject
                </Button>
              ) : null,
          },
        ]}
      />
      {attaching && (
        <AttachIsoModal
          vm={vm}
          onClose={() => {
            setAttaching(false);
          }}
        />
      )}
    </div>
  );
}
