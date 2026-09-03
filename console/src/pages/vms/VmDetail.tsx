// One VM: properties, lifecycle actions, edit, delete, disks, media, snapshots.

import { useCallback, useState, type SyntheticEvent } from 'react';
import { useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useOutletContext, useParams } from 'react-router';

import { useEvents } from '../../api/events.ts';
import type { Event, Session } from '../../api/hooks.ts';
import {
  useDeleteVm,
  useUpdateVm,
  useVm,
  useVmAction,
  vmKeys,
  type Bootrom,
  type Vm,
} from '../../api/vms.ts';
import type { ZoneNic } from '../../api/zones.ts';
import {
  Alert,
  Button,
  Checkbox,
  Dropdown,
  FormField,
  Input,
  Modal,
  Select,
  Spinner,
  StackView,
  Tabs,
} from '../../design/index.tsx';
import { bytes } from '../../fmt.ts';
import { MetadataFields } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody, problem } from '../common/util.ts';
import { parseSize } from '../storage/util.ts';
import { NicEditor } from '../zones/NicEditor.tsx';
import { nicErrors } from '../zones/util.ts';
import { ConsoleTerminal } from '../zones/Terminal.tsx';
import { ZoneStateLabel } from '../zones/ZoneDetail.tsx';
import { DisksTab, MediaTab } from './Disks.tsx';
import { SnapshotsTab } from './Snapshots.tsx';
import { VmDisplay } from './Vnc.tsx';
import { BOOTROMS, canStart, sizingErrors } from './util.ts';

function EditVmModal({ vm, onClose }: { vm: Vm; onClose: () => void }) {
  const update = useUpdateVm();
  const [vcpus, setVcpus] = useState(String(vm.vcpus));
  const [memory, setMemory] = useState(String(vm.memory_bytes));
  const [bootrom, setBootrom] = useState<Bootrom>(vm.bootrom);
  const [acpi, setAcpi] = useState(vm.acpi);
  const [vnc, setVnc] = useState(vm.vnc);
  const [autoboot, setAutoboot] = useState(vm.autoboot);
  const [nics, setNics] = useState<ZoneNic[]>(vm.nics);
  const [meta, setMeta] = useState(emptyMetadata(vm.metadata));
  const errors = [...sizingErrors(vcpus, memory), ...nicErrors(nics)];
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    update.mutate(
      {
        id: vm.id,
        body: {
          vcpus: Number(vcpus),
          memory_bytes: parseSize(memory) ?? vm.memory_bytes,
          bootrom,
          acpi,
          vnc,
          autoboot,
          nics,
          ...(metadata ? { metadata } : {}),
        },
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      size="lg"
      title={`Edit ${vm.name}`}
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={update.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="vm-edit"
            type="submit"
            loading={update.isPending}
            disabled={errors.length > 0}
          >
            Save
          </Button>
        </>
      }
    >
      <form id="vm-edit" className="form-stack" onSubmit={submit}>
        {update.error && (
          <Alert status="danger" sm>
            {problem(update.error)}
          </Alert>
        )}
        {vm.state === 'running' && (
          <Alert status="info" sm>
            Sizing, firmware, VNC, and NIC changes apply at the next boot; autoboot and metadata at
            once.
          </Alert>
        )}
        <div className="form-row">
          <FormField label="vCPUs" required>
            <Input
              value={vcpus}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setVcpus(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Memory" required helper="e.g. 4G">
            <Input
              value={memory}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setMemory(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Firmware">
            <Select
              value={bootrom}
              options={BOOTROMS}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setBootrom(e.target.value as Bootrom);
              }}
            />
          </FormField>
        </div>
        <Checkbox
          label="ACPI"
          checked={acpi}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setAcpi(e.target.checked);
          }}
        />
        <Checkbox
          label="VNC display"
          checked={vnc}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setVnc(e.target.checked);
          }}
        />
        <Checkbox
          label="Boot with the host (autoboot)"
          checked={autoboot}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setAutoboot(e.target.checked);
          }}
        />
        <NicEditor nics={nics} onChange={setNics} />
        {errors.length > 0 && (
          <Alert
            status="warning"
            items={errors.map((e) => (
              <span key={e}>{e}</span>
            ))}
          />
        )}
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function DeleteVmModal({ vm, onClose }: { vm: Vm; onClose: () => void }) {
  const remove = useDeleteVm();
  const navigate = useNavigate();
  const [purge, setPurge] = useState(false);
  return (
    <Modal
      open
      size="sm"
      title={`Delete ${vm.name}?`}
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
              remove.mutate(
                { id: vm.id, purge },
                {
                  onSuccess: () => {
                    void navigate('/vms');
                  },
                },
              );
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
        <p>
          The VM is halted and its configuration removed. Its disks and snapshots stay unless you
          purge them.
        </p>
        <Checkbox
          label={`Also destroy ${vm.dataset ?? 'its dataset'} and every disk`}
          checked={purge}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setPurge(e.target.checked);
          }}
        />
      </div>
    </Modal>
  );
}

