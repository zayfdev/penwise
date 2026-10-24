import { defineConfig } from "vite";

/** penwise-server address used by the dev server proxy (`just serve` must be running). */
const SERVER = "http://127.0.0.1:7420";

export default defineConfig({
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    // The server embeds dist/ (rust-embed); keep asset names hashed for immutable caching.
    assetsInlineLimit: 0,
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/sync": { target: SERVER, ws: true, changeOrigin: true },
      "/api": { target: SERVER, changeOrigin: true },
      "/pair": { target: SERVER, changeOrigin: true },
    },
  },
});
