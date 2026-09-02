/**
 * Clarity button. Default is the outline "action" button; `primary` is the
 * filled signal-violet call-to-action. One primary per view.
 * @startingPoint section="Components" subtitle="Clarity buttons, Nightshade violet" viewport="700x260"
 */
export interface ButtonProps {
  /** Visual variant */
  variant?: 'outline' | 'primary' | 'success' | 'warning' | 'danger' | 'neutral' | 'success-outline' | 'warning-outline' | 'danger-outline' | 'link' | 'link-neutral' | 'inverse';
  /** 24px compact height (default 32px) */
  sm?: boolean;
  /** Full-width */
  block?: boolean;
  /** Clarity icon shape rendered before the text; icon-only when no children */
  icon?: string;
  /** Clarity icon shape rendered after the text */
  iconRight?: string;
  /** Replaces icon with an inline spinner */
  loading?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  children?: React.ReactNode;
}
export interface ButtonGroupProps {
  /** Buttons to fuse into one segmented group */
  children: React.ReactNode;
}
