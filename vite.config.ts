import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { createWebDevelopmentBrowserPlugin } from "./src/dev/vite-browser-launch.ts";
import {
  WEB_DEVELOPMENT_BACKEND_HEALTH_URL,
  WEB_DEVELOPMENT_BACKEND_READY_INTERVAL_MS,
} from "./src/dev/web-backend.ts";
import tauriConfig from "./src-tauri/tauri.conf.json" with { type: "json" };

const developmentUrl = new URL(tauriConfig.build.devUrl);
const browserDevelopment = Object.freeze({
  origin: developmentUrl.origin,
  host: developmentUrl.hostname,
  port: Number(developmentUrl.port),
});

export default defineConfig({
  plugins: [
    react(),
    tailwindcss(),
    createWebDevelopmentBrowserPlugin({
      enabled: process.env.ASB_WEB_DEVELOPMENT === "1",
      healthUrl: WEB_DEVELOPMENT_BACKEND_HEALTH_URL,
      origin: browserDevelopment.origin,
      retryDelayMs: WEB_DEVELOPMENT_BACKEND_READY_INTERVAL_MS,
    }),
  ],
  clearScreen: false,
  resolve: {
    alias: {
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  server: {
    host: browserDevelopment.host,
    port: browserDevelopment.port,
    strictPort: true,
    open: false,
    // Cargo output lives under target/ plus one-off target-* / .tmp*
    // verification dirs; watching them starves the dev server on Windows.
    watch: {
      ignored: ["**/target/**", "**/target-*/**", "**/.tmp*/**", "**/node_modules/**"],
    },
  },
  build: {
    target: "es2022",
    outDir: "dist",
    rollupOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        tray: fileURLToPath(new URL("./tray.html", import.meta.url)),
      },
    },
  },
});
