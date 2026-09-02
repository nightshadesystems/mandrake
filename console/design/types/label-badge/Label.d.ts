/**
 * Clarity label (pill tag) and Badge (count bubble) — both exported here.
 * Severity states in operator UI are labels/badges with mono uppercase text.
 */
export interface LabelProps {
  status?: 'info' | 'success' | 'warning' | 'danger';
  /** Violet-soaked brand pill */
  accent?: boolean;
  clickable?: boolean;
  dismissable?: boolean;
  onDismiss?: () => void;
  /** Trailing count badge inside the pill */
  badge?: number | string;
  children: React.ReactNode;
}
