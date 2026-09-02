/**
 * Clarity alert — standard (tinted, bordered) or app-level (saturated
 * full-bleed bar). Status icon is automatic.
 */
export interface AlertProps {
  status?: 'info' | 'success' | 'warning' | 'danger';
  /** Saturated full-width app bar (goes above the header) */
  appLevel?: boolean;
  /** Compact 16px-line variant */
  sm?: boolean;
  closable?: boolean;
  onClose?: () => void;
  /** Inline action links, e.g. [{label:"Acknowledge", onClick}] */
  actions?: { label: string; onClick?: () => void }[];
  /** Multiple stacked alert texts in one container */
  items?: React.ReactNode[];
  children?: React.ReactNode;
}
