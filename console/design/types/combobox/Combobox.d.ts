/**
 * Clarity combobox — type-ahead filtering select; `multi` renders picked
 * values as dismissable label pills.
 */
export interface ComboboxProps {
  options: string[];
  multi?: boolean;
  placeholder?: string;
  defaultValue?: string | string[];
  onChange?: (value: string | string[]) => void;
}
