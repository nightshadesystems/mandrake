/**
 * Clarity-style horizontal bar chart — stacked segments, mono value label at
 * the bar end, link-colored category label under each bar (Orchestrator
 * "Top X by Y" anatomy).
 */
export interface BarChartProps {
  /** [{label, value?, color?, segments?: [{value,color?}], onClick?}] */
  items: { label: string; value?: number; color?: string; segments?: { value: number; color?: string }[]; onClick?: () => void }[];
  /** Scale max; defaults to the largest bar total */
  max?: number;
  valueFormat?: (v: number) => string;
  /** Rotated y-axis caption */
  axisLabel?: string;
}
