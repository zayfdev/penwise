import { defineConfig } from "vitest/config";

/** penwise-server address used by the dev server proxy (`just serve` must be running). */
const SERVER = "http://127.0.0.1:7420";

/** HTTP routes of the server (`crates/penwise-server/src/api`, `pictures.rs`, `render.rs`, …). */
const HTTP_ROUTES = [
  "/pair",
  "/pair.png",
  "/pages",
  "/notebooks",
  "/text",
  "/search",
  "/page.png",
  "/assets",
  "/health",
];

export default defineConfig({
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "es2022",
    // `/assets/{id}` is the server's picture route, so bundle files must not live under `/assets/`.
    assetsDir: "static",
    assetsInlineLimit: 0,
  },
  server: {
    port: 5173,
    strictPort: true,
    proxy: {
      "/sync": { target: SERVER, ws: true, changeOrigin: true },
      ...Object.fromEntries(HTTP_ROUTES.map((r) => [r, { target: SERVER, changeOrigin: true }])),
    },
  },
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
