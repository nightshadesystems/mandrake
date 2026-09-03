// The snapshots tab of a VM: take, roll back, delete.

import { useState, type SyntheticEvent } from 'react';

import {
  useCreateVmSnapshot,
  useDeleteVmSnapshot,
  useRollbackVmSnapshot,
  useVmSnapshots,
  type Vm,
  type VmSnapshot,
} from '../../api/vms.ts';
import {
  Alert,
  Button,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Modal,
  Spinner,
} from '../../design/index.tsx';
import { bytes, timestamp } from '../../fmt.ts';
import { MetadataFields, NameCell } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody, problem } from '../common/util.ts';
import { isStopped } from './util.ts';

const SNAPSHOT_NAME = /^[a-zA-Z0-9][a-zA-Z0-9_.:-]{0,254}$/;

function TakeSnapshotModal({ vm, onClose }: { vm: Vm; onClose: () => void }) {
  const create = useCreateVmSnapshot();
  const [name, setName] = useState('');
  const [meta, setMeta] = useState(emptyMetadata());
  const valid = SNAPSHOT_NAME.test(name);
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      { id: vm.id, body: { name, ...(metadata ? { metadata } : {}) } },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      size="sm"
      title="Take a snapshot"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="snap-take"
            type="submit"
            loading={create.isPending}
            disabled={!valid}
          >
            Snapshot
          </Button>
        </>
      }
    >
      <form id="snap-take" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <FormField
          label="Name"
          required
          helper="Letters, digits, _ . : -"
          {...(name && !valid ? { error: 'Not a valid snapshot name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        {vm.state === 'running' && (
          <p className="field-note">
            The VM is running: the snapshot is crash-consistent, like a power cut.
          </p>
        )}
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function RollbackModal({
  vm,
  snapshot,
  onClose,
}: {
  vm: Vm;
  snapshot: VmSnapshot;
  onClose: () => void;
}) {
  const rollback = useRollbackVmSnapshot();
  return (
    <Modal
      open
      size="sm"
      title={`Roll back to ${snapshot.name}?`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={rollback.isPending}>
            Cancel
          </Button>
          <Button
            variant="danger"
            loading={rollback.isPending}
            onClick={() => {
              rollback.mutate({ id: vm.id, snapshot: snapshot.name }, { onSuccess: onClose });
            }}
          >
            Roll back
          </Button>
        </>
      }
    >
      <div className="form-stack">
        {rollback.error && (
          <Alert status="danger" sm>
            {problem(rollback.error)}
          </Alert>
        )}
        <p>
          Every disk returns to {snapshot.name} ({timestamp(snapshot.created_at)}). Changes since
          then, and any newer snapshots, are lost.
        </p>
      </div>
    </Modal>
  );
}

export function SnapshotsTab({ vm, canWrite }: { vm: Vm; canWrite: boolean }) {
  const snapshots = useVmSnapshots(vm.id);
  const remove = useDeleteVmSnapshot();
  const [taking, setTaking] = useState(false);
  const [rollingBack, setRollingBack] = useState<VmSnapshot | null>(null);
  const stopped = isStopped(vm);
  return (
    <div className="form-stack">
      {canWrite && (
        <div>
          <Button
            icon="camera"
            onClick={() => {
              setTaking(true);
            }}
          >
            Take snapshot
          </Button>
        </div>
      )}
      {remove.error && (
        <Alert status="danger" closable>
          {problem(remove.error)}
        </Alert>
      )}
      {snapshots.isError && (
        <Alert status="danger" closable>
          {problem(snapshots.error)}
        </Alert>
      )}
      {snapshots.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<VmSnapshot>
          rows={snapshots.data?.items ?? []}
          placeholder="No snapshots."
          columns={[
            {
              key: 'name',
              label: 'Snapshot',
              sortable: true,
              render: (s) => <NameCell name={s.name} metadata={s.metadata} />,
            },
            { key: 'created', label: 'Taken', render: (s) => timestamp(s.created_at) },
            { key: 'used', label: 'Used', render: (s) => bytes(s.used_bytes) },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (s) =>
                canWrite ? (
                  <Dropdown
                    trigger=""
                    variant="link-neutral"
                    sm
                    right
                    items={[
                      {
                        label: stopped ? 'Roll back' : 'Roll back (stop the VM first)',
                        icon: 'undo',
                        disabled: !stopped,
                        onClick: () => {
                          setRollingBack(s);
                        },
                      },
                      {
                        label: 'Delete',
                        icon: 'trash',
                        onClick: () => {
                          remove.mutate({ id: vm.id, snapshot: s.name });
                        },
                      },
                    ]}
                  />
                ) : null,
            },
          ]}
        />
      )}
      {taking && (
        <TakeSnapshotModal
          vm={vm}
          onClose={() => {
            setTaking(false);
          }}
        />
      )}
      {rollingBack && (
        <RollbackModal
          vm={vm}
          snapshot={rollingBack}
          onClose={() => {
            setRollingBack(null);
          }}
        />
      )}
    </div>
  );
}
