// The zone create wizard: image, sizing, networking, review.

import { useState } from 'react';

import { ApiError } from '../../api/client.ts';
import { useImages } from '../../api/images.ts';
import { usePools } from '../../api/storage.ts';
import { useCreateZone, type ZoneBrand, type ZoneNic } from '../../api/zones.ts';
import {
  Alert,
  Checkbox,
  FormField,
  Input,
  Select,
  StackView,
  Wizard,
} from '../../design/index.tsx';
import { bytes } from '../../fmt.ts';
import { MetadataFields } from '../common/Metadata.tsx';
import { emptyMetadata, metadataBody } from '../common/util.ts';
import { parseSize } from '../storage/util.ts';
import { NicEditor } from './NicEditor.tsx';
import { nicErrors } from './util.ts';

const BRANDS: { value: ZoneBrand; label: string }[] = [
  { value: 'lx', label: 'lx (Linux)' },
  { value: 'ipkg', label: 'ipkg (native, own packages)' },
  { value: 'lipkg', label: 'lipkg (native, linked to the host)' },
  { value: 'sparse', label: 'sparse (native, shares /usr)' },
];

const ZONE_NAME = /^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,62}$/;

export function CreateZone({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (zoneId: string) => void;
}) {
  const create = useCreateZone();
  const images = useImages();
  const pools = usePools();
  const [name, setName] = useState('');
  const [brand, setBrand] = useState<ZoneBrand>('lx');
  const [imageId, setImageId] = useState('');
  const [pool, setPool] = useState('');
  const [cpuCap, setCpuCap] = useState('');
  const [memory, setMemory] = useState('');
  const [autoboot, setAutoboot] = useState(true);
  const [start, setStart] = useState(true);
  const [nics, setNics] = useState<ZoneNic[]>([]);
  const [hostname, setHostname] = useState('');
  const [resolvers, setResolvers] = useState('');
  const [meta, setMeta] = useState(emptyMetadata());
  const [submitError, setSubmitError] = useState<string | null>(null);

  const wanted = brand === 'lx' ? 'zone-lx' : 'zone-native';
  const candidates = (images.data?.items ?? []).filter(
    (i) => i.state === 'ready' && i.type === wanted,
  );
  const image = candidates.find((i) => i.id === imageId);
  const memoryBytes = memory.trim() ? parseSize(memory) : undefined;
  const resolverList = resolvers
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter((s) => s !== '');

  const errors: string[] = [];
  if (!ZONE_NAME.test(name)) errors.push('Name: letters, digits, _ . - and not "global"');
  if (name === 'global') errors.push('"global" is the host itself');
  if (brand === 'lx' && !image) errors.push('An lx zone needs an image');
  if (cpuCap.trim() && !(Number(cpuCap) > 0)) errors.push('CPU cap must be a positive number');
  if (memory.trim() && (memoryBytes === undefined || memoryBytes < 64 * 1024 * 1024)) {
    errors.push('Memory cap must be a size of at least 64M');
  }
  errors.push(...nicErrors(nics));
  resolverList.forEach((r) => {
    if (!/^[0-9a-fA-F:.]+$/.test(r)) errors.push(`Resolver "${r}" is not an IP address`);
  });

  const finish = () => {
    if (errors.length > 0) {
      setSubmitError(errors[0] ?? 'Fix the highlighted fields first.');
      return;
    }
    const metadata = metadataBody(meta);
    create.mutate(
      {
        name,
        brand,
        ...(image ? { image_id: image.id } : {}),
        ...(pool ? { pool } : {}),
        nics,
        ...(cpuCap.trim() ? { cpu_cap: Number(cpuCap) } : {}),
        ...(memoryBytes !== undefined ? { memory_cap_bytes: memoryBytes } : {}),
        autoboot,
        start,
        ...(hostname.trim() ? { hostname: hostname.trim() } : {}),
        resolvers: resolverList,
        ...(metadata ? { metadata } : {}),
      },
      {
        onSuccess: (job) => {
          const id = job.target?.id;
          if (id) onCreated(id);
          else onClose();
        },
        onError: (e) => {
          setSubmitError(e instanceof ApiError ? e.message : 'Request failed.');
        },
      },
    );
  };

  const imageStep = (
    <div className="form-stack">
      <div className="form-row">
        <FormField
          label="Name"
          required
          helper="Letters, digits, _ . -"
          {...(name && !ZONE_NAME.test(name) ? { error: 'Not a valid zone name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Brand" required>
          <Select
            value={brand}
            options={BRANDS}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setBrand(e.target.value as ZoneBrand);
              setImageId('');
            }}
          />
        </FormField>
      </div>
      <FormField
        label="Image"
        {...(brand === 'lx' ? { required: true } : {})}
        helper={
          brand === 'lx'
            ? 'A ready lx image; the zone is a clone of it'
            : 'Optional: a ready native image, else the zone installs from the host packages'
        }
      >
        <Select
          value={imageId}
          options={[
            { value: '', label: brand === 'lx' ? 'Choose an image' : 'Install from packages' },
            ...candidates.map((i) => ({
              value: i.id,
              label: `${i.name}@${i.version}${i.os ? ` (${i.os})` : ''}`,
            })),
          ]}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
            setImageId(e.target.value);
          }}
        />
      </FormField>
      {brand === 'lx' && candidates.length === 0 && (
        <Alert status="warning" sm>
          No ready lx image. Import one under Images first.
        </Alert>
      )}
      <FormField
        label="Pool"
        helper={
          image
            ? `The image lives in ${image.pool ?? 'its pool'}; the clone stays there`
            : 'Empty: the data pool with the most free space'
        }
      >
        <Select
          value={pool}
          disabled={Boolean(image)}
          options={[
            { value: '', label: 'Default' },
            ...(pools.data?.items.map((p) => p.name) ?? []),
          ]}
          onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
            setPool(e.target.value);
          }}
        />
      </FormField>
    </div>
  );

  const sizingStep = (
    <div className="form-stack">
      <div className="form-row">
        <FormField label="CPU cap" helper="CPUs worth of time, e.g. 1.5; empty for none">
          <Input
            value={cpuCap}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setCpuCap(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Memory cap" helper="e.g. 2G; empty for none">
          <Input
            value={memory}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setMemory(e.target.value);
            }}
          />
        </FormField>
      </div>
      <Checkbox
        label="Boot with the host (autoboot)"
        checked={autoboot}
        onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
          setAutoboot(e.target.checked);
        }}
      />
      <Checkbox
        label="Boot as soon as it is installed"
        checked={start}
        onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
          setStart(e.target.checked);
        }}
      />
      <MetadataFields value={meta} onChange={setMeta} />
    </div>
  );

  const networkStep = (
    <div className="form-stack">
      <div className="form-row">
        <FormField label="Hostname" helper="Default: the zone name">
          <Input
            value={hostname}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setHostname(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Resolvers" helper="Comma or space separated">
          <Input
            value={resolvers}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setResolvers(e.target.value);
            }}
          />
        </FormField>
      </div>
      <NicEditor nics={nics} onChange={setNics} />
    </div>
  );

  const reviewStep = (
    <div className="form-stack">
      {errors.length > 0 && (
        <Alert
          status="warning"
          items={errors.map((e) => (
            <span key={e}>{e}</span>
          ))}
        />
      )}
      {submitError && (
        <Alert status="danger" sm>
          {submitError}
        </Alert>
      )}
      <StackView
        blocks={[
          { key: 'Name', value: name || '-' },
          { key: 'Brand', value: brand },
          {
            key: 'Image',
            value: image ? `${image.name}@${image.version}` : 'from packages',
          },
          { key: 'Pool', value: image ? (image.pool ?? 'default') : pool || 'default' },
          { key: 'CPU cap', value: cpuCap.trim() || 'none' },
          {
            key: 'Memory cap',
            value: memoryBytes === undefined ? 'none' : bytes(memoryBytes),
          },
          { key: 'Autoboot', value: autoboot ? 'yes' : 'no' },
          { key: 'Boot after install', value: start ? 'yes' : 'no' },
          { key: 'Hostname', value: hostname.trim() || name || '-' },
          { key: 'Resolvers', value: resolverList.join(', ') || 'none' },
          {
            key: 'NICs',
            value: String(nics.length),
            expanded: true,
            children: nics.map((n) => ({
              key: n.name,
              value: `over ${n.over}${n.vid !== undefined ? ` vid ${String(n.vid)}` : ''}${
                n.address ? ` ${n.address}` : ''
              }${n.gateway ? ` via ${n.gateway}` : ''}`,
            })),
          },
        ]}
      />
      {create.isPending && <p className="field-note">Creating…</p>}
    </div>
  );

  return (
    <Wizard
      open
      title="New zone"
      onClose={onClose}
      onFinish={finish}
      steps={[
        { title: 'Image', content: imageStep },
        { title: 'Sizing', content: sizingStep },
        { title: 'Networking', content: networkStep },
        { title: 'Review', content: reviewStep },
      ]}
    />
  );
}
