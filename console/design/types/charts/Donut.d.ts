/**
 * Clarity-style donut chart (SVG, viz tokens) with optional center metric.
 * Also exports ChartLegend (swatch + "Label: value" rows, thousands space-
 * grouped per the brand's numeral convention), ChartStat (big mono number
 * with label), VIZ series palette, and SEVERITY color map.
 */
export interface DonutProps {
  /** [{label, value, color?}] — colors default to the VIZ series palette */
  segments: { label: string; value: number; color?: string }[];
  /** Diameter in px (default 96) */
  size?: number;
  /** Ring thickness (default 12) */
  thickness?: number;
  /** Center metric: {value, label?} */
  center?: { value: string | number; label?: string };
}
