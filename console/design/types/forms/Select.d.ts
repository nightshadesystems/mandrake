/**
 * Clarity select with caret. Also exports Datalist (input + native suggestions).
 */
export interface SelectProps {
  /** Options as strings or {value,label}; or pass <option> children */
  options?: (string | { value: string; label: string })[];
  value?: string;
  disabled?: boolean;
  onChange?: (e: any) => void;
  children?: React.ReactNode;
}
