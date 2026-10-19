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

import { ROOT } from "./global-setup";

const DRAG_MS = 5000;

test("G8.1: frame times during a slider drag", async ({ page, browserName }) => {
  await page.addInitScript(() => {
    const w = window as unknown as { __frames: { t: number; work: number }[]; __record: boolean; __acc: number };
    w.__frames = [];
    w.__record = false;
    w.__acc = 0;
    const timed = <A extends unknown[], R>(fn: (this: unknown, ...a: A) => R) =>
      function (this: unknown, ...a: A): R {
        const s = performance.now();
        try {
          return fn.apply(this, a);
        } finally {
          w.__acc += performance.now() - s;
        }
      };
    const raf = window.requestAnimationFrame.bind(window);
    let frameStart = -1;
    window.requestAnimationFrame = (cb) =>
      raf((t) => {
        if (t !== frameStart) {
          if (frameStart >= 0 && w.__record) w.__frames.push({ t: frameStart, work: w.__acc });
          if (frameStart >= 0) w.__acc = 0;
          frameStart = t;
        }
        timed(cb)(t);
      });
    const wrapped = new WeakMap<object, EventListener>();
    const proto = EventTarget.prototype;
    const { addEventListener: add, removeEventListener: remove } = Object.getOwnPropertyDescriptors(proto);
    const addFn = add.value as EventTarget["addEventListener"];
    const removeFn = remove.value as EventTarget["removeEventListener"];
    proto.addEventListener = function (this: EventTarget, type, listener, options) {
      if (typeof listener === "function" && /^(pointer|mouse)/.test(type)) {
        let t = wrapped.get(listener);
        if (!t) wrapped.set(listener, (t = timed(listener)));
        return addFn.call(this, type, t, options);
      }
      return addFn.call(this, type, listener, options);
    };
    proto.removeEventListener = function (this: EventTarget, type, listener, options) {
      const t = typeof listener === "function" ? wrapped.get(listener) : undefined;
      return removeFn.call(this, type, t ?? listener, options);
    };
    const port = Object.getOwnPropertyDescriptor(MessagePort.prototype, "onmessage")!;
    Object.defineProperty(MessagePort.prototype, "onmessage", {
      ...port,
      set(fn: ((e: MessageEvent) => void) | null) {
        port.set!.call(this, fn ? timed(fn) : fn);
      },
    });
  });
  await page.goto("/?example=rig3_laser_ur5e");
  await expect(page.getByRole("treeitem", { name: /rig_cam0/ })).toBeVisible({ timeout: 30_000 });
  await page.waitForTimeout(1500); // meshes
  await page.getByRole("treeitem", { name: /^rig\b/ }).click();

  const record = async (ms: number, during?: () => Promise<void>) => {
    await page.evaluate(() => {
      const w = window as unknown as { __frames: unknown[]; __record: boolean };
      w.__frames = [];
      w.__record = true;
    });
    if (during) await during();
    else await page.waitForTimeout(ms);
    return page.evaluate(() => {
      const w = window as unknown as { __frames: { t: number; work: number }[]; __record: boolean };
      w.__record = false;
      return w.__frames;
    });
  };
  const pct = (xs: number[], p: number) => [...xs].sort((a, b) => a - b)[Math.ceil((p / 100) * xs.length) - 1]!;
  const stats = (xs: number[]) => ({ p50: pct(xs, 50), p95: pct(xs, 95), p99: pct(xs, 99), max: Math.max(...xs) });
  const intervals = (f: { t: number }[]) => f.slice(1).map((x, i) => x.t - f[i]!.t);

  const idle = await record(2000);
  const period = pct(intervals(idle), 50);

  const bakes = () => page.evaluate(() => performance.getEntriesByName("etendue:bake").length);
  const bakesBefore = await bakes();
  const thumb = page.getByTestId("pose-x").getByRole("slider");
  const box = (await thumb.boundingBox())!;
  const [x0, y0] = [box.x + box.width / 2, box.y + box.height / 2];
  let bakesDuring = -1;
  const drag = await record(DRAG_MS, async () => {
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
