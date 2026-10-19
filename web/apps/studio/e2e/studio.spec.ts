import { readFileSync } from "node:fs";

import { type Page, expect, test } from "@playwright/test";

import { BAKED, ROOT } from "./global-setup";

interface Baked {
  frames: string[];
  samples: { t: number; world_se3_frame: { translation: number[] }[]; capture?: { id: string } }[];
}

async function loaded(page: Page, camera = "cam_left") {
  await page.goto("/?example=eye_in_hand_ur5e");
  await expect(page.getByRole("treeitem", { name: new RegExp(camera) })).toBeVisible({ timeout: 30_000 });
}

test("loads, bakes and shows the default example", async ({ page }) => {
  await loaded(page);
  const baked = JSON.parse(readFileSync(BAKED, "utf8")) as Baked;
  await expect(page.getByText(`0/${baked.samples.length - 1}`)).toBeVisible();
  const captures = baked.samples.filter((s) => s.capture).length;
  await expect(page.getByRole("button", { name: /capture/i })).toHaveCount(captures);
  await expect(page.getByRole("group", { name: "Scene viewport" }).locator("canvas")).toBeVisible();
});

test("a camera's world pose matches the CLI bake at a capture", async ({ page }) => {
  await loaded(page);
  const baked = JSON.parse(readFileSync(BAKED, "utf8")) as Baked;
  const k = baked.samples.flatMap((s, i) => (s.capture ? [i] : []))[3]!;
  const [x, y, z] = baked.samples[k]!.world_se3_frame[baked.frames.indexOf("cam_left")]!.translation.map((m) =>
    (m * 1000).toFixed(1),
  );

  await page
    .getByRole("button", { name: /capture/i })
    .nth(3)
    .click();
  await page.getByRole("treeitem", { name: /cam_left/ }).click();
  const world = page.getByTestId("world-pose");
  await expect(world).toContainText(`t = ${baked.samples[k]!.t.toFixed(2)} s`);
  for (const v of [x, y, z]) await expect(world).toContainText(v!);

  // The Images workspace shows what every camera sees.
  await page.getByRole("radio", { name: "Images" }).click();
  // The board is in view at every capture of this scenario.
  await expect(page.getByTestId("camera-view-cam_left")).toHaveAttribute("data-visible", "1");
  // …and rendered through the camera's remap LUT, at the camera's resolution.
  const image = page.getByRole("img", { name: "cam_left rendered image" });
  await expect(image).toHaveAttribute("width", "1280");
  await expect(image).toHaveAttribute("height", "1024");
});

test("a pose slider moves the camera live, bakes once on release, and undoes as one step", async ({ page }) => {
  const meshes: string[] = [];
  page.on("request", (r) => {
    if (r.url().endsWith(".glb")) meshes.push(r.url());
  });
  await loaded(page);
  // The robot's meshes load once, after the scene appears.
  await expect.poll(() => meshes.length, { timeout: 30_000 }).toBeGreaterThan(0);
  await page.waitForLoadState("networkidle");
  const loadedMeshes = meshes.length;
  await page.getByRole("treeitem", { name: /cam_left/ }).click();
  const world = page.getByTestId("world-pose").locator("summary");
  const before = await world.textContent();
  const bakes = () => page.evaluate(() => performance.getEntriesByName("etendue:bake").length);
  const baked = await bakes();

  const thumb = page.getByTestId("pose-x").getByRole("slider");
  const box = (await thumb.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  for (let dx = 4; dx <= 40; dx += 4) await page.mouse.move(box.x + box.width / 2 + dx, box.y + box.height / 2);
  // While dragging: the camera has moved in the world, and nothing was baked or recorded.
  await expect(world).not.toHaveText(before!);
  expect(await bakes()).toBe(baked);
  await expect(page.getByRole("button", { name: "Undo move cam_left" })).toHaveCount(0);
  await page.mouse.up();

  // On release: one bake, one undo step, no mesh fetched again.
  await expect(page.getByRole("button", { name: "Undo move cam_left" })).toBeEnabled();
  expect(await bakes()).toBe(baked + 1);
  expect(meshes.length).toBe(loadedMeshes);
  const moved = await world.textContent();
  await page.getByRole("button", { name: "Undo move cam_left" }).click();
  await expect(world).toHaveText(before!);
  await page.getByRole("button", { name: "Redo move cam_left" }).click();
  await expect(world).toHaveText(moved!);
});

test("an invalid scene reports its issues and keeps the loaded one", async ({ page }) => {
  await loaded(page);
  const scene = JSON.parse(readFileSync(`${ROOT}examples/eye_in_hand_ur5e/scene.json`, "utf8")) as { version: number };
  scene.version = 2;
  await page.getByTestId("open-files").setInputFiles({
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
  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "eye_to_hand_ur5e" }).click();
  await expect(page.getByText("examples/eye_to_hand_ur5e")).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /board/ })).toBeVisible();
});

