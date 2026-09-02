/**
 * Clarity tooltip — inverted (light-on-dark theme) hover bubble.
 * Keep content to one short clause; use Signpost for rich content.
 */
export interface TooltipProps {
  content: React.ReactNode;
  position?: 'top' | 'bottom';
  /** Max width: xs 72 / sm 120 / md 200 (default) / lg 288 */
  size?: 'xs' | 'sm' | 'lg';
  children: React.ReactNode;
}
