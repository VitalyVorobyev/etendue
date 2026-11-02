/**
 * Gate G15.1: the cell builder recreates `examples/eye_in_hand_ur5e` and
 * `examples/eye_to_hand_ur5e` from library items. The test drives the builder, downloads the
 * scene it made and compares it with the example's: every number within 1e-12, ids, parents and
 * every other value identical (`close.ts`).
 */

import { readFileSync } from "node:fs";

import { type Download, type Page, expect, test } from "@playwright/test";

import { differences, largestDeviation } from "./close";
import { ROOT } from "./global-setup";

const example = (cell: string): { description: string } =>
  JSON.parse(readFileSync(`${ROOT}examples/${cell}/scene.json`, "utf8")) as { description: string };

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

async function opened(page: Page) {
  await page.goto("/?example=eye_in_hand_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "New cell…" }).click();
  await expect(page.getByTestId("cell-builder")).toBeVisible();
}

/** Pick `option` (by its name, which an option's note follows) in the picker `label`. */
async function choose(page: Page, label: string, option: string) {
  await page.getByRole("combobox", { name: label, exact: true }).click();
  await page.getByRole("option", { name: new RegExp(`^${escape(option)}`) }).click();
}

/** Type a pose: the translation in mm and the angles (roll, pitch, yaw) in degrees. */
async function pose(page: Page, label: string, mm: number[], deg: number[] = [0, 0, 0]) {
  const group = page.getByRole("group", { name: label, exact: true });
  for (const [i, axis] of ["x", "y", "z"].entries()) {
    await group.getByRole("spinbutton", { name: `Translation ${axis}` }).fill(String(mm[i]));
  }
  for (const [i, angle] of ["roll", "pitch", "yaw"].entries()) {
    await group.getByRole("spinbutton", { name: `Rotation ${angle}` }).fill(String(deg[i]));
  }
}

const next = (page: Page) => page.getByRole("button", { name: "Next", exact: true }).click();

/** The stereo pair of both examples: the parallel design, its cameras named, the right one its own unit. */
async function stereoRig(page: Page) {
  await choose(page, "Optics", "Python 1300, 2318.8 px pinhole (the examples' camera)");
  await next(page);
  await page.getByRole("radio", { name: "Rig design" }).click();
  await choose(page, "Rig design", "Parallel stereo pair, 80 mm baseline");
  await page.getByRole("textbox", { name: "Rig id" }).fill("rig");
  await page.getByRole("textbox", { name: "Camera 1 id" }).fill("cam_left");
  await page.getByRole("textbox", { name: "Camera 2 id" }).fill("cam_right");
  await choose(page, "Camera 2", "Python 1300, 2318.8 px pinhole (the examples' right camera)");
  await next(page);
}

/** The target and the UR5e at the examples' start pose; then the description, and Build. */
async function targetRobotAndBuild(page: Page, description: string, eyeInHand: boolean) {
  await choose(page, "Target", "Chessboard 9 × 6, 25 mm squares");
  await page.getByRole("textbox", { name: "Target id" }).fill("board");
  if (eyeInHand) await pose(page, "Target on the world", [450, 150, 0]);
  // The size check: a 225 mm board against the image width at the design's 500 mm.
  await expect(page.getByTestId("target-coverage")).toContainText("of the image width at 500 mm");
  await next(page);

  await choose(page, "Robot", "Universal Robots UR5e");
  await page.getByRole("textbox", { name: "Robot id" }).fill("ur5e");
  await pose(page, "Base on the world", [0, 0, 0]);
  const joints = page.getByTestId("start-joints");
  await expect(joints.getByRole("spinbutton")).toHaveCount(6);
  for (const [name, deg] of [
    ["shoulder_pan_joint", 0],
    ["shoulder_lift_joint", -90],
    ["elbow_joint", 90],
    ["wrist_1_joint", -90],
    ["wrist_2_joint", -90],
    ["wrist_3_joint", 0],
  ] as const) {
    await joints.getByRole("spinbutton", { name, exact: true }).fill(String(deg));
  }
  // The kernel's view of the cell at the start pose, per camera.
  await expect(page.getByTestId("cell-views").getByRole("listitem")).toHaveCount(2);
  await next(page);

  await page.getByRole("textbox", { name: "Description" }).fill(description);
  await expect(page.getByTestId("cell-problems")).toHaveCount(0);
  await page.getByRole("button", { name: "Build cell" }).click();
  await expect(page.getByRole("button", { name: "Undo build a cell" })).toBeEnabled({ timeout: 30_000 });
  await expect(page.getByTestId("cell-builder")).toHaveCount(0);
}

/** The open scene, as File ▸ Download scene writes it. */
async function downloadedScene(page: Page): Promise<unknown> {
  const downloads: Download[] = [];
  page.on("download", (d) => downloads.push(d));
  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "Download scene" }).click();
  await expect.poll(() => downloads.map((d) => d.suggestedFilename()).sort()).toEqual(["scenario.json", "scene.json"]);
  const scene = downloads.find((d) => d.suggestedFilename() === "scene.json")!;
  return JSON.parse(readFileSync(await scene.path(), "utf8")) as unknown;
}

