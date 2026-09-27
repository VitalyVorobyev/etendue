// @ts-check
import { recommended, tokensOnly } from "@vitavision/config-eslint";

export default [
  { ignores: ["**/dist/**", "**/*.config.ts", "**/test-results/**", "**/playwright-report/**"] },
  ...recommended({ tsconfigRootDir: import.meta.dirname }),
  // Gate G5.1 (lab-ui): component sources use design tokens only — no raw palette
  // classes or hex. The incubated packages are held to it from day one so they move
  // to lab-ui unchanged; the studio too, so it looks like every other vitavision app.
  tokensOnly(["packages/*/src/**/*.{ts,tsx}", "apps/*/src/**/*.{ts,tsx}"]),
];
