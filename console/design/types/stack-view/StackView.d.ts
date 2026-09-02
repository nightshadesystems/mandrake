/**
 * Clarity stack view — key/value property list with expandable nested rows.
 * The standard "details" panel of a console.
 */
export interface StackViewProps {
  /** [{key, value, expanded?, children?: [{key,value}]}] */
  blocks: { key: string; value?: React.ReactNode; expanded?: boolean; children?: { key: string; value?: React.ReactNode }[] }[];
}
