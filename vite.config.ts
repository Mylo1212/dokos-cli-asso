import { defineConfig } from "vite";

// Port fixe : Tauri l'attend (tauri.conf.json, devUrl).
export default defineConfig({
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2022", outDir: "dist", emptyOutDir: true },
});
