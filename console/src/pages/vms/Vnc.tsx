// The VM display: noVNC over the daemon's VNC relay WebSocket.

import { useEffect, useRef, useState } from 'react';
import RFB from '@novnc/novnc';

import { vncUrl } from '../../api/vms.ts';
import { Button, Label } from '../../design/index.tsx';

type Status = 'connecting' | 'connected' | 'closed' | 'refused';

export function VmDisplay({ vmId }: { vmId: string }) {
  const host = useRef<HTMLDivElement | null>(null);
  const rfb = useRef<RFB | null>(null);
  const [status, setStatus] = useState<Status>('connecting');
  const [name, setName] = useState('');
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    const element = host.current;
    if (!element) return undefined;
    setStatus('connecting');
    setName('');
    const client = new RFB(element, vncUrl(vmId), { shared: true });
    client.scaleViewport = true;
    client.resizeSession = false;
    client.showDotCursor = true;
    client.background = '#0b0d12';
    let everConnected = false;
    client.addEventListener('connect', () => {
      everConnected = true;
      setStatus('connected');
      client.focus();
    });
    client.addEventListener('disconnect', (event) => {
      setStatus(everConnected && event.detail.clean ? 'closed' : 'refused');
    });
    client.addEventListener('securityfailure', () => {
      setStatus('refused');
    });
    client.addEventListener('desktopname', (event) => {
      setName(event.detail.name);
    });
    rfb.current = client;
    return () => {
      rfb.current = null;
      client.disconnect();
    };
  }, [vmId, generation]);

  return (
    <div className="terminal-block">
      <div className="toolbar">
        <Label
          status={
            status === 'connected'
              ? 'success'
              : status === 'connecting'
                ? 'info'
                : status === 'refused'
                  ? 'danger'
                  : 'warning'
          }
        >
          {status.toUpperCase()}
        </Label>
        <span className="field-note">
          {name ? `${name}. ` : ''}Click the display to type. One display session per VM at a time.
        </span>
        <span className="spacer" />
        <Button
          sm
          disabled={status !== 'connected'}
          onClick={() => {
            rfb.current?.sendCtrlAltDel();
          }}
        >
          Ctrl-Alt-Del
        </Button>
        {status !== 'connected' && status !== 'connecting' && (
          <Button
            sm
            icon="refresh"
            onClick={() => {
              setGeneration((g) => g + 1);
            }}
          >
            Reconnect
          </Button>
        )}
      </div>
      <div className="vnc-host" ref={host} />
    </div>
  );
}
