// Create, edit, and delete dialogs for links; used by the topology view
// and the links table alike.

import { useState, type SyntheticEvent } from 'react';

import {
  useCreateAggr,
  useCreateEtherstub,
  useCreateVlan,
  useCreateVnic,
  useDeleteLink,
  useUpdateLink,
  type Link,
} from '../../api/network.ts';
import { Alert, Button, Checkbox, FormField, Input, Modal, Select } from '../../design/index.tsx';
import { MetadataFields } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody, problem } from '../common/util.ts';

import { KIND_LABEL, type CreatableKind } from './util.ts';

export type { CreatableKind };

const LINK_NAME = /^[a-zA-Z][a-zA-Z0-9_]{0,30}[0-9]$/;
const MAC = /^([0-9a-fA-F]{1,2}:){5}[0-9a-fA-F]{1,2}$/;
const POLICIES = ['L4', 'L2', 'L3', 'L2,L3', 'L2,L3,L4'];

function nameError(name: string): string | undefined {
  if (name === '' || LINK_NAME.test(name)) return undefined;
  return 'Letters, digits, and underscores, ending in a digit';
}

/** A free name for a new link of `kind` over `over`. */
function suggestName(kind: CreatableKind, links: Link[], over?: string): string {
  const prefix = kind === 'etherstub' ? 'stub' : kind;
  const taken = new Set(links.map((l) => l.name));
  for (let i = 0; i < 1000; i += 1) {
    const candidate = kind === 'vlan' && over ? `${prefix}${String(i)}` : `${prefix}${String(i)}`;
    if (!taken.has(candidate)) return candidate;
  }
  return '';
}

/** Links a new link of `kind` may sit on. */
function bases(kind: CreatableKind, links: Link[]): Link[] {
  return links.filter((l) => {
    if (kind === 'vlan') return l.kind === 'phys' || l.kind === 'aggr';
    if (kind === 'vnic') return l.kind === 'phys' || l.kind === 'aggr' || l.kind === 'etherstub';
    return false;
  });
}

function Footer({
  onClose,
  pending,
  form,
  onConfirm,
  valid,
  label,
  danger,
}: {
  onClose: () => void;
  pending: boolean;
  /** Submit this form, or ... */
  form?: string;
  /** ... run this on click. */
  onConfirm?: () => void;
  valid: boolean;
  label: string;
  danger?: boolean;
}) {
  return (
    <>
      <Button onClick={onClose} disabled={pending}>
        Cancel
      </Button>
      <Button
        variant={danger ? 'danger' : 'primary'}
        {...(form ? { form, type: 'submit' as const } : {})}
        {...(onConfirm ? { onClick: onConfirm } : {})}
        loading={pending}
        disabled={!valid}
      >
        {label}
      </Button>
    </>
  );
}

export interface CreateProps {
  links: Link[];
  /** Preselected underlying link (VLAN, VNIC) or ports (aggr). */
  over?: string;
  onClose: () => void;
}

export function CreateAggrModal({ links, over, onClose }: CreateProps) {
  const create = useCreateAggr();
  const [name, setName] = useState(() => suggestName('aggr', links));
  const [ports, setPorts] = useState<string[]>(over ? [over] : []);
  const [policy, setPolicy] = useState('L4');
  const [lacp, setLacp] = useState<'active' | 'passive' | 'off'>('active');
  const [timer, setTimer] = useState<'short' | 'long'>('short');
  const [meta, setMeta] = useState(emptyMetadata());
  const candidates = links.filter(
    (l) => l.kind === 'phys' && !links.some((o) => (o.over ?? []).includes(l.name)),
  );
  const valid = name !== '' && nameError(name) === undefined && ports.length > 0;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      {
        name,
        ports,
        policy,
        lacp_mode: lacp,
        lacp_timer: timer,
        ...(metadata ? { metadata } : {}),
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="New aggregation"
      onClose={onClose}
      footer={
        <Footer
          onClose={onClose}
          pending={create.isPending}
          form="aggr-form"
          valid={valid}
          label="Create aggregation"
        />
      }
    >
      <form id="aggr-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <FormField
          label="Name"
          required
          {...(nameError(name) !== undefined ? { error: nameError(name) ?? '' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Ports" required helper="Physical links not already in use">
          <div className="device-picker">
            {candidates.map((l) => (
              <Checkbox
                key={l.name}
                label={`${l.name}${l.protected ? ' (management)' : ''}${l.state === 'down' ? ' (down)' : ''}`}
                checked={ports.includes(l.name)}
                disabled={l.protected}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  setPorts(
                    e.target.checked ? [...ports, l.name] : ports.filter((p) => p !== l.name),
                  );
                }}
              />
            ))}
            {candidates.length === 0 && <p className="field-note">No free physical links.</p>}
          </div>
        </FormField>
        <div className="form-row">
          <FormField label="Policy">
            <Select
              value={policy}
              options={POLICIES}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setPolicy(e.target.value);
              }}
            />
          </FormField>
          <FormField label="LACP">
            <Select
              value={lacp}
              options={['active', 'passive', 'off']}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setLacp(e.target.value as 'active' | 'passive' | 'off');
              }}
            />
          </FormField>
          <FormField label="LACP timer">
            <Select
              value={timer}
              options={['short', 'long']}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setTimer(e.target.value as 'short' | 'long');
              }}
            />
          </FormField>
        </div>
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

