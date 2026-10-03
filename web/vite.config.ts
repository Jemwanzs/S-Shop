import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

// In development the API runs on :8080 (cargo run); Vite proxies /api to it.
export default defineConfig({
  plugins: [react()],
  resolve: { alias: { "@": path.resolve(__dirname, "./src") } },
  server: {
    port: 5173,
    proxy: { "/api": { target: process.env.API_URL ?? "http://localhost:8080", changeOrigin: true } },
  },
  build: {
    chunkSizeWarningLimit: 900,
    rollupOptions: {
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
