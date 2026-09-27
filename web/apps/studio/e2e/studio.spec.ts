import { readFileSync } from "node:fs";

import { type Page, expect, test } from "@playwright/test";

import { BAKED, ROOT } from "./global-setup";

interface Baked {
  frames: string[];
  samples: { t: number; world_se3_frame: { translation: number[] }[]; capture?: { id: string } }[];
}

async function loaded(page: Page, camera = "cam_left") {
  await page.goto("/");
  await expect(page.getByRole("treeitem", { name: new RegExp(camera) })).toBeVisible({ timeout: 30_000 });
}

test("loads, bakes and shows the default example", async ({ page }) => {
  await loaded(page);
  const baked = JSON.parse(readFileSync(BAKED, "utf8")) as Baked;
  await expect(page.getByText(`0/${baked.samples.length - 1}`)).toBeVisible();
  const captures = baked.samples.filter((s) => s.capture).length;
  await expect(page.getByRole("button", { name: /capture/i })).toHaveCount(captures);
  await expect(page.getByRole("img", { name: "Scene viewport" }).locator("canvas")).toBeVisible();
});

test("a camera's world pose matches the CLI bake at a capture", async ({ page }) => {
  await loaded(page);
  const baked = JSON.parse(readFileSync(BAKED, "utf8")) as Baked;
  const k = baked.samples.flatMap((s, i) => (s.capture ? [i] : []))[3]!;
  const [x, y, z] = baked.samples[k]!.world_se3_frame[baked.frames.indexOf("cam_left")]!.translation.map(
    (m) => (m * 1000).toFixed(1),
  );

  await page.getByRole("button", { name: /capture/i }).nth(3).click();
  await page.getByRole("treeitem", { name: /cam_left/ }).click();
  const world = page.getByTestId("world-pose");
  await expect(world).toContainText(`t = ${baked.samples[k]!.t.toFixed(2)} s`);
  for (const v of [x, y, z]) await expect(world).toContainText(v!);

  // The board is in view at every capture of this scenario.
  await expect(page.getByTestId("camera-view-cam_left")).toHaveAttribute("data-visible", "1");
  // …and rendered through the camera's remap LUT, at the camera's resolution.
  const image = page.getByRole("img", { name: "cam_left rendered image" });
  await expect(image).toHaveAttribute("width", "1280");
  await expect(image).toHaveAttribute("height", "1024");
});

test("an invalid scene reports its issues and keeps the loaded one", async ({ page }) => {
  await loaded(page);
  const scene = JSON.parse(readFileSync(`${ROOT}examples/eye_in_hand_ur5e/scene.json`, "utf8")) as { version: number };
  scene.version = 2;
  await page.locator('input[type="file"]').first().setInputFiles({
    name: "scene.json",
    mimeType: "application/json",
    buffer: Buffer.from(JSON.stringify(scene)),
  });
  const inspector = page.getByTestId("inspector");
  await expect(inspector.getByText("The scene is invalid")).toBeVisible();
  await expect(inspector.getByText(/version: unsupported scene version 2/)).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible();
});

test("the eye-to-hand example loads from the picker", async ({ page }) => {
  await loaded(page);
  await page.getByRole("combobox", { name: "Example scene" }).click();
  await page.getByRole("option", { name: /eye_to_hand_ur5e/ }).click();
  await expect(page.getByText("examples/eye_to_hand_ur5e")).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /board/ })).toBeVisible();
});

test("dropped files find their robots in the library", async ({ page }) => {
  await loaded(page);
  await page.locator('input[type="file"]').first().setInputFiles([
    `${ROOT}web/apps/studio/e2e/fixtures/perf_scene.json`,
    `${ROOT}web/apps/studio/e2e/fixtures/perf_scenario.json`,
  ]);
  await expect(page.getByRole("treeitem", { name: /cam_bl/ })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("treeitem", { name: /^irb/ })).toBeVisible();
});
