import { readFileSync } from "node:fs";
import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

const host = process.env.TAURI_DEV_HOST;
const pkg = JSON.parse(
  readFileSync(new URL("./package.json", import.meta.url), "utf-8"),
);

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [svelte()],

  define: {
    __APP_VERSION__: JSON.stringify(pkg.version),
  },

  build: {
    // maplibre-gl alone is ~730 kB; nothing to be done about that, so don't
    // warn on it.
    chunkSizeWarningLimit: 800,
    rollupOptions: {
      output: {
        // Split maplibre out so the app chunk stays small and cacheable apart
        // from vendor code that changes rarely.
        manualChunks(id) {
          if (id.includes("node_modules/maplibre-gl")) return "maplibre";
        },
      },
    },
  },

  // Tauri expects a fixed port, fail if that port is not available
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? { protocol: "ws", host, port: 1421 }
      : undefined,
    watch: {
      // tell vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
