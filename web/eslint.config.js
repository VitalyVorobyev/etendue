// @ts-check
import { recommended, tokensOnly } from "@vitavision/config-eslint";

export default [
  {
    ignores: [
      "**/dist/**",
      "**/*.config.ts",
      "**/test-results/**",
      "**/playwright-report/**",
      // The Tauri shell's cargo output (generated scripts).
      "**/src-tauri/**",
    ],
  },
  ...recommended({ tsconfigRootDir: import.meta.dirname }),
  // lab-ui's tokens-only rule: the studio uses design tokens only — no raw palette
  // classes or hex — so it looks like every other vitavision app.
  tokensOnly(["apps/*/src/**/*.{ts,tsx}"]),
];
