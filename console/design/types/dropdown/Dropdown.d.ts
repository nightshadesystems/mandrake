/**
 * Clarity dropdown — trigger button + overlay menu with headers, dividers,
 * icons, disabled items. Closes on outside click.
 */
export interface DropdownProps {
  /** Trigger button label (node allowed) */
  trigger?: React.ReactNode;
  variant?: 'outline' | 'primary' | 'link' | 'link-neutral' | 'neutral';
  sm?: boolean;
  /** {label, icon?, disabled?, onClick?} | {divider:true} | {header:string} */
  items: ({ label?: string; icon?: string; disabled?: boolean; onClick?: () => void; divider?: boolean; header?: string; expandable?: boolean })[];
  /** Right-align the menu */
  right?: boolean;
}
