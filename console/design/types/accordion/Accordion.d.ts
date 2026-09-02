/**
 * Clarity accordion — stacked panels, one (or `multi`) open, 4px violet
 * active edge. Also exports CollapsiblePanel (single standalone panel).
 */
export interface AccordionProps {
  panels: { title: string; description?: string; content: React.ReactNode }[];
  /** Allow several panels open at once */
  multi?: boolean;
  /** Indexes open initially */
  defaultOpen?: number[];
}