export function CreateVlanModal({ links, over, onClose }: CreateProps) {
  const create = useCreateVlan();
  const options = bases('vlan', links).map((l) => l.name);
  const [base, setBase] = useState(over ?? options[0] ?? '');
  const [vid, setVid] = useState('');
  const [name, setName] = useState(() => suggestName('vlan', links, over));
  const [meta, setMeta] = useState(emptyMetadata());
  const vidNumber = Number(vid);
  const vidOk = /^\d+$/.test(vid) && vidNumber >= 1 && vidNumber <= 4094;
  const valid = name !== '' && nameError(name) === undefined && base !== '' && vidOk;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      { name, vid: vidNumber, over: base, ...(metadata ? { metadata } : {}) },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="New VLAN"
      onClose={onClose}
      footer={
        <Footer
          onClose={onClose}
          pending={create.isPending}
          form="vlan-form"
          valid={valid}
          label="Create VLAN"
        />
      }
    >
      <form id="vlan-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <div className="form-row">
          <FormField label="Over" required helper="A physical link or aggregation">
            <Select
              value={base}
              options={options}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setBase(e.target.value);
              }}
            />
          </FormField>
          <FormField label="VLAN id" required {...(vid && !vidOk ? { error: '1 to 4094' } : {})}>
            <Input
              value={vid}
              autoFocus
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setVid(e.target.value);
              }}
            />
          </FormField>
        </div>
        <FormField
          label="Name"
          required
          {...(nameError(name) !== undefined ? { error: nameError(name) ?? '' } : {})}
        >
          <Input
            value={name}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

export function CreateEtherstubModal({ links, onClose }: CreateProps) {
  const create = useCreateEtherstub();
  const [name, setName] = useState(() => suggestName('etherstub', links));
  const [meta, setMeta] = useState(emptyMetadata());
  const valid = name !== '' && nameError(name) === undefined;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate({ name, ...(metadata ? { metadata } : {}) }, { onSuccess: onClose });
  };
  return (
    <Modal
      open
      title="New etherstub"
      onClose={onClose}
      footer={
        <Footer
          onClose={onClose}
          pending={create.isPending}
          form="stub-form"
          valid={valid}
          label="Create etherstub"
        />
      }
    >
      <form id="stub-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <p className="field-note">
          An etherstub is a virtual switch with no uplink. VNICs on it talk to each other only.
        </p>
        <FormField
          label="Name"
          required
          {...(nameError(name) !== undefined ? { error: nameError(name) ?? '' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

export function CreateVnicModal({ links, over, onClose }: CreateProps) {
  const create = useCreateVnic();
  const options = bases('vnic', links).map((l) => l.name);
  const [base, setBase] = useState(over ?? options[0] ?? '');
  const [name, setName] = useState(() => suggestName('vnic', links));
  const [mac, setMac] = useState('');
  const [vid, setVid] = useState('');
  const [mtu, setMtu] = useState('');
  const [meta, setMeta] = useState(emptyMetadata());
  const macOk = mac === '' || MAC.test(mac);
  const vidNumber = Number(vid);
  const vidOk = vid === '' || (/^\d+$/.test(vid) && vidNumber >= 1 && vidNumber <= 4094);
  const mtuNumber = Number(mtu);
  const mtuOk = mtu === '' || (/^\d+$/.test(mtu) && mtuNumber >= 576 && mtuNumber <= 9216);
  const valid =
    name !== '' && nameError(name) === undefined && base !== '' && macOk && vidOk && mtuOk;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      {
        name,
        over: base,
        ...(mac ? { mac } : {}),
        ...(vid ? { vid: vidNumber } : {}),
        ...(mtu ? { mtu: mtuNumber } : {}),
        ...(metadata ? { metadata } : {}),
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="New VNIC"
      onClose={onClose}
      footer={
        <Footer
          onClose={onClose}
          pending={create.isPending}
          form="vnic-form"
          valid={valid}
          label="Create VNIC"
        />
      }
    >
      <form id="vnic-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <div className="form-row">
          <FormField label="Over" required helper="Physical link, aggregation, or etherstub">
            <Select
              value={base}
              options={options}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setBase(e.target.value);
              }}
            />
          </FormField>
          <FormField
            label="Name"
            required
            {...(nameError(name) !== undefined ? { error: nameError(name) ?? '' } : {})}
          >
            <Input
              value={name}
              autoFocus
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setName(e.target.value);
              }}
            />
          </FormField>
        </div>
        <div className="form-row">
          <FormField
            label="MAC address"
            helper="Empty: chosen by the system"
            {...(!macOk ? { error: 'Six colon-separated hex bytes' } : {})}
          >
            <Input
              value={mac}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setMac(e.target.value);
              }}
            />
          </FormField>
          <FormField label="VLAN tag" {...(!vidOk ? { error: '1 to 4094' } : {})}>
            <Input
              value={vid}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setVid(e.target.value);
              }}
            />
          </FormField>
          <FormField label="MTU" {...(!mtuOk ? { error: '576 to 9216' } : {})}>
            <Input
              value={mtu}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setMtu(e.target.value);
              }}
            />
          </FormField>
        </div>
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

