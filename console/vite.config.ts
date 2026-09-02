import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Built assets land in console/dist and are embedded into mandraked via
// rust-embed. In development the dev server proxies the API and the event
// WebSocket to a running daemon.
//
//   MANDRAKE_DEV_SERVER   daemon origin, default https://localhost:8443
//   MANDRAKE_DEV_TLS_DIR  directory with cert.pem and key.pem (the daemon's
//                         --tls-dir works); serves the dev server over
//                         HTTPS so the Secure session cookie is accepted
const daemon = process.env.MANDRAKE_DEV_SERVER ?? 'https://localhost:8443';
const tlsDir = process.env.MANDRAKE_DEV_TLS_DIR;
const https =
  tlsDir && existsSync(join(tlsDir, 'cert.pem'))
    ? { cert: readFileSync(join(tlsDir, 'cert.pem')), key: readFileSync(join(tlsDir, 'key.pem')) }
    : undefined;

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: 'dist',
    emptyOutDir: true,
    sourcemap: false,
  },
  server: {
    https,
    proxy: {
      '/api': {
        target: daemon,
        secure: false,
        ws: true,
      },
    },
  },
});
