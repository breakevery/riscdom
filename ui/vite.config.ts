import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],

  // Tauri expects a fixed port and no screen clearing.
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    target: "es2021",
    // The built front end lives one level down, under a parent that is never
    // emptied (v0.9.9 3/N-fix2, decision §69): `tauri-build` checks
    // `bundle.resources` paths with `Path::exists()`, so `../dist` must exist in a
    // fresh checkout — and the parent holds the tracked `.gitkeep` that keeps it
    // there, while everything Vite writes (and empties) is under `app/`.
    outDir: "dist/app",
    emptyOutDir: true,
  },
});
