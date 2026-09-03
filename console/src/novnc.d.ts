// noVNC ships no typings; this is the part of core/rfb.js the console uses
// (docs/API.md in the package).

declare module '@novnc/novnc' {
  export interface RfbOptions {
    shared?: boolean;
    credentials?: { username?: string; password?: string; target?: string };
    wsProtocols?: string[];
  }

  export interface RfbDisconnectDetail {
    clean: boolean;
  }

  export interface RfbSecurityFailureDetail {
    status: number;
    reason: string;
  }

  export default class RFB extends EventTarget {
    constructor(target: HTMLElement, urlOrChannel: string | WebSocket, options?: RfbOptions);
    viewOnly: boolean;
    focusOnClick: boolean;
    clipViewport: boolean;
    dragViewport: boolean;
    scaleViewport: boolean;
    resizeSession: boolean;
    showDotCursor: boolean;
    background: string;
    qualityLevel: number;
    compressionLevel: number;
    disconnect(): void;
    sendCredentials(credentials: RfbOptions['credentials']): void;
    sendKey(keysym: number, code: string | null, down?: boolean): void;
    sendCtrlAltDel(): void;
    focus(options?: FocusOptions): void;
    blur(): void;
    machineShutdown(): void;
    machineReboot(): void;
    machineReset(): void;
    clipboardPasteFrom(text: string): void;
    addEventListener(type: 'connect', listener: (event: CustomEvent<undefined>) => void): void;
    addEventListener(
      type: 'disconnect',
      listener: (event: CustomEvent<RfbDisconnectDetail>) => void,
    ): void;
    addEventListener(
      type: 'credentialsrequired',
      listener: (event: CustomEvent<{ types: string[] }>) => void,
    ): void;
    addEventListener(
      type: 'securityfailure',
      listener: (event: CustomEvent<RfbSecurityFailureDetail>) => void,
    ): void;
    addEventListener(
      type: 'desktopname',
      listener: (event: CustomEvent<{ name: string }>) => void,
    ): void;
    addEventListener(type: string, listener: EventListenerOrEventListenerObject): void;
  }
}
