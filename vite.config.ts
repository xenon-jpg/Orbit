import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    host: '0.0.0.0', port: 5173, strictPort: true,
    allowedHosts: ['.e2b.app', 'localhost', '127.0.0.1'],
    watch: { ignored: ['**/src-tauri/target/**', '**/artifacts/**', '**/test-results/**', '**/playwright-report/**'] },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { target: 'es2022', sourcemap: false },
});
