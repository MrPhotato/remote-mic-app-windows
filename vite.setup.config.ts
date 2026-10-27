import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { fileURLToPath } from "node:url";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: { host: "127.0.0.1", port: 2431, strictPort: true, watch: { ignored: ["**/target/**"] } },
  build: {
    outDir: "dist-setup",
    rollupOptions: { input: fileURLToPath(new URL("./setup.html", import.meta.url)) },
  },
  envPrefix: ["VITE_", "TAURI_"],
});
