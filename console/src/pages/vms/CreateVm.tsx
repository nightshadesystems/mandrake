// The VM create wizard: boot source, sizing, disks, networking, review.

import { useState } from 'react';

import { ApiError } from '../../api/client.ts';
import { useImages, type Image } from '../../api/images.ts';
import { usePools } from '../../api/storage.ts';
import { useCreateVm, type Bootrom, type VmDiskSpec } from '../../api/vms.ts';
import type { ZoneNic } from '../../api/zones.ts';
import {
  Alert,
  Button,
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
import { NicEditor } from '../zones/NicEditor.tsx';
import { nicErrors } from '../zones/util.ts';
import { BOOTROMS, VM_NAME, diskSize, sizingErrors } from './util.ts';

type Source = 'image' | 'iso';

function imageLabel(i: Image): string {
  return `${i.name}@${i.version}${i.os ? ` (${i.os})` : ''}`;
}

export function CreateVm({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated: (vmId: string) => void;
}) {
  const create = useCreateVm();
  const images = useImages();
  const pools = usePools();
  const [name, setName] = useState('');
  const [source, setSource] = useState<Source>('image');
  const [imageId, setImageId] = useState('');
  const [isoId, setIsoId] = useState('');
  const [bootSize, setBootSize] = useState('20G');
  const [pool, setPool] = useState('');
  const [vcpus, setVcpus] = useState('2');
  const [memory, setMemory] = useState('2G');
  const [bootrom, setBootrom] = useState<Bootrom>('uefi');
  const [acpi, setAcpi] = useState(true);
  const [vnc, setVnc] = useState(true);
  const [autoboot, setAutoboot] = useState(true);
  const [start, setStart] = useState(true);
  const [extraDisks, setExtraDisks] = useState<string[]>([]);
  const [extraIsos, setExtraIsos] = useState<string[]>([]);
  const [nics, setNics] = useState<ZoneNic[]>([]);
  const [meta, setMeta] = useState(emptyMetadata());
  const [submitError, setSubmitError] = useState<string | null>(null);

  const ready = (images.data?.items ?? []).filter((i) => i.state === 'ready');
  const vmImages = ready.filter((i) => i.type === 'vm-raw');
  const isos = ready.filter((i) => i.type === 'vm-iso');
  const image = source === 'image' ? vmImages.find((i) => i.id === imageId) : undefined;
  const iso = source === 'iso' ? isos.find((i) => i.id === isoId) : undefined;
  const bootBytes = diskSize(bootSize);
  const memoryBytes = diskSize(memory);
  const extraBytes = extraDisks.map(diskSize);
  const cdromIds = [...(iso ? [iso.id] : []), ...extraIsos.filter((id) => id !== '')];

  const errors: string[] = [];
  if (!VM_NAME.test(name)) errors.push('Name: letters, digits, _ . -');
  if (source === 'image' && !image) errors.push('Choose a VM image to clone');
  if (source === 'iso' && !iso) errors.push('Choose an ISO to install from');
  if (source === 'iso' && bootBytes === undefined)
    errors.push('Boot disk size must be at least 1M');
  errors.push(...sizingErrors(vcpus, memory));
  extraBytes.forEach((b, i) => {
    if (b === undefined) errors.push(`Disk ${String(i + 1)}: size must be at least 1M`);
  });
  if (new Set(cdromIds).size !== cdromIds.length) errors.push('An ISO is attached twice');
  errors.push(...nicErrors(nics));

  const finish = () => {
    if (errors.length > 0 || memoryBytes === undefined) {
      setSubmitError(errors[0] ?? 'Fix the highlighted fields first.');
      return;
    }
    const boot: VmDiskSpec = image
      ? { image_id: image.id, boot: true }
      : { size_bytes: bootBytes ?? 0, boot: true };
    const disks: VmDiskSpec[] = [
      boot,
      ...extraBytes.map((b) => ({ size_bytes: b ?? 0, boot: false })),
    ];
    const metadata = metadataBody(meta);
    create.mutate(
      {
        name,
        vcpus: Number(vcpus),
        memory_bytes: memoryBytes,
        bootrom,
        acpi,
        ...(pool && !image ? { pool } : {}),
        disks,
        cdroms: cdromIds,
        nics,
        vnc,
        autoboot,
        start,
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

  const sourceStep = (
    <div className="form-stack">
      <div className="form-row">
        <FormField
          label="Name"
          required
          helper="Letters, digits, _ . -"
          {...(name && !VM_NAME.test(name) ? { error: 'Not a valid VM name' } : {})}
        >
          <Input
            value={name}
            autoFocus
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setName(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Boot from" required>
          <Select
            value={source}
            options={[
              { value: 'image', label: 'A VM image (clone the boot disk)' },
              { value: 'iso', label: 'An ISO (install onto a blank disk)' },
            ]}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setSource(e.target.value as Source);
            }}
          />
        </FormField>
      </div>
      {source === 'image' ? (
        <FormField label="Image" required helper="A ready vm-raw image; the boot disk is a clone">
          <Select
            value={imageId}
            options={[
              { value: '', label: 'Choose an image' },
              ...vmImages.map((i) => ({ value: i.id, label: imageLabel(i) })),
            ]}
            onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
              setImageId(e.target.value);
            }}
          />
        </FormField>
      ) : (
        <div className="form-row">
          <FormField
            label="ISO"
            required
            helper="A ready vm-iso image, attached as the first cdrom"
          >
            <Select
              value={isoId}
              options={[
                { value: '', label: 'Choose an ISO' },
                ...isos.map((i) => ({ value: i.id, label: imageLabel(i) })),
              ]}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setIsoId(e.target.value);
              }}
            />
          </FormField>
          <FormField label="Boot disk size" required helper="e.g. 20G">
            <Input
              value={bootSize}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setBootSize(e.target.value);
              }}
            />
          </FormField>
        </div>
      )}
      {source === 'image' && vmImages.length === 0 && (
        <Alert status="warning" sm>
          No ready VM image. Import one under Images first.
        </Alert>
      )}
      {source === 'iso' && isos.length === 0 && (
        <Alert status="warning" sm>
          No ready ISO. Import one under Images first.
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
        <FormField label="vCPUs" required helper="1 to 128">
          <Input
            value={vcpus}
            onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
              setVcpus(e.target.value);
            }}
          />
        </FormField>
        <FormField label="Memory" required helper="e.g. 2G; at least 128M">
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
        label="VNC display (reachable only through the console)"
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
      <Checkbox
        label="Boot as soon as it is created"
        checked={start}
        onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
          setStart(e.target.checked);
        }}
      />
      <MetadataFields value={meta} onChange={setMeta} />
    </div>
  );

  const disksStep = (
    <div className="form-stack">
      <p className="field-note">
        Boot disk:{' '}
        {image
          ? `clone of ${imageLabel(image)}`
          : `blank, ${bootBytes === undefined ? '?' : bytes(bootBytes)}`}
        . Extra disks are blank zvols.
      </p>
      {extraDisks.map((size, i) => (
        <div className="form-row" key={String(i)}>
          <FormField label={`Disk ${String(i + 1)} size`} helper="e.g. 100G">
            <Input
              value={size}
              onChange={(e: React.ChangeEvent<HTMLInputElement>) => {
                setExtraDisks(extraDisks.map((s, j) => (j === i ? e.target.value : s)));
              }}
            />
          </FormField>
          <Button
            variant="link-neutral"
            icon="trash"
            onClick={() => {
              setExtraDisks(extraDisks.filter((_, j) => j !== i));
            }}
          >
            Remove
          </Button>
        </div>
      ))}
      <div>
        <Button
          variant="link"
          icon="plus"
          onClick={() => {
            setExtraDisks([...extraDisks, '50G']);
          }}
        >
          Add a disk
        </Button>
      </div>
      {extraIsos.map((id, i) => (
        <div className="form-row" key={String(i)}>
          <FormField label={`Extra ISO ${String(i + 1)}`}>
            <Select
              value={id}
              options={[
                { value: '', label: 'Choose an ISO' },
                ...isos.map((x) => ({ value: x.id, label: imageLabel(x) })),
              ]}
              onChange={(e: React.ChangeEvent<HTMLSelectElement>) => {
                setExtraIsos(extraIsos.map((s, j) => (j === i ? e.target.value : s)));
              }}
            />
          </FormField>
          <Button
            variant="link-neutral"
            icon="trash"
            onClick={() => {
              setExtraIsos(extraIsos.filter((_, j) => j !== i));
            }}
          >
            Remove
          </Button>
        </div>
      ))}
      <div>
        <Button
          variant="link"
          icon="plus"
          disabled={isos.length === 0}
          onClick={() => {
            setExtraIsos([...extraIsos, '']);
          }}
        >
          Attach another ISO
        </Button>
      </div>
    </div>
  );

  const networkStep = (
    <div className="form-stack">
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
          {
            key: 'Boot disk',
            value: image
              ? `clone of ${imageLabel(image)}`
              : `blank ${bootBytes === undefined ? '?' : bytes(bootBytes)}${iso ? `, install from ${imageLabel(iso)}` : ''}`,
          },
          { key: 'Pool', value: image ? (image.pool ?? 'default') : pool || 'default' },
          { key: 'vCPUs', value: vcpus },
          { key: 'Memory', value: memoryBytes === undefined ? '?' : bytes(memoryBytes) },
          { key: 'Firmware', value: BOOTROMS.find((b) => b.value === bootrom)?.label ?? bootrom },
          { key: 'ACPI', value: acpi ? 'yes' : 'no' },
          { key: 'VNC', value: vnc ? 'on' : 'off' },
          { key: 'Autoboot', value: autoboot ? 'yes' : 'no' },
          { key: 'Boot after create', value: start ? 'yes' : 'no' },
          {
            key: 'Extra disks',
            value: extraBytes.map((b) => (b === undefined ? '?' : bytes(b))).join(', ') || 'none',
          },
          {
            key: 'ISOs',
            value:
              cdromIds
                .map((id) => isos.find((x) => x.id === id))
                .map((x) => (x ? imageLabel(x) : '?'))
                .join(', ') || 'none',
          },
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
      title="New VM"
      onClose={onClose}
      onFinish={finish}
      steps={[
        { title: 'Boot source', content: sourceStep },
        { title: 'Sizing', content: sizingStep },
        { title: 'Disks and media', content: disksStep },
        { title: 'Networking', content: networkStep },
        { title: 'Review', content: reviewStep },
      ]}
    />
  );
}