test("G15.1: the builder recreates the eye-in-hand example from library items", async ({ page }) => {
  const expected = example("eye_in_hand_ur5e");
  await opened(page);
  await stereoRig(page);
  await page.getByRole("radio", { name: "Eye-in-hand" }).click();
  await choose(page, "Mount (rig on the flange)", "Flange plate, 30 mm");
  await next(page);
  await targetRobotAndBuild(page, expected.description, true);
  const scene = await downloadedScene(page);
  expect(differences(scene, expected)).toEqual([]);
  console.log(`eye_in_hand_ur5e: largest |Δ| ${largestDeviation(scene, expected)}`);
});

test("G15.1: the builder recreates the eye-to-hand example from library items", async ({ page }) => {
  const expected = example("eye_to_hand_ur5e");
  await opened(page);
  await stereoRig(page);
  await page.getByRole("radio", { name: "Eye-to-hand" }).click();
  await choose(page, "Mount (target on the flange)", "Board holder, 10 mm");
  await page.getByRole("textbox", { name: "Stand id" }).fill("stand");
  await pose(page, "Stand on the world", [450, -750, 0]);
  // The rig 550 mm up the stand, turned down towards the robot.
  await pose(page, "Rig on the stand", [0, 0, 550], [-105.10109816138544, 0, 0]);
  await next(page);
  await targetRobotAndBuild(page, expected.description, false);
  const scene = await downloadedScene(page);
  expect(differences(scene, expected)).toEqual([]);
  console.log(`eye_to_hand_ur5e: largest |Δ| ${largestDeviation(scene, expected)}`);
});

test("the builder shows the cell as it goes, and Cancel puts back the open one", async ({ page }) => {
  await opened(page);
  // The first draft is already a cell: one camera on a rig, in the 3D view.
  await expect(page.getByTestId("cell-steps")).toBeVisible();
  await page.getByRole("button", { name: "Next", exact: true }).click();
  await page.getByRole("textbox", { name: "Camera id" }).fill("cam_new");
  await page.getByTestId("cell-steps").getByRole("button", { name: /Robot/ }).click();
  await expect(page.getByTestId("cell-views")).toContainText("cam_new");
  // Joints that hold the rig over the target, by the kernel's IK: then the camera sees all of it.
  await page.getByRole("button", { name: "Aim at the target" }).click();
  await expect(page.getByTestId("cell-views").getByRole("listitem")).toContainText(/sees (\d+) of \1$/);
  await page.getByRole("button", { name: "Cancel" }).click();
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible();
  await expect(page.getByRole("treeitem", { name: /cam_new/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /^Undo/ })).toBeDisabled();
});
