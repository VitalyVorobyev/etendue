import { createRequire } from "node:module";

import react from "@vitejs/plugin-react";
import { playwright } from "@vitest/browser-playwright";
import { DOD_COVERAGE } from "@vitavision/config-vitest";
import { defaultClientConditions } from "vite";
import { defineConfig } from "vitest/config";

const setup = createRequire(import.meta.url).resolve("@vitavision/config-vitest/setup");

// No components, so no stories: logic runs in happy-dom (`*.test.ts`), and what needs real
// WebGL runs in Chromium (`*.browser.test.ts`). One coverage report, held to the DoD.
export default defineConfig({
  plugins: [react()],
  resolve: { conditions: ["@vitavision/source", ...defaultClientConditions] },
  test: {
    coverage: {
      provider: "v8",
      include: ["src/**/*.ts"],
      exclude: ["src/**/*.test.ts", "src/index.ts"],
      reporter: ["text", "text-summary", "json-summary"],
      thresholds: DOD_COVERAGE,
    },
    projects: [
      {
        extends: true,
        test: {
          name: "unit",
          environment: "happy-dom",
          setupFiles: [setup],
          include: ["src/**/*.test.ts"],
          exclude: ["src/**/*.browser.test.ts"],
        },
      },
      {
        extends: true,
        test: {
          name: "browser",
          include: ["src/**/*.browser.test.ts"],
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
