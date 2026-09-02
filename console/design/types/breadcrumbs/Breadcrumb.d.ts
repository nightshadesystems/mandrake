/**
 * Clarity breadcrumbs — slash-separated trail; last item is the current page.
 */
export interface BreadcrumbProps {
  items: { label: string; href?: string; onClick?: () => void }[];
}
