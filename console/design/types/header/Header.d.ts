/**
 * Clarity app header (56px, darkest ink): branding, context HeaderDropdowns
 * separated by HeaderDividers, and icon actions. Primary navigation belongs
 * in the Subnav tab row below it (Clarity Orchestrator anatomy). Also exports
 * HeaderDropdown, HeaderDivider, HeaderAction, Subnav.
 */
export interface HeaderProps {
  /** Brand mark image url */
  logo?: string;
  title?: string;
  /** Optional header links; prefer Subnav for primary nav */
  nav?: { label: string; href?: string; active?: boolean }[];
  onNavigate?: (item: any) => void;
  /** true or placeholder string renders the search box */
  search?: boolean | string;
  /** Right side — compose HeaderAction buttons */
  actions?: React.ReactNode;
  /** Context region — HeaderDropdown / HeaderDivider elements */
  children?: React.ReactNode;
}
