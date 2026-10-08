import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath, URL } from "node:url";

// In development the API runs on :8080 (cargo run); Vite proxies /api to it.
export default defineConfig({
  plugins: [react()],
  // Same alias as tsconfig.json "paths": @/* → src/*
  resolve: { alias: { "@": fileURLToPath(new URL("./src", import.meta.url)) } },
  server: {
    port: 5173,
    proxy: { "/api": { target: process.env.API_URL ?? "http://localhost:8080", changeOrigin: true } },
  },
  build: {
    chunkSizeWarningLimit: 900,
    rollupOptions: {
      // The S'Shop app, and the businesses' public websites (served by the server with their content — routes/site.rs).
      input: { main: fileURLToPath(new URL("./index.html", import.meta.url)), site: fileURLToPath(new URL("./site.html", import.meta.url)) },
      output: {
        manualChunks: {
          react: ["react", "react-dom", "react-router-dom", "@tanstack/react-query"],
          charts: ["recharts"],
          pdf: ["jspdf", "jspdf-autotable"],
          scanner: ["@zxing/browser", "@zxing/library"],
        },
      },
    },
  },
});
