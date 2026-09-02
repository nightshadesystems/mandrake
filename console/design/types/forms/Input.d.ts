/**
 * Clarity text input (32px, 2px stroke, radius 4). Also exports Textarea,
 * NumberInput, and InputGroup (prefix/suffix addons) from this module.
 */
export interface InputProps {
  value?: string;
  placeholder?: string;
  disabled?: boolean;
  type?: string;
  onChange?: (e: any) => void;
}
