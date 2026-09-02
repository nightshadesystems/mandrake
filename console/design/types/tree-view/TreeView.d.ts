/**
 * Clarity tree view — expandable hierarchy with selection. 32px nodes,
 * indented children with a hairline rail.
 */
export interface TreeViewProps {
  /** Recursive: {id, label, icon?, expanded?, children?} */
  nodes: { id?: string; label: string; icon?: string; expanded?: boolean; children?: any[] }[];
  defaultActiveId?: string;
  onSelect?: (node: any) => void;
}
