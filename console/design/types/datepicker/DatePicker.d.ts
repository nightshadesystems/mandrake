/**
 * Clarity datepicker — input group with calendar-button trigger and
 * month-grid popover. Monday-first, mono digits, violet selection.
 */
export interface DatePickerProps {
  /** Initial date (Date-parsable) */
  defaultValue?: string;
  onChange?: (date: Date) => void;
  placeholder?: string;
}