export function CreateLinkModal({
  kind,
  ...props
}: CreateProps & {
  kind: CreatableKind;
}) {
  switch (kind) {
    case 'aggr':
      return <CreateAggrModal {...props} />;
    case 'vlan':
      return <CreateVlanModal {...props} />;
    case 'etherstub':
      return <CreateEtherstubModal {...props} />;
    case 'vnic':
      return <CreateVnicModal {...props} />;
  }
}

export function EditLinkModal({ link, onClose }: { link: Link; onClose: () => void }) {
  const update = useUpdateLink();
  const [mtu, setMtu] = useState(link.mtu === undefined ? '' : String(link.mtu));
  const [meta, setMeta] = useState(emptyMetadata(link.metadata));
  const mtuNumber = Number(mtu);
  const mtuOk = mtu === '' || (/^\d+$/.test(mtu) && mtuNumber >= 576 && mtuNumber <= 9216);
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    update.mutate(
      {
        id: link.id,
        body: {
          ...(mtu && mtuNumber !== link.mtu ? { mtu: mtuNumber } : {}),
          ...(metadata ? { metadata } : {}),
        },
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title={`Edit ${link.name}`}
      onClose={onClose}
      footer={
        <Footer
          onClose={onClose}
          pending={update.isPending}
          form="link-edit"
          valid={mtuOk}
          label="Save"
        />
      }
    >
      <form id="link-edit" className="form-stack" onSubmit={submit}>
        {update.error && (
          <Alert status="danger" sm>
            {problem(update.error)}
          </Alert>
        )}
        <FormField
          label="MTU"
          helper="576 to 9216; applies with dladm set-linkprop"
          {...(!mtuOk ? { error: '576 to 9216' } : {})}
        >
          <Input
            value={mtu}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setMtu(e.target.value);
            }}
          />
        </FormField>
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

export function DeleteLinkModal({
  link,
  links,
  onClose,
}: {
  link: Link;
  links: Link[];
  onClose: () => void;
}) {
  const remove = useDeleteLink();
  const dependents = links.filter((l) => (l.over ?? []).includes(link.name)).map((l) => l.name);
  return (
    <Modal
      open
      size="sm"
      title={`Delete ${link.name}?`}
      onClose={onClose}
      footer={
        <Footer
          onClose={onClose}
          pending={remove.isPending}
          valid={dependents.length === 0}
          label="Delete"
          danger
          onConfirm={() => {
            remove.mutate({ kind: link.kind, id: link.id }, { onSuccess: onClose });
          }}
        />
      }
    >
      <div className="form-stack">
        {remove.error && (
          <Alert status="danger" sm>
            {problem(remove.error)}
          </Alert>
        )}
        {dependents.length > 0 ? (
          <p>Refused: {dependents.join(', ')} sit on it. Delete them first.</p>
        ) : (
          <p>
            The {KIND_LABEL[link.kind].toLowerCase()} and any IP interface on it are removed
            immediately.
          </p>
        )}
      </div>
    </Modal>
  );
}
