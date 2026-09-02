/**
 * Clarity tabs — horizontal (3px violet underline) or vertical (right rail).
 */
export interface TabsProps {
  /** [{label, content, icon?, badge?, disabled?}] */
  tabs: { label: string; content: React.ReactNode; icon?: string; badge?: number | string; disabled?: boolean }[];
  vertical?: boolean;
  defaultIndex?: number;
  onChange?: (index: number) => void;
}
