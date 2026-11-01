/**
 * The studio in its Tauri shell, against a mocked shell: `window.__TAURI_INTERNALS__` is
 * injected before the app loads, so `invoke` reaches canned commands instead of Rust. The
 * "filesystem" is the preview server: the mocked `data_root` is `/repo`, and files under it are
 * fetched from the served repository, so absolute paths and URLs coincide.
 *
 * The native pipeline itself is tested in Rust (`src-tauri`, `cargo test`); this checks the
 * wiring: what the Dataset panel sends, and how it shows progress and results.
 */

import { readFileSync } from "node:fs";

import { type Page, expect, test } from "@playwright/test";

import { CALIBRATION, ROOT } from "./global-setup";

async function installShell(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (data: unknown) => void>();
    let next = 1;
    const w = window as unknown as Record<string, unknown>;
    w.__requests = [];
    w.__calibrations = [];
    w.__cancelled = false;
    // A project folder the user "picks", and the user's bank, kept in sessionStorage so they
    // survive a reload.
    const PROJECT = "/proj";
    const BANK = "/bank";
    const disk = (): Record<string, string> =>
      JSON.parse(sessionStorage.getItem("fake-disk") ?? "{}") as Record<string, string>;
    const store = (files: Record<string, string>) => sessionStorage.setItem("fake-disk", JSON.stringify(files));
    const inProject = (path: string) => [PROJECT, BANK].some((dir) => path === dir || path.startsWith(`${dir}/`));
    w.__TAURI_INTERNALS__ = {
      transformCallback: (cb: (data: unknown) => void) => {
        const id = next++;
        callbacks.set(id, cb);
        return id;
      },
      unregisterCallback: (id: number) => callbacks.delete(id),
      convertFileSrc: (path: string) => path,
      invoke: async (cmd: string, args: Record<string, unknown>) => {
        switch (cmd) {
          case "data_root":
            return { root: "/repo", datasets: "/repo/target/studio" };
          case "pick":
            return args.directory ? [PROJECT] : [];
          case "write_text": {
            const files = disk();
            files[args.path as string] = args.text as string;
            store(files);
            return null;
          }
          case "remove_file": {
            const files = disk();
            const path = args.path as string;
            if (!inProject(path)) throw new Error(`${path}: not in a folder chosen in the studio`);
            if (!(path in files)) throw new Error(`${path}: not a file`);
            delete files[path];
            store(files);
            return null;
          }
          case "open_bank": {
            const files = disk();
            const path = `${BANK}/etendue-bank.json`;
            files[path] ??= `${JSON.stringify({ version: 1, name: "My bank" }, null, 2)}\n`;
            store(files);
            return [BANK, JSON.parse(files[path]) as unknown];
          }
          case "open_project": {
            const text = disk()[`${args.dir as string}/etendue-project.json`];
            if (text === undefined) throw new Error(`${args.dir as string}: no etendue-project.json`);
            return JSON.parse(text) as unknown;
          }
          case "path_exists":
            if (inProject(args.path as string)) return (args.path as string) in disk();
            return (await fetch(args.path as string, { method: "HEAD" })).ok;
          case "read_text": {
            if (inProject(args.path as string)) {
              const text = disk()[args.path as string];
              if (text === undefined) throw new Error(`${args.path as string}: not found`);
              return text;
            }
            const res = await fetch(args.path as string);
            if (!res.ok) throw new Error(`${args.path as string}: not found`);
            return res.text();
          }
          case "list_dir": {
            if (inProject(args.path as string)) {
              const dir = `${args.path as string}/`;
              return Object.keys(disk())
                .filter((p) => p.startsWith(dir) && !p.slice(dir.length).includes("/"))
                .map((p) => p.slice(dir.length))
                .sort();
            }
            const res = await fetch(`${args.path as string}/.index.json`);
            return res.ok ? ((await res.json()) as string[]) : [];
          }
          case "blender_status":
            return { found: true, exe: "/Applications/Blender.app", version: "5.2.2", pin: "5.2", message: null };
          case "cancel_run":
            w.__cancelled = true;
            return true;
          case "calibrate_dataset": {
            (w.__calibrations as unknown[]).push({ dir: args.dir, options: args.options, keep: args.keep });
            const channel = (args.onProgress as { id: number }).id;
            callbacks.get(channel)?.({ index: 0, message: { kind: "step", stage: "calibrate", done: 0, total: 1 } });
            // The native report on closed_loop_ur5e's exact observations (global setup).
            return { kind: "ok", ...(w.__calibrationRun as object) };
          }
          case "calibration_runs": {
            // Every run calibrated so far, kept where the studio asked.
            const kept = (w.__calibrations as { dir: string; options: object; keep: string }[]).filter((c) =>
              c.keep.startsWith(`${args.folder as string}/`),
            );
            return kept
              .map((c) => ({
                name: c.keep.split("/").at(-1),
                config: { version: 1, dataset: c.dir, options: c.options },
                run: w.__calibrationRun,
              }))
              .sort((a, b) => (a.name! < b.name! ? -1 : 1));
          }
          case "generate_dataset": {
            (w.__requests as unknown[]).push(args.request);
            const channel = (args.onProgress as { id: number }).id;
            const send = (index: number, message: unknown) => callbacks.get(channel)?.({ index, message });
            send(0, { kind: "step", stage: "render", done: 1, total: 2 });
            send(1, { kind: "log", line: "etendue: rendered exr/cam_left/cap_000.exr" });
            // The run lasts until the test lets it finish (`__finishRun`), not for a set time.
            await new Promise((resolve) => {
              w.__finishRun = resolve;
            });
            const request = args.request as { output: string };
            return {
              kind: "ok",
              output: request.output,
              gt: { captures: 20, views: 40, visible: 2160, points: 54 },
              render: { images: 40, cameras: ["cam_left", "cam_right"] },
              detection: [
                {
                  camera: "cam_left",
                  views: 20,
                  ok: 20,
                  no_board: 0,
                  partial: 0,
                  ambiguous: 0,
                  undecoded: 0,
                  points: 1080,
                  mislabelled: 0,
                  unmatched: 0,
                  off_board: 0,
                  rms_px: 0.0505,
                  max_px: 0.1417,
                },
              ],
            };
          }
          default:
            return null;
        }
      },
    };
  });
}

