/**
 * Clarity vertical nav — flat full-bleed rows (no rounding), collapse
 * chevron at the top, violet group labels, selected row is a full-width
 * violet-soaked band. Collapses to a 48px icon rail.
 */
export interface VerticalNavProps {
  /** [{label?, items:[{id, label, icon?, badge?, badgeStatus?, active?}]}] */
  groups: { label?: string; items: { id?: string; label: string; icon?: string; badge?: number | string; badgeStatus?: 'info' | 'success' | 'warning' | 'danger'; active?: boolean }[] }[];
  /** Show the collapse trigger */
  collapsible?: boolean;
  defaultCollapsed?: boolean;
  activeId?: string;
  onNavigate?: (item: any) => void;
}
