import { defineConfig } from "@playwright/test";

/**
 * The widgets in the built book: build them (`bun run build`) and the book (`mdbook build book`)
 * first. Every page with a widget opens, every widget starts, and each answers a slider.
 */
export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  fullyParallel: true,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: { baseURL: "http://localhost:5180", viewport: { width: 1200, height: 900 } },
  webServer: {
    command: "bun e2e/serve.ts",
    url: "http://localhost:5180/index.html",
    reuseExistingServer: !process.env.CI,
  },
});
