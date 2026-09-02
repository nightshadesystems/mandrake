// The event stream: a WebSocket on /api/v1/events, authenticated by the
// session cookie, resumed with `since` after a drop.

import { useEffect, useRef } from 'react';

import type { Event } from './hooks';

const RETRY_MS = [1_000, 2_000, 5_000, 10_000];

/** Subscribe to live events while the component is mounted. */
export function useEvents(onEvent: (event: Event) => void): void {
  const handler = useRef(onEvent);
  useEffect(() => {
    handler.current = onEvent;
  });

  useEffect(() => {
    let socket: WebSocket | undefined;
    let closed = false;
    let attempt = 0;
    let lastId: string | undefined;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const connect = () => {
      const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
      const since = lastId ? `?since=${encodeURIComponent(lastId)}` : '';
      socket = new WebSocket(`${scheme}://${window.location.host}/api/v1/events${since}`);
      socket.onopen = () => {
        attempt = 0;
      };
      socket.onmessage = (message: MessageEvent<string>) => {
        try {
          const event = JSON.parse(message.data) as Event;
          lastId = event.id;
          handler.current(event);
        } catch {
          // A frame that is not an event is ignored.
        }
      };
      socket.onclose = () => {
        if (closed) return;
        const delay = RETRY_MS[Math.min(attempt, RETRY_MS.length - 1)] ?? 10_000;
        attempt += 1;
        timer = setTimeout(connect, delay);
      };
    };
    connect();

    return () => {
      closed = true;
      if (timer) clearTimeout(timer);
      socket?.close();
    };
  }, []);
}
