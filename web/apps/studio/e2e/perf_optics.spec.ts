/**
 * Gate G10.1, studio side (docs/pivot/PLAN.md P10): dragging a slider of the Optics workspace
 * redraws the Scheimpflug section and the 64 × 48 window map within a frame: JS work per frame
 * p95 ≤ 16 ms during a 4 s drag of the focus slider. Headed, on the GPU, like G2.2 and G8.1.
 */

import { mkdirSync, writeFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

import { installFrameProbe, intervals, pct, record, stats } from "./frames";
import { ROOT } from "./global-setup";

const DRAG_MS = 4000;

test("G10.1: Optics redraw while a slider moves", async ({ page }) => {
  await installFrameProbe(page);
  await page.goto("/?example=rig3_laser_ur5e");
  await expect(page.getByRole("treeitem", { name: /rig_cam0/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("radio", { name: "Optics" }).click();
  await expect(page.getByTestId("section-view")).toBeVisible();
  await page.waitForTimeout(500);
  const idle = await record(page, 1500);
  const thumb = page.getByTestId("focus").getByRole("slider");
  const box = (await thumb.boundingBox())!;
  const [x0, y0] = [box.x + box.width / 2, box.y + box.height / 2];
  const drag = await record(page, DRAG_MS, async () => {
    await page.mouse.move(x0, y0);
    await page.mouse.down();
    const start = Date.now();
    while (Date.now() - start < DRAG_MS) {
      const t = (Date.now() - start) / 1000;
      await page.mouse.move(x0 + 30 + 30 * Math.sin(2 * Math.PI * 0.5 * t), y0);
      await page.waitForTimeout(4);
    }
    await page.mouse.up();
  });
  const work = drag.slice(1).map((f) => f.work);
  const result = {
    gate: "G10.1",
    renderer: await page.evaluate((): string => {
      const gl = document.createElement("canvas").getContext("webgl2");
      const ext = gl?.getExtension("WEBGL_debug_renderer_info");
      return ext ? String(gl!.getParameter(ext.UNMASKED_RENDERER_WEBGL)) : "unknown";
    }),
    display_period_ms: pct(intervals(idle), 50),
    drag: { frames: drag.length - 1, interval_ms: stats(intervals(drag)), work_ms: stats(work) },
  };
  mkdirSync(`${ROOT}target/perf`, { recursive: true });
  writeFileSync(`${ROOT}target/perf/g10_1.json`, JSON.stringify(result, null, 2) + "\n");
  console.log(JSON.stringify(result, null, 2));
  expect(result.drag.work_ms.p95).toBeLessThanOrEqual(16);
});
