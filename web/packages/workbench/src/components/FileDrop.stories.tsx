import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fireEvent, fn, waitFor, within } from "storybook/test";

import { FileDrop } from "./FileDrop";

/** A drag's data carrying the given files, as the browser builds it for a drag from the desktop. */
function filesTransfer(...files: File[]): DataTransfer {
  const transfer = new DataTransfer();
  for (const file of files) transfer.items.add(file);
  return transfer;
}

/** Let React render and a drop's files be collected, before asserting that nothing happened. */
const settle = () => new Promise((resolve) => setTimeout(resolve, 50));

/** A drag carrying text, not files. */
function textTransfer(): DataTransfer {
  const transfer = new DataTransfer();
  transfer.setData("text/plain", "not a file");
  return transfer;
}

/**
 * Dispatch a native drag event. Not `fireEvent`: Testing Library swaps the `dataTransfer` it is
 * given for a fresh, empty one, which carries no files.
 */
function drag(target: EventTarget, type: "dragenter" | "dragover" | "dragleave" | "drop", dataTransfer: DataTransfer) {
  target.dispatchEvent(new DragEvent(type, { bubbles: true, cancelable: true, dataTransfer }));
}

const scene = () => new File(['{"version":1}'], "scene.json", { type: "application/json" });
const notes = () => new File(["hello"], "notes.txt", { type: "text/plain" });

function fileInput(root: HTMLElement): HTMLInputElement {
  const input = root.querySelector<HTMLInputElement>("input[type=file]");
  if (!input) throw new Error("FileDrop's input was not rendered");
  return input;
}

/** Choose files in the picker, bypassing the browser's own `accept` filter. */
async function pick(input: HTMLInputElement, ...files: File[]) {
  input.files = filesTransfer(...files).files;
  await fireEvent.change(input);
}

const meta = {
  title: "workbench/FileDrop",
  component: FileDrop,
  parameters: {
    docs: {
      description: {
        component: `Opens local files by drop or by picker. Inline it is a dashed drop zone with an "Open files…" button;
with \`overlay\` it renders only the button and takes drops anywhere in the window, showing a full-window
overlay while files are dragged over it.

**Use** \`accept\` to say which files are wanted — it is applied to drops and the picker alike, and the rest
go to \`onReject\` (say so, with a \`toast\`). Dropped folders are walked; their files carry
\`webkitRelativePath\`. \`directory\` makes the picker choose a folder.

**Don't** use it for uploads with progress (that is the app's upload flow), or to open anything that is
not a local file (a URL is an \`Input\`).

**Accessibility**: the picker is a real button, the one route for keyboard and assistive technology; the
drop zone and the overlay are pointer conveniences (the overlay is \`aria-hidden\`). Only drags carrying
files are reacted to. State: \`data-dragging\`, \`data-disabled\`.`,
      },
    },
  },
  args: { onFiles: fn(), onReject: fn(), accept: ".json" },
  render: (args) => (
    <div style={{ width: 420 }}>
      <FileDrop {...args} />
    </div>
  ),
} satisfies Meta<typeof FileDrop>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Inline: Story = {
  play: async ({ canvas, canvasElement, args }) => {
    await expect(canvas.getByRole("button", { name: "Open files…" })).toBeEnabled();
    const input = fileInput(canvasElement);
    await expect(input).toHaveAttribute("accept", ".json");
    await expect(input.multiple).toBe(true);

    await pick(input, scene(), notes());
    await expect(args.onFiles).toHaveBeenCalledWith([expect.objectContaining({ name: "scene.json" })]);
    await expect(args.onReject).toHaveBeenCalledWith([expect.objectContaining({ name: "notes.txt" })]);
    // The input is cleared, so picking the same file again is a change.
    await expect(input.value).toBe("");
    // An empty pick (cancelled dialog) reports nothing.
    await fireEvent.change(input);
    await expect(args.onFiles).toHaveBeenCalledTimes(1);
  },
};

export const Drop: Story = {
  play: async ({ canvas, args }) => {
    const zone = canvas.getByText("Drop files here, or").parentElement;
    if (!zone) throw new Error("no drop zone");
    const transfer = filesTransfer(scene());
    drag(zone, "dragenter", transfer);
    await waitFor(() => expect(zone).toHaveAttribute("data-dragging"));
    drag(zone, "dragover", transfer);
    drag(zone, "dragleave", transfer);
    await waitFor(() => expect(zone).not.toHaveAttribute("data-dragging"));
    drag(zone, "dragenter", transfer);
    await waitFor(() => expect(zone).toHaveAttribute("data-dragging"));
    drag(zone, "drop", transfer);
    await waitFor(() => expect(zone).not.toHaveAttribute("data-dragging"));
    await waitFor(() => expect(args.onFiles).toHaveBeenCalledWith([expect.objectContaining({ name: "scene.json" })]));
  },
};

export const IgnoresTextDrags: Story = {
  play: async ({ canvas }) => {
    const zone = canvas.getByText("Drop files here, or").parentElement;
    if (!zone) throw new Error("no drop zone");
    const text = textTransfer();
    drag(zone, "dragenter", text);
    await settle();
    await expect(zone).not.toHaveAttribute("data-dragging");
  },
};

export const Overlay: Story = {
  args: { overlay: true, overlayMessage: "Drop a scenario to open it", multiple: false },
  play: async ({ canvas, args }) => {
    await expect(canvas.getByRole("button", { name: "Open files…" })).toBeInTheDocument();
    const page = within(document.body);
    const transfer = filesTransfer(scene(), notes(), scene());
    drag(document.body, "dragenter", transfer);
    await expect(await page.findByText("Drop a scenario to open it")).toBeInTheDocument();
    drag(document.body, "dragleave", transfer);
    await waitFor(() => expect(page.queryByText("Drop a scenario to open it")).toBeNull());

    drag(document.body, "dragenter", transfer);
    drag(document.body, "dragover", transfer);
    drag(document.body, "drop", transfer);
    await waitFor(() => expect(page.queryByText("Drop a scenario to open it")).toBeNull());
    // Not `multiple`: only the first accepted file.
    await waitFor(() => expect(args.onFiles).toHaveBeenCalledWith([expect.objectContaining({ name: "scene.json" })]));
    await expect(args.onReject).toHaveBeenCalledWith([expect.objectContaining({ name: "notes.txt" })]);

    // A drag carrying no files is not ours.
    const text = textTransfer();
    drag(document.body, "dragenter", text);
    drag(document.body, "dragover", text);
    drag(document.body, "drop", text);
    await settle();
    await expect(page.queryByText("Drop a scenario to open it")).toBeNull();
    await expect(args.onFiles).toHaveBeenCalledTimes(1);
  },
};

export const Directory: Story = {
  args: { directory: true, accept: undefined },
  play: async ({ canvas, canvasElement }) => {
    await expect(canvas.getByRole("button", { name: "Open folder…" })).toBeInTheDocument();
    await expect(fileInput(canvasElement).webkitdirectory).toBe(true);
  },
};

export const Disabled: Story = {
  args: { disabled: true },
  play: async ({ canvas, args }) => {
    await expect(canvas.getByRole("button", { name: "Open files…" })).toBeDisabled();
    const zone = canvas.getByText("Drop files here, or").parentElement;
    if (!zone) throw new Error("no drop zone");
    await expect(zone).toHaveAttribute("data-disabled");
    const transfer = filesTransfer(scene());
    drag(zone, "dragenter", transfer);
    drag(zone, "drop", transfer);
    await settle();
    await expect(zone).not.toHaveAttribute("data-dragging");
    await expect(args.onFiles).not.toHaveBeenCalled();
  },
};
