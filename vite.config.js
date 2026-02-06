import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
  },
  build: {
    target: ['es2021', 'chrome97', 'safari13'],
    minify: 'esbuild',
    sourcemap: false,
    outDir: 'dist',
  },
});
