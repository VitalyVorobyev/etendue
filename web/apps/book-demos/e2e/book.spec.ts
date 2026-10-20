import { readFileSync, readdirSync } from "node:fs";
import { join, relative } from "node:path";
import { fileURLToPath } from "node:url";

import { expect, test } from "@playwright/test";

const BOOK = fileURLToPath(new URL("../../../../book/book/", import.meta.url));

/** Every widget name the bundle registers (src/main.ts). */
const NAMES = [
  ...readFileSync(new URL("../src/main.ts", import.meta.url), "utf8").matchAll(/^\s+([a-z]+): \(\) => import\(/gm),
].map((m) => m[1]!);

function* pages(dir: string): Generator<string> {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== "demos") yield* pages(path);
    } else if (entry.name.endsWith(".html") && entry.name !== "print.html") yield path;
  }
}

/** Book pages with widgets, and the widgets on each. */
const USES = [...pages(BOOK)]
  .map((path) => ({
    page: relative(BOOK, path),
    demos: [...readFileSync(path, "utf8").matchAll(/class="etendue-demo" data-demo="([a-z-]+)"/g)].map((m) => m[1]!),
  }))
  .filter((p) => p.demos.length > 0);

test("every widget is on some page", () => {
  const used = new Set(USES.flatMap((p) => p.demos));
  expect([...used].sort()).toEqual([...NAMES].sort());
});

for (const { page: path, demos } of USES) {
  test(`${path}: ${demos.join(", ")}`, async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (m) => {
      if (m.type() === "error") errors.push(m.text());
    });
    page.on("pageerror", (e) => errors.push(e.message));
    await page.goto(`/${path}`);
    const widgets = page.locator(".etendue-demo");
    await expect(widgets).toHaveCount(demos.length);
    for (let i = 0; i < demos.length; i++) {
      const w = widgets.nth(i);
      await expect(w).toHaveAttribute("data-state", "ready", { timeout: 30_000 });
      await expect(w.locator(".ed-error")).toHaveText("");
      // Move the first slider to its maximum: the widget redraws without an error.
      const slider = w.locator('input[type="range"]').first();
      await slider.evaluate((input: HTMLInputElement) => {
        input.value = input.max;
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
      await page.waitForTimeout(300);
      await expect(w.locator(".ed-error")).toHaveText("");
    }
    expect(errors).toEqual([]);
  });
}
