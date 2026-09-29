/**
 * The studio in its Tauri shell, against a mocked shell: `window.__TAURI_INTERNALS__` is
 * injected before the app loads, so `invoke` reaches canned commands instead of Rust. The
 * "filesystem" is the preview server: the mocked `repo_root` is `/repo`, and files under it are
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
          case "repo_root":
            return "/repo";
          case "path_exists":
            return (await fetch(args.path as string, { method: "HEAD" })).ok;
          case "read_text": {
            const res = await fetch(args.path as string);
            if (!res.ok) throw new Error(`${args.path as string}: not found`);
            return res.text();
          }
          case "blender_status":
            return { found: true, exe: "/Applications/Blender.app", version: "5.1.1", pin: "5.1.1", message: null };
          case "cancel_run":
            w.__cancelled = true;
            return true;
          case "generate_dataset": {
            (w.__requests as unknown[]).push(args.request);
            const channel = (args.onProgress as { id: number }).id;
            const send = (index: number, message: unknown) => callbacks.get(channel)?.({ index, message });
            send(0, { kind: "step", stage: "render", done: 1, total: 2 });
            send(1, { kind: "log", line: "etendue: rendered exr/cam_left/cap_000.exr" });
            await new Promise((resolve) => setTimeout(resolve, 1500));
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
                  points: 1080,
                  mislabelled: 0,
                  unmatched: 0,
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
  await expect(page.getByRole("button", { name: "Open scene…" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Import poses…" })).toBeEnabled();

  await page.getByRole("tab", { name: "Dataset" }).click();
  const panel = page.getByTestId("dataset-panel");
  await expect(panel.getByTestId("blender-status")).toContainText("Blender 5.1.1");
  // Examples of the checkout write into target/, never into examples/.
  await expect(panel.getByRole("textbox", { name: "Output folder" })).toHaveValue("/repo/target/studio/closed_loop_ur5e");
  await expect(panel.getByRole("textbox", { name: /Sensor model/ })).toHaveValue(
    "/repo/examples/closed_loop_ur5e/sensor_linear.json",
  );

  await panel.getByRole("button", { name: "Generate" }).click();
  await expect(panel.getByRole("progressbar", { name: "Dataset generation" })).toBeVisible();
  await expect(panel.getByText("Rendering: 1 of 2")).toBeVisible();
  await expect(panel.getByTestId("dataset-log")).toContainText("etendue: rendered exr/cam_left/cap_000.exr");
  await expect(panel.getByRole("button", { name: "Cancel" })).toBeVisible();

  const result = panel.getByTestId("dataset-result");
  await expect(result).toContainText("20 captures, 40 views, 2160 visible points");
  await expect(result.getByRole("cell", { name: "0.0505 px" })).toBeVisible();

  const requests = await page.evaluate(() => (window as unknown as { __requests: Record<string, unknown>[] }).__requests);
  expect(requests).toHaveLength(1);
  const request = requests[0]!;
  expect(request).toMatchObject({
    scene: "/repo/examples/closed_loop_ur5e/scene.json",
    output: "/repo/target/studio/closed_loop_ur5e",
    render: true,
    samples: 16,
    supersample: 4,
    sensor: "/repo/examples/closed_loop_ur5e/sensor_linear.json",
    cameras: [],
  });
  expect((request.scenario as { steps: unknown[] }).steps).toHaveLength(40);
});

test("a plain browser has no Dataset tab", async ({ page }) => {
  await page.goto("/?example=closed_loop_ur5e");
  await expect(page.getByRole("treeitem", { name: /cam_left/ })).toBeVisible({ timeout: 30_000 });
  await expect(page.getByRole("tab", { name: "Dataset" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Open scene…" })).toHaveCount(0);
});
