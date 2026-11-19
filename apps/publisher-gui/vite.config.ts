import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Imaginears PackPublisher UI. Own port (the launcher's dev server uses 1420).
// Run from this folder (Tauri's before-commands do), so `root` is this folder.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1430,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    emptyOutDir: true,
  },
});
