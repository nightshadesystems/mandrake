import { useDevices, type Device } from '../../api/storage.ts';
import { Alert, Datagrid, Label, Spinner } from '../../design/index.tsx';
import { bytes } from '../../fmt.ts';
import { problem } from './util.ts';

export function Devices() {
  const devices = useDevices();
  const rows = devices.data?.items ?? [];
  if (devices.isPending) {
    return (
      <div className="empty">
        <Spinner />
      </div>
    );
  }
  return (
    <>
      {devices.isError && (
        <Alert status="danger" closable>
          {problem(devices.error)}
        </Alert>
      )}
      <Datagrid<Device>
        rows={rows}
        placeholder="No disks found."
        footerText={`${String(rows.length)} disks · ${String(rows.filter((d) => !d.pool).length)} free`}
        columns={[
          {
            key: 'name',
            label: 'Device',
            sortable: true,
            render: (d) => <span className="cell-mono">{d.name}</span>,
          },
          {
            key: 'size_bytes',
            label: 'Size',
            sortable: true,
            render: (d) => <span className="cell-mono">{bytes(d.size_bytes)}</span>,
          },
          {
            key: 'product',
            label: 'Model',
            render: (d) => [d.vendor, d.product].filter(Boolean).join(' ') || '-',
          },
          {
            key: 'serial',
            label: 'Serial',
            render: (d) => <span className="cell-mono">{d.serial ?? '-'}</span>,
          },
          {
            key: 'kind',
            label: 'Kind',
            render: (d) => (
              <span className="name-cell">
                {d.solid_state === true && <Label>SSD</Label>}
                {d.solid_state === false && <Label>HDD</Label>}
                {d.removable && <Label status="warning">REMOVABLE</Label>}
              </span>
            ),
          },
          {
            key: 'pool',
            label: 'Pool',
            sortable: true,
            render: (d) =>
              d.pool ? (
                <span className="cell-mono">{d.pool}</span>
              ) : (
                <Label status="success">FREE</Label>
              ),
          },
        ]}
      />
    </>
  );
}
