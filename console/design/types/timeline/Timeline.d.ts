/**
 * Clarity timeline — horizontal or vertical progress chain with per-step
 * state nodes (success / error / current / processing).
 */
export interface TimelineProps {
  /** [{title, header? (mono timestamp), description?, state?}] */
  steps: { title: string; header?: string; description?: string; state?: 'success' | 'error' | 'current' | 'processing' }[];
  vertical?: boolean;
}
