/**
 * Bake the example the e2e suite checks against with the native CLI, so the test compares
 * what the studio shows (baked by @etendue/wasm) with `etendue bake` (ADR 0003: one bake).
 */

import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const ROOT = fileURLToPath(new URL("../../../../", import.meta.url));
export const BAKED = `${ROOT}target/e2e/eye_in_hand_ur5e.baked.json`;

export default function globalSetup(): void {
  mkdirSync(`${ROOT}target/e2e`, { recursive: true });
  execFileSync(
    "cargo",
    [
      "run",
      "-q",
      "-p",
      "etendue-cli",
      "--",
      "bake",
      "examples/eye_in_hand_ur5e/scene.json",
      "examples/eye_in_hand_ur5e/scenario.json",
      "-o",
      BAKED,
    ],
    { cwd: ROOT, stdio: "inherit" },
  );
}
