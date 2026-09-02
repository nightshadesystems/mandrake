/**
 * Clarity signpost — click-to-open rich info popover with title + close.
 * Use where a tooltip is too small (multi-line help, links).
 */
export interface SignpostProps {
  title?: string;
  children: React.ReactNode;
}