function Overview({ vm }: { vm: Vm }) {
  return (
    <StackView
      blocks={[
        { key: 'State', value: <ZoneStateLabel state={vm.state} /> },
        { key: 'vCPUs', value: String(vm.vcpus) },
        { key: 'Memory', value: bytes(vm.memory_bytes) },
        {
          key: 'Firmware',
          value: BOOTROMS.find((b) => b.value === vm.bootrom)?.label ?? vm.bootrom,
        },
        { key: 'ACPI', value: vm.acpi ? 'yes' : 'no' },
        { key: 'VNC', value: vm.vnc ? 'on' : 'off' },
        { key: 'Autoboot', value: vm.autoboot ? 'yes' : 'no' },
        ...(vm.image_id ? [{ key: 'Image', value: <Link to="/images">{vm.image_id}</Link> }] : []),
        { key: 'Zonepath', value: <span className="mono">{vm.zonepath}</span> },
        ...(vm.dataset
          ? [{ key: 'Dataset', value: <span className="mono">{vm.dataset}</span> }]
          : []),
        {
          key: 'Disks',
          value: String(vm.disks.length),
          expanded: true,
          children: vm.disks.map((d) => ({
            key: `disk${String(d.index)}${d.boot ? ' (boot)' : ''}`,
            value: `${bytes(d.size_bytes)} · ${d.dataset}`,
          })),
        },
        {
          key: 'NICs',
          value: String(vm.nics.length),
          expanded: true,
          children: vm.nics.map((n) => ({
            key: n.name,
            value: `over ${n.over}${n.vid !== undefined ? ` · vid ${String(n.vid)}` : ''}${
              n.mac ? ` · ${n.mac}` : ''
            }${n.address ? ` · ${n.address}` : ''}${n.gateway ? ` via ${n.gateway}` : ''}`,
          })),
        },
        ...(vm.metadata?.description
          ? [{ key: 'Description', value: vm.metadata.description }]
          : []),
        ...(vm.metadata?.tags && vm.metadata.tags.length > 0
          ? [{ key: 'Tags', value: vm.metadata.tags.join(', ') }]
          : []),
        ...(vm.metadata?.notes ? [{ key: 'Notes', value: vm.metadata.notes }] : []),
      ]}
    />
  );
}

