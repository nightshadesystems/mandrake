import { useState, type SyntheticEvent } from 'react';

import {
  useAddresses,
  useCreateAddress,
  useDeleteAddress,
  useLinks,
  type Address,
  type AddressKind,
} from '../../api/network.ts';
import {
  Alert,
  Button,
  Checkbox,
  Datagrid,
  Dropdown,
  FormField,
  Input,
  Label,
  Modal,
  Select,
  Spinner,
} from '../../design/index.tsx';
import { MetadataFields, NameCell } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody, problem } from '../common/util.ts';

const KINDS: { value: AddressKind; label: string }[] = [
  { value: 'static', label: 'Static' },
  { value: 'dhcp', label: 'DHCP' },
  { value: 'addrconf', label: 'IPv6 autoconf' },
];

function validAddress(text: string): boolean {
  const [ip, prefix, ...rest] = text.split('/');
  if (rest.length > 0 || ip === undefined || prefix === undefined) return false;
  const v4 = /^(\d{1,3}\.){3}\d{1,3}$/.test(ip);
  const v6 = !v4 && ip.includes(':') && /^[0-9a-fA-F:.]+$/.test(ip);
  if (!v4 && !v6) return false;
  const p = Number(prefix);
  return /^\d+$/.test(prefix) && p <= (v4 ? 32 : 128);
}

function CreateAddressModal({
  interfaces,
  onClose,
}: {
  interfaces: string[];
  onClose: () => void;
}) {
  const create = useCreateAddress();
  const [iface, setIface] = useState(interfaces[0] ?? '');
  const [kind, setKind] = useState<AddressKind>('static');
  const [address, setAddress] = useState('');
  const [alias, setAlias] = useState('');
  const [temporary, setTemporary] = useState(false);
  const [meta, setMeta] = useState(emptyMetadata());
  const aliasOk = alias === '' || /^[a-zA-Z0-9_]{1,16}$/.test(alias);
  const addressOk = kind !== 'static' || validAddress(address);
  const valid = iface !== '' && aliasOk && addressOk;
  const submit = (e: SyntheticEvent) => {
    e.preventDefault();
    const metadata = metadataBody(meta);
    create.mutate(
      {
        interface: iface,
        kind,
        ...(kind === 'static' ? { address: address.trim() } : {}),
        ...(alias ? { alias } : {}),
        temporary,
        ...(metadata ? { metadata } : {}),
      },
      { onSuccess: onClose },
    );
  };
  return (
    <Modal
      open
      title="New address"
      onClose={onClose}
      footer={
        <>
          <Button onClick={onClose} disabled={create.isPending}>
            Cancel
          </Button>
          <Button
            variant="primary"
            form="address-form"
            type="submit"
            loading={create.isPending}
            disabled={!valid}
          >
            Add address
          </Button>
        </>
      }
    >
      <form id="address-form" className="form-stack" onSubmit={submit}>
        {create.error && (
          <Alert status="danger" sm>
            {problem(create.error)}
          </Alert>
        )}
        <div className="form-row">
          <FormField label="Interface" required helper="The IP interface is created if missing">
            <Select
              value={iface}
              options={interfaces}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setIface(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Kind" required>
            <Select
              value={kind}
              options={KINDS}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setKind(e.target.value as AddressKind);
              }}
            />
          </FormField>
        </div>
        {kind === 'static' && (
          <FormField
            label="Address"
            required
            helper="With prefix length: 192.0.2.10/24 or 2001:db8::10/64"
            {...(address && !addressOk ? { error: 'Not an address with prefix length' } : {})}
          >
            <Input
              value={address}
              autoFocus
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setAddress(e.target.value);
              }}
            />
          </FormField>
        )}
        <FormField
          label="Alias"
          helper="The part after / in the address object name; default v4 or v6"
          {...(!aliasOk ? { error: '1 to 16 letters, digits, or underscores' } : {})}
        >
          <Input
            value={alias}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setAlias(e.target.value);
            }}
          />
        </FormField>
        <Checkbox
          label="Temporary: do not persist across reboot"
          checked={temporary}
          onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
            setTemporary(e.target.checked);
          }}
        />
        <MetadataFields value={meta} onChange={setMeta} />
      </form>
    </Modal>
  );
}

function DeleteAddressModal({ address, onClose }: { address: Address; onClose: () => void }) {
  const remove = useDeleteAddress();
  return (
    <Modal
      open
      size="sm"
      title={`Remove ${address.name}?`}
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
              remove.mutate(address.id, { onSuccess: onClose });
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
        <p>
          {address.address ?? 'The address'} on {address.interface} goes away now and at the next
          boot.
        </p>
      </div>
    </Modal>
  );
}

export function Addresses({ canWrite }: { canWrite: boolean }) {
  const addresses = useAddresses();
  const links = useLinks();
  const [creating, setCreating] = useState(false);
  const [deleting, setDeleting] = useState<Address | null>(null);
  const rows = addresses.data?.items ?? [];
  const interfaces = [...(links.data?.items.map((l) => l.name) ?? []), 'lo0'];

  return (
    <>
      <div className="toolbar">
        <span className="spacer" />
        {canWrite && (
          <Button
            variant="primary"
            icon="plus-circle"
            onClick={() => {
              setCreating(true);
            }}
          >
            New address
          </Button>
        )}
      </div>
      {addresses.isError && (
        <Alert status="danger" closable>
          {problem(addresses.error)}
        </Alert>
      )}
      {addresses.isPending ? (
        <div className="empty">
          <Spinner />
        </div>
      ) : (
        <Datagrid<Address>
          rows={rows}
          placeholder="No addresses."
          footerText={`${String(rows.length)} addresses`}
          columns={[
            {
              key: 'name',
              label: 'Address object',
              sortable: true,
              render: (a) => (
                <span className="name-cell">
                  <NameCell name={a.name} metadata={a.metadata} />
                  {a.protected && <Label>MANAGEMENT</Label>}
                </span>
              ),
            },
            {
              key: 'address',
              label: 'Address',
              sortable: true,
              render: (a) => <span className="cell-mono">{a.address ?? '(pending)'}</span>,
            },
            {
              key: 'interface',
              label: 'Interface',
              sortable: true,
              render: (a) => <span className="cell-mono">{a.interface}</span>,
            },
            { key: 'kind', label: 'Kind', sortable: true },
            { key: 'family', label: 'Family' },
            {
              key: 'state',
              label: 'State',
              render: (a) =>
                a.state === 'ok' ? (
                  <Label status="success">OK</Label>
                ) : (
                  <Label status="warning">{a.state.toUpperCase()}</Label>
                ),
            },
            {
              key: 'persistent',
              label: 'Persistent',
              render: (a) => (a.persistent ? 'yes' : 'no'),
            },
            {
              key: 'actions',
              label: '',
              width: 48,
              render: (a) =>
                canWrite ? (
                  <Dropdown
                    trigger=""
                    variant="link-neutral"
                    sm
                    right
                    items={[
                      {
                        label: 'Remove',
                        icon: 'trash',
                        disabled: a.protected,
                        onClick: () => {
                          setDeleting(a);
                        },
                      },
                    ]}
                  />
                ) : null,
            },
          ]}
        />
      )}
      {creating && (
        <CreateAddressModal
          interfaces={interfaces}
          onClose={() => {
            setCreating(false);
          }}
        />
      )}
      {deleting && (
        <DeleteAddressModal
          address={deleting}
          onClose={() => {
            setDeleting(null);
          }}
        />
      )}
    </>
  );
}
