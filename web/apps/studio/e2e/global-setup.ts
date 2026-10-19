/**
 * Bake the example the e2e suite checks against with the native CLI, so the test compares
 * what the studio shows (baked by @etendue/wasm) with `etendue bake` (ADR 0003: one bake); and
 * calibrate an example natively, so the mocked shell answers with a real calibration report.
 *
 * `ETENDUE_CARGO_CONFIG` (optional) is passed to cargo as `--config`: a `[patch.crates-io]`
 * table pointing calibration-rs at another checkout (CLAUDE.md: the locks follow the tag).
 */

import { execFileSync } from "node:child_process";
import { mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";

export const ROOT = fileURLToPath(new URL("../../../../", import.meta.url));
export const BAKED = `${ROOT}target/e2e/eye_in_hand_ur5e.baked.json`;
/** `etendue calibrate`'s report on closed_loop_ur5e's exact observations. */
export const CALIBRATION = "target/e2e/calibrate/report.json";

function etendue(...args: string[]): void {
  const config = process.env.ETENDUE_CARGO_CONFIG;
  execFileSync("cargo", ["run", "-q", ...(config ? ["--config", config] : []), "-p", "etendue-cli", "--", ...args], {
    cwd: ROOT,
    stdio: "inherit",
  });
}

export default function globalSetup(): void {
  mkdirSync(`${ROOT}target/e2e`, { recursive: true });
  etendue("bake", "examples/eye_in_hand_ur5e/scene.json", "examples/eye_in_hand_ur5e/scenario.json", "-o", BAKED);
  const ex = "examples/closed_loop_ur5e";
  etendue("gt", `${ex}/scene.json`, `${ex}/scenario.json`, "-o", "target/e2e/calibrate");
  etendue("calibrate", "target/e2e/calibrate", "-o", CALIBRATION);
}
