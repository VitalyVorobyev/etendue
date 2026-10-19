/**
 * Gate G8.1 (docs/pivot/PLAN.md P8): during a 5 s slider drag on `rig3_laser_ur5e` there are no
 * bakes, the main thread's JS work per frame has p95 ≤ 4 ms, and the frame interval has p95
 * ≤ 1.1× the display period (the idle median interval, measured first).
 *
 * The work of a frame is everything the page's JS did since the previous frame: animation
 * callbacks (R3F's render), pointer and mouse listeners (the slider), and message-port tasks
 * (React's scheduler, which renders a slider's updates). An init script wraps all three.
 */

import { mkdirSync, writeFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

import { installFrameProbe, intervals, pct, record, stats } from "./frames";
import { ROOT } from "./global-setup";

const DRAG_MS = 5000;

test("G8.1: frame times during a slider drag", async ({ page, browserName }) => {
  await installFrameProbe(page);
  await page.goto("/?example=rig3_laser_ur5e");
  await expect(page.getByRole("treeitem", { name: /rig_cam0/ })).toBeVisible({ timeout: 30_000 });
  await page.waitForTimeout(1500); // meshes
  await page.getByRole("treeitem", { name: /^rig\b/ }).click();

  const idle = await record(page, 2000);
  const period = pct(intervals(idle), 50);

  const bakes = () => page.evaluate(() => performance.getEntriesByName("etendue:bake").length);
  const bakesBefore = await bakes();
  const thumb = page.getByTestId("pose-x").getByRole("slider");
  const box = (await thumb.boundingBox())!;
  const [x0, y0] = [box.x + box.width / 2, box.y + box.height / 2];
  let bakesDuring = -1;
  const drag = await record(page, DRAG_MS, async () => {
    await page.mouse.move(x0, y0);
    await page.mouse.down();
    const start = Date.now();
    while (Date.now() - start < DRAG_MS) {
      const t = (Date.now() - start) / 1000;
      await page.mouse.move(x0 + 60 * Math.sin(2 * Math.PI * 0.5 * t), y0);
      await page.waitForTimeout(4);
    }
    bakesDuring = (await bakes()) - bakesBefore;
    await page.mouse.up();
  });

  const result = {
    gate: "G8.1",
    browser: browserName,
    userAgent: await page.evaluate(() => navigator.userAgent),
    renderer: await page.evaluate((): string => {
      const gl = document.createElement("canvas").getContext("webgl2");
      const ext = gl?.getExtension("WEBGL_debug_renderer_info");
      return ext ? String(gl!.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "unknown";
    }),
    display_period_ms: period,
    idle: { frames: idle.length - 1, interval_ms: stats(intervals(idle)), work_ms: stats(idle.map((f) => f.work)) },
    drag: {
      frames: drag.length - 1,
      bakes: bakesDuring,
      interval_ms: stats(intervals(drag)),
      work_ms: stats(drag.slice(1).map((f) => f.work)),
    },
    commit_bakes: (await bakes()) - bakesBefore - bakesDuring,
  };
  mkdirSync(`${ROOT}target/perf`, { recursive: true });
  writeFileSync(`${ROOT}target/perf/g8_1.json`, JSON.stringify(result, null, 2) + "\n");
  console.log(JSON.stringify(result, null, 2));
  expect(result.drag.bakes).toBe(0);
  expect(result.drag.work_ms.p95).toBeLessThanOrEqual(4);
  expect(result.drag.interval_ms.p95).toBeLessThanOrEqual(1.1 * period);
});
