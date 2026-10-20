/**
 * The book's interactive widgets. `book/js/demos-loader.js` imports this module on pages with
 * `<div class="etendue-demo" data-demo="NAME">` and calls {@link mount}. Each widget loads the
 * wasm kernel on first use.
 */

import { describe } from "./ui";

type Widget = (host: HTMLElement) => Promise<void>;

/** Every widget, by its `data-demo` name; each module loads when a page uses it. */
const WIDGETS: Record<string, () => Promise<{ default: Widget }>> = {
  optics: () => import("./demos/optics"),
  blur: () => import("./demos/blur"),
  scheimpflug: () => import("./demos/scheimpflug"),
  head: () => import("./demos/head"),
  coverage: () => import("./demos/coverage"),
  kinematics: () => import("./demos/kinematics"),
  motion: () => import("./demos/motion"),
  poses: () => import("./demos/poses"),
  noise: () => import("./demos/noise"),
};

/** The widget names a page may use. */
export const NAMES = Object.keys(WIDGETS);

async function mountOne(host: HTMLElement): Promise<void> {
  const name = host.dataset.demo ?? "";
  const load = WIDGETS[name];
  if (!load) {
    host.textContent = `Unknown widget "${name}".`;
    host.setAttribute("data-state", "error");
    return;
  }
  host.setAttribute("data-state", "loading");
  host.textContent = "Loading the interactive widget…";
  try {
    const widget = (await load()).default;
    await widget(host);
  } catch (e) {
    console.error(`etendue widget ${name}:`, e);
    host.textContent = `The widget could not start: ${describe(e)}`;
    host.setAttribute("data-state", "error");
  }
}

/** Mount every widget in `nodes`. */
export function mount(nodes: Iterable<Element>): void {
  for (const node of nodes) if (node instanceof HTMLElement) void mountOne(node);
}