export function VmDetail() {
  const { id = '' } = useParams();
  const { actor } = useOutletContext<{ actor: Session['actor'] }>();
  const canWrite = actor.role === 'admin' || actor.role === 'operator';
  const vm = useVm(id);
  const action = useVmAction();
  const client = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [deleting, setDeleting] = useState(false);

  useEvents(
    useCallback(
      (event: Event) => {
        if (event.kind.startsWith('vm.') || event.object?.kind === 'job') {
          void client.invalidateQueries({ queryKey: vmKeys.all });
        }
      },
      [client],
    ),
  );

  if (vm.isPending) {
    return (
      <div className="empty">
        <Spinner />
      </div>
    );
  }
  if (vm.isError) {
    return (
      <>
        <div className="page-header">
          <h1>VM</h1>
        </div>
        <Alert status="danger">{problem(vm.error)}</Alert>
        <p>
          <Link to="/vms">Back to VMs</Link>
        </p>
      </>
    );
  }
  const v = vm.data;
  const running = v.state === 'running';
  const consoleReady = v.state !== 'configured' && v.state !== 'incomplete';

  return (
    <>
      <div className="page-header">
        <h1>
          <Link to="/vms">VMs</Link> / {v.metadata?.display_name ?? v.name}
        </h1>
        <ZoneStateLabel state={v.state} />
        <span className="spacer" />
        {canWrite && (
          <>
            {canStart(v) && (
              <Button
                variant="primary"
                icon="play"
                loading={action.isPending}
                onClick={() => {
                  action.mutate({ id: v.id, action: 'start' });
                }}
              >
                Start
              </Button>
            )}
            {running && (
              <Button
                icon="stop"
                loading={action.isPending}
                onClick={() => {
                  action.mutate({ id: v.id, action: 'stop' });
                }}
              >
                Shut down
              </Button>
            )}
            <Dropdown
              trigger=""
              variant="link-neutral"
              right
              items={[
                {
                  label: 'Restart',
                  icon: 'refresh',
                  disabled: !running,
                  onClick: () => {
                    action.mutate({ id: v.id, action: 'restart' });
                  },
                },
                {
                  label: 'Reset (hard)',
                  icon: 'power',
                  disabled: !running,
                  onClick: () => {
                    action.mutate({ id: v.id, action: 'reset' });
                  },
                },
                {
                  label: 'Power off (force)',
                  icon: 'power',
                  disabled: !running,
                  onClick: () => {
                    action.mutate({ id: v.id, action: 'stop', force: true });
                  },
                },
                {
                  label: 'Edit',
                  icon: 'pencil',
                  onClick: () => {
                    setEditing(true);
                  },
                },
                { divider: true },
                {
                  label: 'Delete',
                  icon: 'trash',
                  onClick: () => {
                    setDeleting(true);
                  },
                },
              ]}
            />
          </>
        )}
      </div>
      {action.error && (
        <Alert status="danger" closable>
          {problem(action.error)}
        </Alert>
      )}
      <Tabs
        tabs={[
          { label: 'Overview', content: <Overview vm={v} /> },
          { label: 'Disks', content: <DisksTab vm={v} canWrite={canWrite} /> },
          { label: 'Media', content: <MediaTab vm={v} canWrite={canWrite} /> },
          { label: 'Snapshots', content: <SnapshotsTab vm={v} canWrite={canWrite} /> },
          {
            label: 'Serial',
            disabled: !canWrite || !consoleReady,
            content: consoleReady ? (
              <ConsoleTerminal kind="vm" id={v.id} />
            ) : (
              <p className="field-note">
                The serial console is available once the VM is installed.
              </p>
            ),
          },
          {
            label: 'Display',
            disabled: !canWrite || !running || !v.vnc,
            content:
              running && v.vnc ? (
                <VmDisplay vmId={v.id} />
              ) : (
                <p className="field-note">
                  {v.vnc
                    ? 'The display is available while the VM runs.'
                    : 'VNC is off for this VM; turn it on under Edit.'}
                </p>
              ),
          },
        ]}
      />
      {editing && (
        <EditVmModal
          vm={v}
          onClose={() => {
            setEditing(false);
          }}
        />
      )}
      {deleting && (
        <DeleteVmModal
          vm={v}
          onClose={() => {
            setDeleting(false);
          }}
        />
      )}
    </>
  );
}
