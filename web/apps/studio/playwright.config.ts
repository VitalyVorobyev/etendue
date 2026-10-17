import { defineConfig } from "@playwright/test";

/**
 * End-to-end tests of the studio against a production build (`vite build` + `vite preview`).
 * WebGL runs on SwiftShader so the suite behaves the same on every CI runner.
 */
export default defineConfig({
  testDir: "e2e",
  testMatch: ["studio.spec.ts", "tauri.spec.ts"],
  globalSetup: "./e2e/global-setup.ts",
  timeout: 60_000,
  fullyParallel: true,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:5179",
    viewport: { width: 1600, height: 1000 },
    launchOptions: { args: ["--use-angle=swiftshader", "--enable-unsafe-swiftshader"] },
  },
  webServer: {
    command: "bun run build && bun x vite preview",
    url: "http://localhost:5179",
    reuseExistingServer: !process.env.CI,
    timeout: 180_000,
  },
});
