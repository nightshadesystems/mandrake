/**
 * Clarity datagrid — sortable columns, row selection, expandable detail rows,
 * pagination footer, action bar, empty placeholder. 32px rows (24 compact).
 * @startingPoint section="Components" subtitle="Sortable, selectable data grid" viewport="700x420"
 */
export interface DatagridProps {
  /** [{key, label, sortable?, width?, render?(row)}] */
  columns: { key: string; label: string; sortable?: boolean; width?: number | string; render?: (row: any) => React.ReactNode }[];
  rows: any[];
  /** Checkbox select column */
  selectable?: boolean;
  /** Caret column + renderDetail(row) detail rows */
  expandable?: boolean;
  renderDetail?: (row: any) => React.ReactNode;
  /** Rows per page; 0 = no pagination */
  pageSize?: number;
  /** Batch-action toolbar; receives the selection Set */
  actionBar?: (selection: Set<number>) => React.ReactNode;
  placeholder?: string;
  footerText?: string;
  /** 24px rows */
  compact?: boolean;
}
