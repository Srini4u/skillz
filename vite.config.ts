import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and ignores Rust file changes.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  // A desktop app loads from disk, so one bundle is fine.
  build: { chunkSizeWarningLimit: 1500 },
  server: { port: 1420, strictPort: true, watch: { ignored: ["**/src-tauri/**", "**/crates/**"] } },
});
