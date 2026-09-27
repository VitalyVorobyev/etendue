/**
 * Gate G2.2 (docs/pivot/PLAN.md P2-3): with 2 robots, 4 cameras, 1 target and live scenario
 * playback, the p95 frame time over 600 frames is ≤ 16.7 ms in Chromium.
 *
 * Two numbers per frame, from an init script that wraps `requestAnimationFrame`:
 * - interval: time between consecutive animation frames (what the viewer sees);
 * - work: main-thread time spent in that frame's animation callbacks (R3F's render, the
 *   playback clock, React work they trigger).
 * The gate is on the interval; the work shows the headroom.
 */

import { mkdirSync, writeFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

import { ROOT } from "./global-setup";

const FRAMES = 600;

test("G2.2: playback frame times", async ({ page, browserName }) => {
  await page.addInitScript(() => {
    const w = window as unknown as { __frames: { t: number; work: number }[]; __record: boolean };
    w.__frames = [];
    w.__record = false;
    const raf = window.requestAnimationFrame.bind(window);
    let frameStart = -1;
    let frameWork = 0;
    window.requestAnimationFrame = (cb) =>
      raf((t) => {
        if (t !== frameStart) {
          if (frameStart >= 0 && w.__record) w.__frames.push({ t: frameStart, work: frameWork });
          frameStart = t;
          frameWork = 0;
        }
        const s = performance.now();
        try {
          cb(t);
        } finally {
          frameWork += performance.now() - s;
        }
      });
  });
  await page.goto("/");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.locator('input[type="file"]').first().setInputFiles([
    `${ROOT}web/apps/studio/e2e/fixtures/perf_scene.json`,
    `${ROOT}web/apps/studio/e2e/fixtures/perf_scenario.json`,
  ]);
  await expect(page.getByRole("treeitem", { name: /cam_bl/ })).toBeVisible({ timeout: 30_000 });
  await page.waitForTimeout(1500); // meshes

  await page.getByRole("button", { name: /loop/i }).click();
  await page.getByRole("button", { name: /^play/i }).click();
  await page.waitForTimeout(500);
  await page.evaluate(() => ((window as unknown as { __record: boolean }).__record = true));
  await page.waitForFunction((n) => (window as unknown as { __frames: unknown[] }).__frames.length > n, FRAMES, {
    timeout: 60_000,
  });
  const frames = await page.evaluate(() => (window as unknown as { __frames: { t: number; work: number }[] }).__frames);

  const sample = frames.slice(0, FRAMES + 1);
  const intervals = sample.slice(1).map((f, i) => f.t - sample[i]!.t);
  const work = sample.slice(1).map((f) => f.work);
  const pct = (xs: number[], p: number) => [...xs].sort((a, b) => a - b)[Math.ceil((p / 100) * xs.length) - 1]!;
  const stats = (xs: number[]) => ({
    p50: pct(xs, 50),
    p95: pct(xs, 95),
    p99: pct(xs, 99),
    max: Math.max(...xs),
  });
  const result = {
    gate: "G2.2",
    browser: browserName,
    userAgent: await page.evaluate(() => navigator.userAgent),
    renderer: await page.evaluate((): string => {
      const gl = document.createElement("canvas").getContext("webgl2");
      const ext = gl?.getExtension("WEBGL_debug_renderer_info");
      return ext ? String(gl!.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "unknown";
    }),
    frames: intervals.length,
    interval_ms: stats(intervals),
    work_ms: stats(work),
  };
  mkdirSync(`${ROOT}target/perf`, { recursive: true });
  writeFileSync(`${ROOT}target/perf/g2_2.json`, JSON.stringify(result, null, 2) + "\n");
  console.log(JSON.stringify(result, null, 2));
  expect(result.interval_ms.p95).toBeLessThanOrEqual(16.7);
});