test("generates a dataset through the native pipeline", async ({ page }) => {
  await installShell(page);
  const run = JSON.parse(readFileSync(`${ROOT}${CALIBRATION}`, "utf8")) as object;
  await page.addInitScript((r) => {
    (window as unknown as { __calibrationRun: object }).__calibrationRun = r;
  }, run);
  await page.goto("/?example=closed_loop_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "File" }).click();
  await expect(page.getByRole("menuitem", { name: "Open scene…" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Import tool poses…" })).toBeEnabled();
  await page.keyboard.press("Escape");

  await page.getByRole("radio", { name: "Calibrate" }).click();
  const panel = page.getByTestId("dataset-panel");
  await expect(panel.getByTestId("blender-status")).toContainText("Blender 5.2.2");
  await expect(panel.getByTestId("blender-status")).not.toContainText("pinned");
  // Examples of the checkout write into target/, never into examples/.
  await expect(panel.getByRole("textbox", { name: "Output folder" })).toHaveValue(
    "/repo/target/studio/closed_loop_ur5e",
  );
  await expect(panel.getByRole("textbox", { name: /Sensor model/ })).toHaveValue(
    "/repo/examples/closed_loop_ur5e/sensor_linear.json",
  );

  await panel.getByRole("button", { name: "Generate" }).click();
  await expect(panel.getByRole("progressbar", { name: "Dataset generation" })).toBeVisible();
  await expect(panel.getByText("Rendering: 1 of 2")).toBeVisible();
  await expect(panel.getByTestId("dataset-log")).toContainText("etendue: rendered exr/cam_left/cap_000.exr");
  await expect(panel.getByRole("button", { name: "Cancel" })).toBeVisible();
  // The run outlives the panel: away and back, it still shows, and cannot start twice.
  await page.getByRole("radio", { name: "Images" }).click();
  await page.getByRole("radio", { name: "Calibrate" }).click();
  await expect(page.getByTestId("dataset-panel").getByRole("button", { name: "Cancel" })).toBeVisible();
  await expect(page.getByTestId("dataset-panel").getByRole("button", { name: "Generate" })).toBeDisabled();
  await page.evaluate(() => (window as unknown as { __finishRun: () => void }).__finishRun());

  const result = panel.getByTestId("dataset-result");
  await expect(result).toContainText("20 captures, 40 views, 2160 visible points");
  await expect(result.getByRole("cell", { name: "0.0505 px" })).toBeVisible();

  const requests = await page.evaluate(
    () => (window as unknown as { __requests: Record<string, unknown>[] }).__requests,
  );
  expect(requests).toHaveLength(1);
  const request = requests[0]!;
  expect(request).toMatchObject({
    origin: "/repo/examples/closed_loop_ur5e/scene.json",
    output: "/repo/target/studio/closed_loop_ur5e",
    render: true,
    samples: 16,
    supersample: 4,
    sensor: "/repo/examples/closed_loop_ur5e/sensor_linear.json",
    cameras: [],
  });
  expect((request.scenario as { steps: unknown[] }).steps).toHaveLength(48);
  // The scene goes as the studio holds it (edits included), not as a path.
  expect((request.scene as { cameras: unknown[] }).cameras).toHaveLength(2);

  // Calibrated natively (the shell's answer is `etendue calibrate`'s real report): the rows
  // against the gate, the residuals and their histogram.
  const calibrate = page.getByTestId("calibrate-panel");
  await calibrate.getByRole("radio", { name: "With noise" }).click();
  await expect(calibrate.getByTestId("noise-form")).toBeVisible();
  await calibrate.getByRole("radio", { name: "Exact" }).click();
  await calibrate.getByRole("button", { name: "Calibrate" }).click();
  const outcome = page.getByTestId("calibration-result");
  await expect(outcome).toContainText("Closed-loop check: PASS");
  await expect(outcome).toContainText("rig hand-eye");
  await expect(outcome.getByRole("cell", { name: "hand-eye rotation deg" })).toBeVisible();
  await expect(outcome.getByRole("img", { name: "Reprojection errors" })).toBeVisible();
  const calibrations = await page.evaluate(
    () => (window as unknown as { __calibrations: Record<string, unknown>[] }).__calibrations,
  );
  expect(calibrations).toEqual([
    {
      dir: "/repo/target/studio/closed_loop_ur5e",
      options: {},
      keep: expect.stringMatching(
        /^\/repo\/target\/studio\/closed_loop_ur5e\/runs\/closed_loop_ur5e-\d{8}-\d{6}-exact$/,
      ),
    },
  ]);
  // The run is kept beside the dataset (no project is open for writing) and listed.
  const kept = calibrate.getByTestId("kept-runs");
  await expect(kept.getByRole("row")).toHaveCount(2);
  await expect(kept).toContainText("exact");
  await expect(kept).toContainText("shown");

  // The 3-D view draws the estimate (both cameras and the board) over the truth, exaggerated.
  await expect(page.getByTestId("viewport")).toHaveAttribute("data-estimate", "2");
  const scale = outcome.getByTestId("estimate-scale");
  await expect(scale.getByRole("radio", { name: "×100", exact: true })).toBeChecked();
  await scale.getByRole("radio", { name: "×1000" }).click();
  await expect(scale.getByRole("radio", { name: "×1000" })).toBeChecked();

  // At a capture, the camera view shows what the calibration observes: every corner exactly…
  await page
    .getByRole("button", { name: /capture/i })
    .nth(2)
    .click();
  const view = page.getByTestId("observations").getByTestId("camera-view-cam_left");
  await expect(view).toHaveAttribute("data-observed", /^[1-9]\d*$/);
  const exact = Number(await view.getAttribute("data-observed"));
  // …or, with noise, fewer of them once half are dropped.
  await calibrate.getByRole("radio", { name: "With noise" }).click();
  const dropouts = calibrate.getByTestId("noise-form").getByRole("spinbutton", { name: "Dropouts" });
  await dropouts.fill("50");
  await dropouts.press("Enter");
  await expect
    .poll(async () => Number(await view.getAttribute("data-observed")), { timeout: 10_000 })
    .toBeLessThan(0.8 * exact);

  // A second run, with that noise, is kept too; the first can be shown again.
  await calibrate.getByRole("button", { name: "Calibrate" }).click();
  await expect(kept.getByRole("row")).toHaveCount(3);
  await expect(kept.getByRole("row").nth(1)).toContainText("50 % dropped, seed 1");
  await expect(kept.getByRole("row").nth(1)).toContainText("shown");
  await kept.getByRole("row").nth(2).getByRole("button", { name: "Show" }).click();
  await expect(kept.getByRole("row").nth(2)).toContainText("shown");
  await expect(outcome).toContainText(/closed_loop_ur5e-\d{8}-\d{6}-exact/);
});

test("a plain browser previews what a calibration observes", async ({ page }) => {
  await page.goto("/?example=closed_loop_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "File" }).click();
  await expect(page.getByRole("menuitem", { name: "Open scene…" })).toHaveCount(0);
  await expect(page.getByRole("menuitem", { name: "Open files…" })).toBeVisible();
  await page.keyboard.press("Escape");

  // No dataset generation or calibration here: the observations of the open scene.
  await page.getByRole("radio", { name: "Calibrate" }).click();
  const preview = page.getByTestId("observation-preview");
  await expect(preview).toContainText("desktop app");
  await expect(page.getByTestId("dataset-panel")).toHaveCount(0);
  // closed_loop_ur5e: 24 captures of 2 cameras, and the 2592 points `etendue calibrate` reports.
  await expect(preview.getByTestId("observation-totals")).toHaveText("24 captures · 48 views · 2592 points");
  await page
    .getByRole("button", { name: /capture/i })
    .nth(2)
    .click();
  const view = page.getByTestId("observations").getByTestId("camera-view-cam_left");
  await expect(view).toHaveAttribute("data-observed", /^[1-9]\d*$/);
  // With noise: outliers are counted, and the totals follow the form.
  await preview.getByRole("radio", { name: "With noise" }).click();
  const outliers = preview.getByTestId("noise-form").getByRole("spinbutton", { name: "Outliers" });
  await outliers.fill("10");
  await outliers.press("Enter");
  await expect(preview.getByTestId("observation-totals")).toContainText(/2592 points \(\d+ outliers\)/);
});

test("a new project gets the starter library, takes a cell, and reopens", async ({ page }) => {
  await installShell(page);
  await page.goto("/?example=eye_in_hand_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });

  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "New project…" }).click();
  await expect(page.getByText("Opened proj")).toBeVisible();
  const disk = () =>
    page.evaluate(() => JSON.parse(sessionStorage.getItem("fake-disk") ?? "{}") as Record<string, string>);
  const files = await disk();
  expect(JSON.parse(files["/proj/etendue-project.json"]!)).toMatchObject({ name: "proj", cells: [] });
  expect(files["/proj/library/cameras/imx174_lm25hc.json"]).toBeDefined();
  // The example's scene stays open, to save into the project.
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible();

  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "Save as a cell of proj…" }).click();
  await page.getByRole("textbox", { name: "Cell" }).fill("bench");
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved as bench")).toBeVisible();
  const saved = await disk();
  expect((JSON.parse(saved["/proj/etendue-project.json"]!) as { cells: string[] }).cells).toEqual(["bench"]);
  expect(JSON.parse(saved["/proj/bench/scene.json"]!)).toHaveProperty("cameras");
  expect(saved["/proj/bench/scenario.json"]).toBeDefined();

  // A head designed in the Optics workspace goes into the project's library.
  await page.getByRole("radio", { name: "Optics" }).click();
  await page.getByRole("button", { name: "Scheimpflug" }).click();
  await page.getByRole("button", { name: "Save to library" }).click();
  await expect(page.getByText("Saved Laser head")).toBeVisible();
  const head = JSON.parse((await disk())["/proj/library/heads/laser_head.json"]!) as {
    head: { camera: { optics: unknown } };
  };
  expect(head.head.camera.optics).toBeDefined();

  // And a stereo pair from the Rigs workspace, its layout's cameras physical.
  await page.getByRole("radio", { name: "Rigs" }).click();
  await page.getByRole("button", { name: "Save to library" }).click();
  await expect(page.getByText("Saved Stereo pair")).toBeVisible();
  const rig = JSON.parse((await disk())["/proj/library/rigs/stereo_pair.json"]!) as {
    rig: { layout: { count: number; image_x: string; camera: { optics: unknown } } };
  };
  expect(rig.rig.layout).toMatchObject({ count: 2, image_x: "common" });
  expect(rig.rig.layout.camera.optics).toBeDefined();
  await page.getByRole("radio", { name: "Setup" }).click();

  // The next session opens the project and its cell.
  await page.goto("/");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "File" }).click();
  await expect(page.getByRole("menuitem", { name: "bench" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "eye_in_hand_ur5e" })).toHaveCount(0);
  await page.keyboard.press("Escape");

  // Its datasets go into the project (ADR 0009), as do the calibration runs.
  await page.getByRole("radio", { name: "Calibrate" }).click();
  await expect(page.getByTestId("dataset-panel").getByRole("textbox", { name: "Output folder" })).toHaveValue(
    "/proj/datasets/bench",
  );
});

