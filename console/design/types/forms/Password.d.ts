/**
 * Clarity password input with show/hide eye toggle. Also exports Range
 * (slider) and FileInput (browse button + filename) from this module.
 */
export interface PasswordProps {
  value?: string;
  placeholder?: string;
  disabled?: boolean;
  onChange?: (e: any) => void;
}
