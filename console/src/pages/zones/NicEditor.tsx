// A small editor for a zone's NIC list, used by the wizard and the edit
// dialog. Each NIC becomes a zonecfg anet resource.

import { useLinks } from '../../api/network.ts';
import type { ZoneNic } from '../../api/zones.ts';
import { Button, FormField, Input, Select } from '../../design/index.tsx';

export function NicEditor({
  nics,
  onChange,
}: {
  nics: ZoneNic[];
  onChange: (next: ZoneNic[]) => void;
}) {
  const links = useLinks();
  const bases = (links.data?.items ?? [])
    .filter((l) => l.kind === 'phys' || l.kind === 'aggr' || l.kind === 'etherstub')
    .map((l) => l.name);
  const set = (i: number, patch: Partial<ZoneNic>) => {
    onChange(nics.map((n, j) => (j === i ? { ...n, ...patch } : n)));
  };
  const text = (value: string | undefined) => value ?? '';
  return (
    <div className="vdev-groups">
      {nics.map((nic, i) => (
        <div className="vdev-group" key={String(i)}>
          <div className="form-row">
            <FormField label="Name" required>
              <Input
                value={nic.name}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  set(i, { name: e.target.value });
                }}
              />
            </FormField>
            <FormField label="Over" required>
              <Select
                value={nic.over}
                options={bases.includes(nic.over) ? bases : ['', ...bases]}
                onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                  set(i, { over: e.target.value });
                }}
              />
            </FormField>
            <FormField label="VLAN id">
              <Input
                value={nic.vid === undefined ? '' : String(nic.vid)}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  const v = e.target.value.trim();
                  const next = { ...nic };
                  if (v === '') delete next.vid;
                  else next.vid = Number(v);
                  onChange(nics.map((n, j) => (j === i ? next : n)));
                }}
              />
            </FormField>
          </div>
          <div className="form-row">
            <FormField label="Address" helper="With prefix, applied inside lx zones">
              <Input
                value={text(nic.address)}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  const v = e.target.value.trim();
                  const next = { ...nic };
                  if (v === '') delete next.address;
                  else next.address = v;
                  onChange(nics.map((n, j) => (j === i ? next : n)));
                }}
              />
            </FormField>
            <FormField label="Gateway">
              <Input
                value={text(nic.gateway)}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  const v = e.target.value.trim();
                  const next = { ...nic };
                  if (v === '') delete next.gateway;
                  else next.gateway = v;
                  onChange(nics.map((n, j) => (j === i ? next : n)));
                }}
              />
            </FormField>
            <FormField label="MAC" helper="Empty: chosen by the system">
              <Input
                value={text(nic.mac)}
                onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                  const v = e.target.value.trim();
                  const next = { ...nic };
                  if (v === '') delete next.mac;
                  else next.mac = v;
                  onChange(nics.map((n, j) => (j === i ? next : n)));
                }}
              />
            </FormField>
          </div>
          <Button
            variant="link"
            sm
            icon="trash"
            onClick={() => {
              onChange(nics.filter((_, j) => j !== i));
            }}
          >
            Remove NIC
          </Button>
        </div>
      ))}
      <div>
        <Button
          sm
          icon="plus"
          onClick={() => {
            onChange([...nics, { name: `net${String(nics.length)}`, over: bases[0] ?? '' }]);
          }}
        >
          Add NIC
        </Button>
      </div>
    </div>
  );
}
