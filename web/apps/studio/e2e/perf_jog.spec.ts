/**
 * P12: the P8 frame-time criteria hold while a robot is jogged. A joint slider of the Jog panel
 * is dragged for 5 s on `rig3_laser_ur5e` (a three-camera rig on the flange): FK and the pose
 * overrides run on every move, never a bake, and the main thread's JS work per frame has p95
 * ≤ 4 ms (G8.1's work limit). The frame interval is recorded beside the idle one; G8.1's own
 * interval criterion is pending a decision (docs/measurements/g8_1_drag.md).
 */

import { mkdirSync, writeFileSync } from "node:fs";

import { expect, test } from "@playwright/test";

import { installFrameProbe, intervals, pct, record, stats } from "./frames";
import { ROOT } from "./global-setup";

const DRAG_MS = 5000;

test("P12: frame times while jogging a joint", async ({ page }) => {
  await installFrameProbe(page);
  await page.goto("/?example=rig3_laser_ur5e");
  await expect(page.getByRole("treeitem", { name: /rig_cam0/ })).toBeVisible({ timeout: 30_000 });
  await page.waitForTimeout(1500); // meshes
  await page.getByRole("radio", { name: "Motion" }).click();
  await page.getByRole("radio", { name: "Jog" }).click();
  await expect(page.getByTestId("jog-panel")).toBeVisible();

  const idle = await record(page, 2000);
  const period = pct(intervals(idle), 50);
  const bakes = () => page.evaluate(() => performance.getEntriesByName("etendue:bake").length);
  const bakesBefore = await bakes();
  const thumb = page.getByTestId("joint-0").getByRole("slider");
  const box = (await thumb.boundingBox())!;
  const [x0, y0] = [box.x + box.width / 2, box.y + box.height / 2];
  const drag = await record(page, DRAG_MS, async () => {
    await page.mouse.move(x0, y0);
    await page.mouse.down();
    const start = Date.now();
    while (Date.now() - start < DRAG_MS) {
      const t = (Date.now() - start) / 1000;
      await page.mouse.move(x0 + 40 * Math.sin(2 * Math.PI * 0.5 * t), y0);
      await page.waitForTimeout(4);
    }
    await page.mouse.up();
  });

  const result = {
    gate: "P12 (G8.1 criteria while jogging)",
    userAgent: await page.evaluate(() => navigator.userAgent),
    display_period_ms: period,
    idle: { frames: idle.length - 1, interval_ms: stats(intervals(idle)), work_ms: stats(idle.map((f) => f.work)) },
    jog: {
      frames: drag.length - 1,
      bakes: (await bakes()) - bakesBefore,
      interval_ms: stats(intervals(drag)),
      work_ms: stats(drag.slice(1).map((f) => f.work)),
    },
  };
  mkdirSync(`${ROOT}target/perf`, { recursive: true });
  writeFileSync(`${ROOT}target/perf/p12_jog.json`, JSON.stringify(result, null, 2) + "\n");
  console.log(JSON.stringify(result, null, 2));
  expect(result.jog.bakes).toBe(0);
  expect(result.jog.work_ms.p95).toBeLessThanOrEqual(4);
});
