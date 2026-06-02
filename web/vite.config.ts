import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The SPA is served by the gateway (server/index.mjs) in production. In dev,
// `/api` and `/events` are proxied to a locally running gateway on :8090.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5180,
    proxy: {
      "/api": { target: "http://127.0.0.1:8090", changeOrigin: true, ws: true },
      "/events": { target: "http://127.0.0.1:8090", changeOrigin: true, ws: true },
    },
  },
  build: { outDir: "dist", sourcemap: false },
});
