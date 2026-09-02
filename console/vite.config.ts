import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Built assets land in console/dist and are embedded into mandraked via
// rust-embed (Phase 2). In development the dev server proxies /api and /ws to
// a running daemon; the proxy target is configured once the API exists.
export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
  },
});