test("the Library workspace: a bank lens used in a project, edited, its copy updated, then deleted", async ({
  page,
}) => {
  await installShell(page);
  await page.goto("/?example=eye_in_hand_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  const disk = () =>
    page.evaluate(() => JSON.parse(sessionStorage.getItem("fake-disk") ?? "{}") as Record<string, string>);
  const file = async (path: string) => {
    const text = (await disk())[path];
    return text === undefined ? undefined : (JSON.parse(text) as Record<string, unknown>);
  };
  const focal = async (path: string) =>
    ((await file(path))?.lens as { focal_length_m?: number } | undefined)?.focal_length_m;

  await page.getByRole("radio", { name: "Library" }).click();
  const list = page.getByTestId("library-list");
  const detail = page.getByTestId("library-detail");
  // The examples are the built-in layer, and read-only.
  await list.locator('[data-layer="builtin"][data-ref="lenses/lm25hc"]').click();
  await expect(detail.getByText("Built-in items are read-only")).toBeVisible();
  await expect(detail.getByRole("button", { name: "Use in project" })).toBeDisabled();

  // A lens made in the bank.
  await page.getByRole("button", { name: "New…" }).click();
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("textbox", { name: "Name" }).fill("Bench lens");
  await expect(dialog.getByRole("textbox", { name: /^Id/ })).toHaveValue("bench_lens");
  await dialog.getByRole("button", { name: "Create" }).click();
  await expect(page.getByText("Saved Bench lens")).toBeVisible();
  expect(await file("/bank/library/lenses/bench_lens.json")).toMatchObject({
    version: 2,
    id: "bench_lens",
    name: "Bench lens",
    lens: { focal_length_m: 0.025 },
  });

  // A new project uses it: a copy, with its origin.
  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "New project…" }).click();
  await expect(page.getByText("Opened proj")).toBeVisible();
  const bankLens = list.locator('[data-layer="bank"][data-ref="lenses/bench_lens"]');
  const copy = list.locator('[data-layer="project"][data-ref="lenses/bench_lens"]');
  await bankLens.click();
  await detail.getByRole("button", { name: "Use in project" }).click();
  await expect(copy).toBeVisible();
  await expect(copy).toContainText("hides bank");
  await expect(bankLens).toContainText("hidden by project");
  expect(await file("/proj/library/lenses/bench_lens.json")).toMatchObject({
    id: "bench_lens",
    origin: { layer: "bank", reference: "lenses/bench_lens" },
  });
  await expect(detail.getByTestId("library-origin")).toContainText("lenses/bench_lens");

  // The bank's lens is measured again: the project's copy is stale.
  await bankLens.click();
  await detail.getByRole("spinbutton", { name: "Focal length" }).fill("35");
  await detail.getByRole("button", { name: "Save", exact: true }).click();
  await expect.poll(() => focal("/bank/library/lenses/bench_lens.json")).toBe(0.035);
  await expect(copy.getByText("stale")).toBeVisible();

  // Update copy brings it up to date, keeping its id.
  await copy.click();
  await expect(detail.getByText("Its source has changed")).toBeVisible();
  await detail.getByRole("button", { name: "Update copy" }).first().click();
  await expect(copy.getByText("stale")).toHaveCount(0);
  expect(await file("/proj/library/lenses/bench_lens.json")).toMatchObject({
    id: "bench_lens",
    lens: { focal_length_m: 0.035 },
    origin: { layer: "bank", reference: "lenses/bench_lens" },
  });

  // Deleting the copy removes its file; the bank's lens shows through again.
  await detail.getByRole("button", { name: "Delete…" }).click();
  await page.getByRole("dialog").getByRole("button", { name: "Delete" }).click();
  await expect(copy).toHaveCount(0);
  expect(await file("/proj/library/lenses/bench_lens.json")).toBeUndefined();
  await expect(bankLens).not.toContainText("hidden by project");
  expect(await file("/bank/library/lenses/bench_lens.json")).toBeDefined();
});

