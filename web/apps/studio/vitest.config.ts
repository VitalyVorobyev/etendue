import { createRequire } from "node:module";

import react from "@vitejs/plugin-react";
import { playwright } from "@vitest/browser-playwright";
import { defaultClientConditions } from "vite";
import { defineConfig } from "vitest/config";

const setup = createRequire(import.meta.url).resolve("@vitavision/config-vitest/setup");

// The studio's pure logic in happy-dom (`*.test.ts`); measurements that need real WebGL and
// the wasm kernel in Chromium (`*.browser.test.ts`, gate G4.1 web side). The app as a whole
// is covered by the Playwright suite in e2e/.
export default defineConfig({
  plugins: [react()],
  resolve: { conditions: ["@vitavision/source", ...defaultClientConditions] },
  optimizeDeps: { exclude: ["@etendue/wasm"] },
  server: { fs: { allow: [new URL("../../..", import.meta.url).pathname] } },
  test: {
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          environment: "happy-dom",
          setupFiles: [setup],
          include: ["src/**/*.test.{ts,tsx}"],
          exclude: ["src/**/*.browser.test.{ts,tsx}"],
        },
      },
      {
        extends: true,
        test: {
          name: "browser",
          include: ["src/**/*.browser.test.{ts,tsx}"],
          testTimeout: 120_000,
          browser: {
            enabled: true,
            headless: true,
            provider: playwright({ launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] } }),
            instances: [{ browser: "chromium" }],
          },
        },
      },
    ],
  },
});
