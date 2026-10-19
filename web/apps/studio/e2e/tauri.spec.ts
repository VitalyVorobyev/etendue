/**
 * The studio in its Tauri shell, against a mocked shell: `window.__TAURI_INTERNALS__` is
 * injected before the app loads, so `invoke` reaches canned commands instead of Rust. The
 * "filesystem" is the preview server: the mocked `data_root` is `/repo`, and files under it are
 * fetched from the served repository, so absolute paths and URLs coincide.
 *
 * The native pipeline itself is tested in Rust (`src-tauri`, `cargo test`); this checks the
 * wiring: what the Dataset panel sends, and how it shows progress and results.
 */

import { type Page, expect, test } from "@playwright/test";

async function installShell(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (data: unknown) => void>();
    let next = 1;
    const w = window as unknown as Record<string, unknown>;
    w.__requests = [];
    w.__cancelled = false;
    // A project folder the user "picks", kept in sessionStorage so it survives a reload.
    const PROJECT = "/proj";
    const disk = (): Record<string, string> =>
      JSON.parse(sessionStorage.getItem("fake-disk") ?? "{}") as Record<string, string>;
    const store = (files: Record<string, string>) => sessionStorage.setItem("fake-disk", JSON.stringify(files));
    const inProject = (path: string) => path === PROJECT || path.startsWith(`${PROJECT}/`);
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
          case "generate_dataset": {
            (w.__requests as unknown[]).push(args.request);
            const channel = (args.onProgress as { id: number }).id;
            const send = (index: number, message: unknown) => callbacks.get(channel)?.({ index, message });
            send(0, { kind: "step", stage: "render", done: 1, total: 2 });
            send(1, { kind: "log", line: "etendue: rendered exr/cam_left/cap_000.exr" });
            await new Promise((resolve) => setTimeout(resolve, 5000));
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
  await page.goto("/?example=closed_loop_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "File" }).click();
  await expect(page.getByRole("menuitem", { name: "Open scene…" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Import tool poses…" })).toBeEnabled();
  await page.keyboard.press("Escape");

  await page.getByRole("radio", { name: "Dataset" }).click();
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
  await page.getByRole("radio", { name: "Dataset" }).click();
  await expect(page.getByTestId("dataset-panel").getByRole("button", { name: "Cancel" })).toBeVisible();
  await expect(page.getByTestId("dataset-panel").getByRole("button", { name: "Generate" })).toBeDisabled();

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
});

test("a plain browser has no Dataset tab", async ({ page }) => {
  await page.goto("/?example=closed_loop_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("radio", { name: "Dataset" })).toHaveCount(0);
  await page.getByRole("button", { name: "File" }).click();
  await expect(page.getByRole("menuitem", { name: "Open scene…" })).toHaveCount(0);
  await expect(page.getByRole("menuitem", { name: "Open files…" })).toBeVisible();
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

  // The next session opens the project and its cell.
  await page.goto("/");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await page.getByRole("button", { name: "File" }).click();
  await expect(page.getByRole("menuitem", { name: "bench" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "eye_in_hand_ur5e" })).toHaveCount(0);
});
