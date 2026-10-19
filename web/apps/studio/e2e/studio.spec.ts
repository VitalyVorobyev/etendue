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

test("the gizmo drags the selection live, keeps it selected, and commits one step", async ({ page }) => {
  await loaded(page);
  await page.getByRole("treeitem", { name: /board/ }).click();
  const world = page.getByTestId("world-pose").locator("summary");
  const before = await world.textContent();
  const bakes = () => page.evaluate(() => performance.getEntriesByName("etendue:bake").length);
  const baked = await bakes();

  const host = page.locator("[data-gizmo-at]");
  await expect(host).toHaveCount(1);
  const [gx, gy] = (await host.getAttribute("data-gizmo-at"))!.split(",").map(Number);
  const box = (await page.getByRole("group", { name: "Scene viewport" }).locator("canvas").boundingBox())!;
  // The centre handle moves in the view plane.
  await page.mouse.move(box.x + gx!, box.y + gy!);
  await page.mouse.down();
  for (let d = 5; d <= 40; d += 5) await page.mouse.move(box.x + gx! + d, box.y + gy! - d / 2);
  await expect(world).not.toHaveText(before!);
  expect(await bakes()).toBe(baked);
  await page.mouse.up();

  await expect(page.getByRole("button", { name: "Undo move board" })).toBeEnabled();
  expect(await bakes()).toBe(baked + 1);
  // The release was not a click on empty space: the board is still selected.
  await expect(page.getByRole("textbox", { name: "Id" })).toHaveValue("board");
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
  // It is made from the project's default camera: a physical IMX174 behind a 25 mm lens.
  await expect(page.getByTestId("inspector")).toContainText("1936×1216 px");
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

test("the Optics workspace designs a laser head: Scheimpflug, handles, optimiser", async ({ page }) => {
  await loaded(page);
  await page.getByRole("radio", { name: "Optics" }).click();
  const section = page.getByTestId("section-view");
  await expect(section).toBeVisible();
  const figures = page.getByTestId("optics-readouts");
  await expect(figures).toContainText("100 % seen");

  // The Scheimpflug setting puts the laser plane in focus: the window is left with diffraction.
  await page.getByRole("button", { name: "Scheimpflug" }).click();
  await expect(figures).toContainText("laser plane in focus");

  // Dragging the focus handle along the axis refocuses.
  const focus = page.getByTestId("focus").getByRole("spinbutton");
  const before = await focus.inputValue();
  const handle = section.locator('[data-handle="focus"]');
  const box = (await handle.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 60, box.y + box.height / 2, { steps: 5 });
  await page.mouse.up();
  await expect(focus).not.toHaveValue(before);
  await expect(figures).not.toContainText("laser plane in focus");

  // With the sensor kept square, the optimiser trades defocus against diffraction.
  await page.getByText("Tilt the sensor").click();
  await page.getByRole("button", { name: "Optimise" }).click();
  const optimum = page.getByTestId("optimum");
  await expect(optimum).toContainText("0.00°, 0.00°");
  await optimum.getByRole("button", { name: "Apply" }).click();
  await expect(page.getByRole("spinbutton", { name: "tilt x value" })).toHaveValue(/^-?0[.,]00$/);

  // Placed in the cell: a physical camera with the laser on its body, one undoable edit.
  await page.getByRole("button", { name: "Place in cell" }).click();
  await expect(page.getByRole("radio", { name: "Setup" })).toBeChecked();
  await expect(page.getByRole("treeitem", { name: /head_cam/ })).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /head_laser/ })).toBeVisible();
  await expect(page.getByRole("textbox", { name: "Id" })).toHaveValue("head_cam");
  await expect(page.getByRole("button", { name: "Undo place Laser head" })).toBeEnabled();
});

test("the Rigs workspace lays out a stereo pair and a ring of heads, and places them", async ({ page }) => {
  await loaded(page);
  await page.getByRole("radio", { name: "Rigs" }).click();
  await expect(page.getByTestId("rig-view")).toBeVisible();
  const figures = page.getByTestId("rig-readouts");
  await expect(figures).toContainText("2 cameras");
  const seen2 = page.getByTestId("rig-seen-2");
  await expect(seen2).not.toHaveText(/^0[.,]00 cm³$/);

  // A wider baseline, typed: the pair's geometry follows.
  const baseline = page.getByTestId("baseline").getByRole("spinbutton");
  await baseline.fill("200");
  await baseline.press("Enter");
  await expect(figures).toContainText("200 mm");

  // A ring of the Optics workspace's head: every camera has its laser, and the rig measures.
  await page.getByRole("combobox", { name: "unit" }).click();
  await page.getByRole("option", { name: /\(Optics\)/ }).click();
  await page.getByRole("radio", { name: "Ring" }).click();
  await expect(figures).toContainText("3 cameras · 3 lasers");
  await expect(page.getByTestId("rig-measured")).not.toHaveText(/^0[.,]00 cm³$/);

  // Placed in the cell: a rig frame with its expanded cameras and lasers, one undoable edit.
  await page.getByRole("button", { name: "Place in cell" }).click();
  await expect(page.getByRole("radio", { name: "Setup" })).toBeChecked();
  await expect(page.getByRole("treeitem", { name: /stereo_pair_cam2/ })).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /stereo_pair_laser2/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "Undo place Stereo pair" })).toBeEnabled();
});

test("the Motion workspace jogs a robot and teaches its pose", async ({ page }) => {
  await loaded(page);
  await page.getByRole("radio", { name: "Motion" }).click();
  const steps = page.getByTestId("step-list").locator(":scope > li");
  const before = await steps.count();
  await page.getByRole("radio", { name: "Jog" }).click();
  const tcp = page.getByTestId("jog-tcp");
  const start = await tcp.textContent();

  // A joint, typed: FK moves the TCP, without a bake.
  const j1 = page.getByTestId("joint-0").getByRole("spinbutton");
  await j1.fill("40");
  await j1.press("Enter");
  await expect(tcp).not.toHaveText(start!);
  const afterJoint = await tcp.textContent();

  // A Cartesian step along the tool's z: IK keeps the arm continuous.
  await page.getByRole("button", { name: "Jog +z" }).click();
  await expect(tcp).not.toHaveText(afterJoint!);
  await expect(page.getByText("Out of reach")).toHaveCount(0);

  // Taught: a PTP move to the pose and a capture, one undoable edit; the program grows by two.
  await page.getByRole("button", { name: "Add PTP + capture" }).click();
  await expect(page.getByRole("button", { name: "Undo teach a capture" })).toBeEnabled();
  await page.getByRole("radio", { name: "Program" }).click();
  await expect(steps).toHaveCount(before + 2);

  // And deleted again from the program.
  await steps.last().hover();
  await page.getByRole("button", { name: `Delete step ${before + 2}` }).click();
  await expect(steps).toHaveCount(before + 1);
});
