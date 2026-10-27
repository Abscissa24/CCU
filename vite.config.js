import { defineConfig } from 'vite';

export default defineConfig(({ command }) => ({
  server: { port: 8000, strictPort: true },
  // `esbuild` is a top-level Vite option (it was previously nested under
  // `build`, where Vite ignores it). Only strip console/debugger for
  // production builds so `npm run dev` keeps its logging.
  esbuild: command === 'build' ? { drop: ['console', 'debugger'] } : {},
  build: {
    sourcemap: false,
    minify: 'esbuild',
  },
}));
