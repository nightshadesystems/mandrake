/**
 * Clarity card — 1px border, 8px radius, hairline shadow. Compose with
 * CardBlock / CardMediaBlock (exported here). Clickable cards hover violet.
 * @startingPoint section="Components" subtitle="Clarity card container" viewport="700x260"
 */
export interface CardProps {
  /** Header bar with bottom divider */
  header?: React.ReactNode;
  /** Footer with action buttons/links */
  footer?: React.ReactNode;
  /** Hover: violet border + shadow */
  clickable?: boolean;
  /** Full-bleed image on top */
  img?: string;
  onClick?: () => void;
  children?: React.ReactNode;
}