test("dropped files find their robots in the library", async ({ page }) => {
  await loaded(page);
  await page
    .getByTestId("open-files")
    .setInputFiles([
      `${ROOT}web/apps/studio/e2e/fixtures/perf_scene.json`,
      `${ROOT}web/apps/studio/e2e/fixtures/perf_scenario.json`,
    ]);
  await expect(page.getByRole("treeitem", { name: /cam_bl/ })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("treeitem", { name: /^irb/ })).toBeVisible();
});

test("an edit rebakes in place and undoes", async ({ page }) => {
  await loaded(page);
  await page
    .getByRole("button", { name: /capture/i })
    .nth(2)
    .click();
  const position = page.getByText(/^\d+\/\d+$/);
  const before = await position.textContent();
  await page.getByRole("treeitem", { name: /cam_left/ }).click();
  const width = page.getByRole("spinbutton", { name: "width" });
  await width.fill("640");
  await width.press("Enter");
  await expect(page.getByTestId("dirty")).toBeVisible();
  // The playhead stays where it was: an edit is not a new document.
  await expect(position).toHaveText(before!);
  await page.getByRole("radio", { name: "Images" }).click();
  await expect(page.getByRole("img", { name: "cam_left rendered image" })).toHaveAttribute("width", "640");

  await page.getByRole("button", { name: "Undo edit cam_left" }).click();
  await expect(page.getByRole("img", { name: "cam_left rendered image" })).toHaveAttribute("width", "1280");
  await expect(page.getByTestId("dirty")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Redo edit cam_left" })).toBeEnabled();
});

test("entities are added, refused when invalid, and removed", async ({ page }) => {
  await loaded(page);
  await page.getByRole("treeitem", { name: /^rig/ }).click();
  await page.getByRole("button", { name: "Add to the scene" }).click();
  await page.getByRole("menuitem", { name: "Camera" }).click();
  await expect(page.getByRole("treeitem", { name: /^cam\b/ })).toBeVisible();
  // Renaming onto a taken id is refused; the field keeps the text.
  const id = page.getByRole("textbox", { name: "Id" });
  await id.fill("cam_left");
  await id.press("Enter");
  await expect(page.getByRole("treeitem", { name: /^cam\b/ })).toBeVisible();
  await id.fill("cam_top");
  await id.press("Enter");
  await expect(page.getByRole("treeitem", { name: /cam_top/ })).toBeVisible();
  await page.getByRole("button", { name: "Remove cam_top" }).click();
  await expect(page.getByRole("treeitem", { name: /cam_top/ })).toHaveCount(0);
});

test("the scene downloads in a browser", async ({ page }) => {
  await loaded(page);
  await page.getByRole("button", { name: "File" }).click();
  const downloads: string[] = [];
  page.on("download", (d) => downloads.push(d.suggestedFilename()));
  await page.getByRole("menuitem", { name: "Download scene" }).click();
  await expect.poll(() => downloads).toEqual(["scene.json", "scenario.json"]);
});

test("the Motion workspace lists the program and seeks to captures", async ({ page }) => {
  await loaded(page);
  await page.getByRole("radio", { name: "Motion" }).click();
  const steps = page.getByTestId("step-list");
  await expect(steps.getByRole("listitem")).toHaveCount(20);
  const baked = JSON.parse(readFileSync(BAKED, "utf8")) as Baked;
  const k = baked.samples.flatMap((s, i) => (s.capture ? [i] : []))[4]!;
  await steps.getByRole("button", { name: /cap_004/ }).click();
  await expect(page.getByText(`${k}/${baked.samples.length - 1}`)).toBeVisible();
  await expect(page.getByRole("img", { name: /Joint positions/ }).or(page.getByLabel(/Joint positions/))).toBeVisible();

  // The motion report: the program's duration, and per step the limit that sets it.
  await expect(page.getByTestId("motion-summary")).toContainText("10 captures");
  await expect(steps.getByText(/\d+\.\d\d s · \w+ (vel|acc|jer) \d+%/).first()).toBeVisible();
  await page.getByRole("radio", { name: "Velocity" }).click();
  await expect(page.getByLabel(/Joint velocities/)).toBeVisible();

  // Pinning the IK solutions is one undoable edit.
  await page.getByRole("button", { name: "Resolve IK", exact: true }).click();
  await expect(steps.getByLabel("IK pinned").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Resolve IK", exact: true })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Undo resolve IK" })).toBeEnabled();

  // The capture poses download as CSV.
  const download = page.waitForEvent("download");
  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "Captures of ur5e (CSV)…" }).click();
  const csv = await (await download).path().then((p) => readFileSync(p, "utf8"));
  const lines = csv.trim().split("\n");
  expect(lines[0]).toMatch(/^capture,t,x,y,z,qx,qy,qz,qw,rx,ry,rz,shoulder_pan_joint/);
  expect(lines).toHaveLength(11);
});

