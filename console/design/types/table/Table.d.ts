/**
 * Clarity basic table — static presentation without datagrid behavior.
 * Use Datagrid when you need sorting/selection/pagination.
 */
export interface TableProps {
  columns: { key: string; label: string; align?: 'left' | 'right'; width?: number | string; render?: (row: any) => React.ReactNode }[];
  rows: any[];
  /** 4px vertical padding */
  compact?: boolean;
  /** Strip container border/background */
  noborder?: boolean;
}
