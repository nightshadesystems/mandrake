/**
 * Clarity modal (sm 288 / md 576 / lg 864 / xl 1152) and SidePanel
 * (right sheet) — both exported here. Backdrop click closes.
 */
export interface ModalProps {
  open: boolean;
  title: string;
  size?: 'sm' | 'lg' | 'xl';
  onClose?: () => void;
  /** Right-aligned action buttons */
  footer?: React.ReactNode;
  children?: React.ReactNode;
}
