/**
 * Clarity wizard — large modal with left step-nav rail and
 * Cancel/Back/Next/Finish footer.
 */
export interface WizardProps {
  open: boolean;
  /** Rail heading */
  title: string;
  steps: { title: string; navTitle?: string; content: React.ReactNode }[];
  onClose?: () => void;
  onFinish?: () => void;
}
