/**
 * Clarity progress bar (12px pill track). Also exports Spinner (16/32/64)
 * and Skeleton loading block from this module.
 */
export interface ProgressBarProps {
  value?: number;
  max?: number;
  status?: 'success' | 'warning' | 'danger';
  /** Indeterminate looping bar */
  loop?: boolean;
  /** 4px thin track */
  sm?: boolean;
  /** Left label */
  label?: string;
  /** Right mono percentage */
  showValue?: boolean;
}
