import { defineConfig } from "vitest/config";

// The widgets' pure logic (`src/**/*.test.ts`); the widgets themselves are covered by the
// Playwright suite in e2e/, against the built book.
export default defineConfig({
  test: { include: ["src/**/*.test.ts"], environment: "node" },
});
