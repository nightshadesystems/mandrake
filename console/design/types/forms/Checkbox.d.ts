/**
 * Clarity selection controls: Checkbox (16px, 2px stroke, supports
 * indeterminate), Radio, and Toggle switch — all exported from this module.
 */
export interface CheckboxProps {
  label?: string;
  checked?: boolean;
  defaultChecked?: boolean;
  indeterminate?: boolean;
  disabled?: boolean;
  onChange?: (e: any) => void;
}
