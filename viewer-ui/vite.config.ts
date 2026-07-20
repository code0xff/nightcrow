import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// `base: ""` emits relative asset URLs, so the built page works when served
// from the embedded server without knowing its mount path in advance.
export default defineConfig({
  plugins: [react(), tailwindcss()],
  base: "",
  build: { outDir: "dist", emptyOutDir: true },
  server: {
    // Dev only: Vite owns the page, the Rust server owns the API.
    proxy: {
      "/api": { target: "http://127.0.0.1:8091", changeOrigin: true },
      "/login": { target: "http://127.0.0.1:8091", changeOrigin: true },
      "/ws": { target: "ws://127.0.0.1:8091", ws: true },
    },
  },
});
