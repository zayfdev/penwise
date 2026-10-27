import { defineConfig } from "vitest/config";

/** Browser end-to-end: real server binary + built viewer + headless Chromium (`npm run e2e`). */
export default defineConfig({
  test: {
    include: ["e2e/**/*.e2e.ts"],
    environment: "node",
    testTimeout: 60_000,
    hookTimeout: 60_000,
    fileParallelism: false,
  },
});
