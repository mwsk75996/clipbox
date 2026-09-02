// ----------
// Vite Configuration
// Description: Bundles the React frontend, configures path aliasing (@ -> ./frontend/src), and serves dev builds on port 1420 for Tauri v2.
// ----------

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "node:path";

export default defineConfig({
  plugins: [react()],
  root: "frontend",
  publicDir: "public",
  resolve: {
    alias: {
      "@": path.resolve(import.meta.dirname, "./frontend/src"),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    outDir: "../dist",
    emptyOutDir: true,
  },
});