test("a cell built from built-in items copies them into the project, and saves as a cell", async ({ page }) => {
  await installShell(page);
  await page.goto("/?example=eye_in_hand_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "New project…" }).click();
  await expect(page.getByText("Opened proj")).toBeVisible();
  const file = async (path: string) => {
    const text = await page.evaluate(
      (p) => (JSON.parse(sessionStorage.getItem("fake-disk") ?? "{}") as Record<string, string>)[p],
      path,
    );
    return text === undefined ? undefined : (JSON.parse(text) as unknown);
  };

  // The project's own default camera and target; a built-in mount.
  await page.getByRole("button", { name: "New cell…" }).click();
  await page
    .getByTestId("cell-steps")
    .getByRole("button", { name: /Mounting/ })
    .click();
  await page.getByRole("combobox", { name: "Mount (rig on the flange)", exact: true }).click();
  await page.getByRole("option", { name: /^Flange plate, 30 mm · built-in/ }).click();
  await page
    .getByTestId("cell-steps")
    .getByRole("button", { name: /Review/ })
    .click();
  await page.getByRole("button", { name: "Build cell" }).click();
  await expect(page.getByText("Built the cell")).toBeVisible({ timeout: 30_000 });
  await expect
    .poll(() => file("/proj/library/mounts/flange_plate_30mm.json"))
    .toMatchObject({ id: "flange_plate_30mm", origin: { layer: "builtin", reference: "mounts/flange_plate_30mm" } });

  await page.getByRole("button", { name: "File" }).click();
  await page.getByRole("menuitem", { name: "Save as a cell of proj…" }).click();
  await page.getByRole("textbox", { name: "Cell" }).fill("built");
  await page.getByRole("dialog").getByRole("button", { name: "Save" }).click();
  await expect(page.getByText("Saved as built")).toBeVisible();
  expect(await file("/proj/built/scene.json")).toMatchObject({
    rigs: [{ id: "rig", parent: "ur5e/tool0", parent_se3_self: { translation: [0, 0, 0.03] } }],
    cameras: [{ id: "cam", parent: "rig" }],
  });
  expect(await file("/proj/built/scenario.json")).toMatchObject({ steps: [] });
});
