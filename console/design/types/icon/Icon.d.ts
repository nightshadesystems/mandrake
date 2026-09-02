/**
 * Clarity Icons wrapper. Requires the @clr/icons CDN tags (see readme.md
 * Iconography). Inherits currentColor.
 */
export interface IconProps {
  /** Clarity icon shape name, e.g. "cog", "vm", "exclamation-triangle" */
  shape: string;
  /** Pixel size (default 16) */
  size?: number;
  /** Filled variant */
  solid?: boolean;
  /** Rotation for directional shapes: up | down | left | right */
  dir?: 'up' | 'down' | 'left' | 'right';
  /** Show notification badge dot */
  badge?: boolean;
}
