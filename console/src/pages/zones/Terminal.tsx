// The zone console and VM serial console: xterm.js over the daemon's
// console WebSocket.

import { useEffect, useRef, useState } from 'react';
import { FitAddon } from '@xterm/addon-fit';
import { Terminal as XTerm } from '@xterm/xterm';
import '@xterm/xterm/css/xterm.css';

import { serialUrl } from '../../api/vms.ts';
import { consoleUrl } from '../../api/zones.ts';
import { Button, Label } from '../../design/index.tsx';

type Status = 'connecting' | 'connected' | 'closed' | 'refused';

export function ConsoleTerminal({ kind, id }: { kind: 'zone' | 'vm'; id: string }) {
  const host = useRef<HTMLDivElement | null>(null);
  const [status, setStatus] = useState<Status>('connecting');
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    const element = host.current;
    if (!element) return undefined;
    const term = new XTerm({
      cursorBlink: true,
      fontFamily: '"IBM Plex Mono", monospace',
      fontSize: 13,
      scrollback: 5000,
      theme: { background: '#0b0d12' },
    });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(element);
    fit.fit();
    setStatus('connecting');

    const socket = new WebSocket(
      kind === 'vm' ? serialUrl(id, term.cols, term.rows) : consoleUrl(id, term.cols, term.rows),
    );
    socket.binaryType = 'arraybuffer';
    socket.onopen = () => {
      setStatus('connected');
      term.focus();
    };
    socket.onmessage = (message: MessageEvent<ArrayBuffer | string>) => {
      if (typeof message.data === 'string') {
        term.write(message.data);
      } else {
        term.write(new Uint8Array(message.data));
      }
    };
    socket.onclose = (event) => {
      setStatus(event.code === 1006 && term.buffer.active.length <= 1 ? 'refused' : 'closed');
      term.write('\r\n\x1b[2m[disconnected]\x1b[0m\r\n');
    };
    socket.onerror = () => {
      setStatus('refused');
    };
    const input = term.onData((data) => {
      if (socket.readyState === WebSocket.OPEN) socket.send(data);
    });
    const resize = term.onResize(({ cols, rows }) => {
      if (socket.readyState === WebSocket.OPEN) {
        socket.send(JSON.stringify({ resize: { cols, rows } }));
      }
    });
    const observer = new ResizeObserver(() => {
      fit.fit();
    });
    observer.observe(element);

    return () => {
      observer.disconnect();
      input.dispose();
      resize.dispose();
      socket.close();
      term.dispose();
    };
  }, [kind, id, generation]);

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
          Ctrl-] is the console escape. One session per {kind} at a time.
        </span>
        <span className="spacer" />
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
      <div className="terminal-host" ref={host} />
    </div>
  );
}
