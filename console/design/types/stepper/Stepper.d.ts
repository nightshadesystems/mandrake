/**
 * Clarity stepper — sequential form accordion with numbered steps,
 * completion checks, and Back/Next controls.
 */
export interface StepperProps {
  steps: { title: string; description?: string; content: React.ReactNode; error?: boolean }[];
  onFinish?: () => void;
}
