/**
 * Clarity-style line/area chart (SVG) — hairline grid, mono axis labels,
 * viz-token series colors, optional area fill. Y-axis ticks are quantized to
 * nice round steps, so `yFormat` never receives float noise.
 */
export interface LineChartProps {
  /** [{label?, color?, points: number[]}] */
  series: { label?: string; color?: string; points: number[] }[];
  /** X-axis labels (evenly spaced) */
  labels?: string[];
  /** Pixel height (default 160) */
  height?: number;
  /** Soft area fill under each line (default true) */
  area?: boolean;
  /** Formats each y-axis tick; receives an already-rounded number */
  yFormat?: (v: number) => string;
}
