// Dark by default; light via data-theme on <html>, remembered per browser,
// as the design system's own kit does.

import { useEffect, useState } from 'react';

export type Theme = 'dark' | 'light';
const KEY = 'mandrake-console-theme';

function stored(): Theme {
  try {
    return localStorage.getItem(KEY) === 'light' ? 'light' : 'dark';
  } catch {
    return 'dark';
  }
}

export function useTheme(): [Theme, () => void] {
  const [theme, setTheme] = useState<Theme>(stored);
  useEffect(() => {
    document.documentElement.setAttribute('data-theme', theme);
    try {
      localStorage.setItem(KEY, theme);
    } catch {
      // Storage may be unavailable; the attribute still applies.
    }
  }, [theme]);
  return [
    theme,
    () => {
      setTheme((t) => (t === 'dark' ? 'light' : 'dark'));
    },
  ];
}
