/**
 * Clarity form container: label + control + helper/validation subtext.
 * Wrap any Input/Select/Checkbox group in one.
 */
export interface FormFieldProps {
  label?: string;
  /** Helper text under the control */
  helper?: string;
  /** Error message — paints the control and subtext red */
  error?: string;
  /** Success message — paints success teal */
  success?: string;
  required?: boolean;
  htmlFor?: string;
  children: React.ReactNode;
}
