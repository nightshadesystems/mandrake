// The Clarity icons custom element, registered by public/vendor/clr-icons.min.js.
// The design-system components emit it with `class` rather than `className`.

import type { DetailedHTMLProps, HTMLAttributes } from 'react';

declare module 'react' {
  namespace JSX {
    interface IntrinsicElements {
      'clr-icon': DetailedHTMLProps<HTMLAttributes<HTMLElement>, HTMLElement> & {
        shape?: string;
        size?: string | number;
        dir?: 'up' | 'down' | 'left' | 'right';
        class?: string;
      };
    }
  }
}
