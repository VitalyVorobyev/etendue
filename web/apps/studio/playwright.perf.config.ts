import { defineConfig } from "@playwright/test";

/**
 * Gate G2.2 (docs/pivot/PLAN.md P2-3): frame times of live playback. Runs on the machine's
 * GPU (no SwiftShader), against a production build. Local only; see
 * docs/measurements/g2_2_perf.md.
 */
export default defineConfig({
  testDir: "e2e",
  testMatch: "perf.spec.ts",
  timeout: 120_000,
  workers: 1,
  reporter: "list",
  // Headed: headless Chromium renders WebGL on SwiftShader (CPU), which measures the wrong
  // thing. A headed window gets the GPU (ANGLE on Metal).
  use: { baseURL: "http://localhost:5179", viewport: { width: 1600, height: 1000 }, headless: false },
  webServer: {
    command: "bun run build && bun x vite preview",
    url: "http://localhost:5179",
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