test("a library robot is added and removed as one edit", async ({ page }) => {
  await loaded(page);
  await page.getByRole("treeitem", { name: /^world/ }).click();
  await page.getByRole("button", { name: "Add to the scene" }).click();
  // Only redistributable robots are built into the studio.
  await expect(page.getByRole("menuitem", { name: /local only/ })).toHaveCount(0);
  await page.getByRole("menuitem", { name: "igus ReBeL 6DOF" }).click();
  await expect(page.getByRole("treeitem", { name: /^igus_rebel_6dof/ })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId("inspector")).toContainText("igus ReBeL 6DOF");
  await page.getByRole("button", { name: "Undo add igus_rebel_6dof" }).click();
  await expect(page.getByRole("treeitem", { name: /^igus_rebel_6dof/ })).toHaveCount(0);
  await page.getByRole("button", { name: "Redo add igus_rebel_6dof" }).click();
  await page.getByRole("treeitem", { name: /^igus_rebel_6dof/ }).click();
  await page.getByRole("button", { name: "Remove igus_rebel_6dof" }).click();
  await expect(page.getByRole("treeitem", { name: /^igus_rebel_6dof/ })).toHaveCount(0);
});

test("the rig builder lays out a six-camera ring with a laser per camera", async ({ page }) => {
  await loaded(page);
  await page.getByRole("treeitem", { name: /^rig/ }).click();
  await page.getByRole("button", { name: "Lay out cameras" }).click();
  // The rig's two cameras become a layout of two.
  await expect(page.getByRole("treeitem", { name: /rig_cam1/ })).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toHaveCount(0);
  await page.getByRole("radio", { name: "6" }).click();
  await expect(page.getByRole("treeitem", { name: /rig_cam5/ })).toBeVisible();
  await page.getByRole("switch", { name: /One laser per camera/ }).click();
  await expect(page.getByRole("treeitem", { name: /rig_laser5/ })).toBeVisible();
  await page.getByRole("radio", { name: "Images" }).click();
  await expect(page.getByTestId("camera-grid").getByRole("region")).toHaveCount(6);
  // Each change is its own undo step.
  await page.getByRole("button", { name: "Undo set rig lasers" }).click();
  await expect(page.getByRole("treeitem", { name: /rig_laser5/ })).toHaveCount(0);
  await expect(page.getByTestId("camera-grid").getByRole("region")).toHaveCount(6);
  await expect(page.getByRole("button", { name: "Undo set rig count, radius m" })).toBeEnabled();
});

test("a target becomes a PuzzlePole, drawn and seen as a cylinder", async ({ page }) => {
  await loaded(page);
  await page.getByRole("treeitem", { name: /^board/ }).click();
  await page.getByRole("combobox", { name: "Target kind" }).click();
  await page.getByRole("option", { name: "PuzzleBoard" }).click();
  await expect(page.getByRole("spinbutton", { name: "origin row" })).toBeVisible();
  await page.getByRole("combobox", { name: "Target kind" }).click();
  await page.getByRole("option", { name: /PuzzlePole/ }).click();
  await expect(page.getByTestId("inspector")).toContainText("⌀");
  await page.getByRole("combobox", { name: "Squares around" }).click();
  await page
    .getByRole("option", { name: /^18 squares/ })
    .first()
    .click();
  await expect(page.getByTestId("inspector")).toContainText("⌀ 143.2 mm");
  await page.getByRole("button", { name: "Undo set board circumference squares, start row" }).click();
  await expect(page.getByTestId("inspector")).toContainText("⌀ 95.5 mm");
});

test("the sphere generator previews candidates and applies the kept ones", async ({ page }) => {
  await page.goto("/?example=closed_loop_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("radio", { name: "Motion" }).click();
  await page.getByRole("radio", { name: "Generate poses" }).click();
  const summary = page.getByTestId("generator-summary");
  await expect(summary).toContainText("48 candidates");
  const apply = page.getByRole("button", { name: /^Apply \d+ poses$/ });
  await expect(apply).toBeEnabled();
  const kept = Number((await apply.textContent())!.match(/\d+/)![0]);
  expect(kept).toBeGreaterThan(10);
  // The example's own spec (2 radii × 2 θ × 6 φ × 2 rolls); fewer azimuths, fewer candidates.
  const phiSteps = page.getByRole("spinbutton", { name: "φ steps" });
  await phiSteps.fill("4");
  await phiSteps.press("Enter");
  await expect(summary).toContainText("32 candidates");
  const applied = Number((await apply.textContent())!.match(/\d+/)![0]);
  await apply.click();
  // The program is the kept poses: a pinned PTP and a capture each.
  await expect(page.getByTestId("step-list").getByRole("listitem")).toHaveCount(2 * applied);
  await expect(page.getByTestId("step-list").getByLabel("IK pinned").first()).toBeVisible();
  await expect(page.getByRole("button", { name: "Undo generate poses" })).toBeEnabled();
});

test("each camera of a laser ring shows its own laser's stripe", async ({ page }) => {
  await page.goto("/?example=rig3_laser_ur5e");
  await expect(page.getByRole("treeitem", { name: /rig_cam0/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("radio", { name: "Images" }).click();
  for (const camera of ["rig_cam0", "rig_cam1", "rig_cam2"]) {
    // One stripe: its own laser's, not the other two.
    await expect(page.getByTestId(`camera-view-${camera}`)).toHaveAttribute("data-stripes", "1");
  }
});
