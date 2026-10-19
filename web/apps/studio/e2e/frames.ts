/**
 * Frame timing for the perf specs: an init script records, per animation frame, its start and
 * the page's JS work since the previous frame — animation callbacks, pointer and mouse
 * listeners, and message-port tasks (React's scheduler).
 */

import type { Page } from "@playwright/test";

export interface Frame {
  t: number;
  work: number;
}

export async function installFrameProbe(page: Page): Promise<void> {
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
    // A heartbeat, so every display frame is seen even where nothing animates (a 2-D workspace).
    const beat = () => window.requestAnimationFrame(beat);
    window.requestAnimationFrame(beat);
  });
}

/** Record frames while `during` runs (or for `ms`). */
export async function record(page: Page, ms: number, during?: () => Promise<void>): Promise<Frame[]> {
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
}

export const pct = (xs: number[], p: number) => [...xs].sort((a, b) => a - b)[Math.ceil((p / 100) * xs.length) - 1]!;
export const stats = (xs: number[]) => ({ p50: pct(xs, 50), p95: pct(xs, 95), p99: pct(xs, 99), max: Math.max(...xs) });
export const intervals = (f: { t: number }[]) => f.slice(1).map((x, i) => x.t - f[i]!.t);
