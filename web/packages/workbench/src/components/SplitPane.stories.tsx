import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fireEvent, fn, userEvent, waitFor } from "storybook/test";

import { SplitPane } from "./SplitPane";

/**
 * A labelled stand-in pane. Its box is set inline as well as by class: the package's story
 * run has no stylesheet, and the play functions below must hold with and without one.
 */
function Fill({ children, height }: { children: string; height?: number }) {
  return (
    <div
      style={height === undefined ? undefined : { height }}
      className="grid h-full w-full place-items-center bg-surface p-3 text-center text-xs text-fg-muted"
    >
      {children}
    </div>
  );
}

const meta = {
  title: "workbench/SplitPane",
  component: SplitPane,
  parameters: {
    docs: {
      description: {
        component: `Two panes and a draggable divider. One pane carries the size (\`sizedPane\`, default \`start\`); the
other takes the rest — so nesting composes, and \`AppShell\` builds its side panels from it.

**Use** it for permanent surfaces whose relative size is the user's call: a navigator beside a viewport,
a viewport above a log. Sizes and limits are pixels or percentages (\`"30%"\`); \`storageKey\` remembers
the size across reloads; \`collapsible\` lets the pane close.

**Don't** use it for content that should scroll with the page, or to lay out a form (that is CSS grid).

**Accessibility**: the divider is a focusable \`role="separator"\` (the WAI-ARIA window-splitter pattern)
with \`aria-valuenow\` (the sized pane in px), \`aria-valuemin\`/\`max\` and \`aria-controls\`. Arrow keys
along the axis move it (Shift: ×4), Home/End go to the limits, Enter collapses and restores a collapsible
pane. A collapsed pane is \`inert\`.`,
      },
    },
  },
  args: {
    children: [<Fill key="a">Navigator</Fill>, <Fill key="b">Viewport</Fill>],
    defaultSize: 200,
    minSize: 120,
    maxSize: "70%",
    onSizeChange: fn(),
  },
  render: (args) => (
    <div style={{ width: 640, height: 256 }} className="rounded-panel border border-line">
      <SplitPane {...args} />
    </div>
  ),
} satisfies Meta<typeof SplitPane>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Horizontal: Story = {
  play: async ({ canvas, args }) => {
    const divider = canvas.getByRole("separator", { name: "Resize panes" });
    await expect(divider).toHaveAttribute("aria-orientation", "vertical");
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "200"));
    await expect(divider).toHaveAttribute("aria-valuemin", "120");
    await expect(divider).toHaveAttribute("aria-valuemax", "448"); // 70% of 640

    divider.focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect(divider).toHaveAttribute("aria-valuenow", "216");
    await expect(args.onSizeChange).toHaveBeenLastCalledWith(216);
    await userEvent.keyboard("{Shift>}{ArrowLeft}{/Shift}");
    await expect(divider).toHaveAttribute("aria-valuenow", "152");
    await userEvent.keyboard("{Home}");
    await expect(divider).toHaveAttribute("aria-valuenow", "120");
    await userEvent.keyboard("{End}");
    await expect(divider).toHaveAttribute("aria-valuenow", "448");
    // The divider controls the sized pane.
    await expect(canvasElementById(divider.getAttribute("aria-controls"))).toHaveTextContent("Navigator");
  },
};

function canvasElementById(id: string | null): HTMLElement {
  const element = id === null ? null : document.getElementById(id);
  if (!element) throw new Error("the sized pane was not rendered");
  return element;
}

export const Drag: Story = {
  play: async ({ canvas, args }) => {
    const divider = canvas.getByRole("separator");
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "200"));
    const { left, top } = divider.getBoundingClientRect();
    await fireEvent.pointerDown(divider, { button: 0, pointerId: 1, clientX: left, clientY: top + 10 });
    await expect(divider).toHaveAttribute("data-dragging");
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left + 60, clientY: top + 10 });
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "260"));
    // Dragging past a limit holds at the limit.
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left - 500, clientY: top + 10 });
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "120"));
    await fireEvent.pointerUp(divider, { button: 0, pointerId: 1, clientX: left - 500, clientY: top + 10 });
    await expect(divider).not.toHaveAttribute("data-dragging");
    await expect(args.onSizeChange).toHaveBeenLastCalledWith(120);
    // A move with no drag in progress changes nothing.
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left + 100, clientY: top + 10 });
    await expect(divider).toHaveAttribute("aria-valuenow", "120");
    // Only the primary button drags.
    await fireEvent.pointerDown(divider, { button: 2, pointerId: 1, clientX: left, clientY: top });
    await expect(divider).not.toHaveAttribute("data-dragging");
  },
};

export const Vertical: Story = {
  args: { orientation: "vertical", sizedPane: "end", defaultSize: 100, minSize: 40, maxSize: 200 },
  render: (args) => (
    <div style={{ width: 480, height: 256 }} className="rounded-panel border border-line">
      <SplitPane {...args}>
        <Fill height={128}>Viewport</Fill>
        <Fill height={128}>Log</Fill>
      </SplitPane>
    </div>
  ),
  play: async ({ canvas }) => {
    const divider = canvas.getByRole("separator");
    await expect(divider).toHaveAttribute("aria-orientation", "horizontal");
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "100"));
    divider.focus();
    // The sized pane is below the divider: moving the divider down shrinks it.
    await userEvent.keyboard("{ArrowDown}");
    await expect(divider).toHaveAttribute("aria-valuenow", "84");
    await userEvent.keyboard("{ArrowUp}{ArrowUp}");
    await expect(divider).toHaveAttribute("aria-valuenow", "116");
    // Horizontal arrows mean nothing to a vertical split.
    await userEvent.keyboard("{ArrowRight}");
    await expect(divider).toHaveAttribute("aria-valuenow", "116");

    const { left, top } = divider.getBoundingClientRect();
    await fireEvent.pointerDown(divider, { button: 0, pointerId: 1, clientX: left + 10, clientY: top });
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left + 10, clientY: top - 30 });
    await fireEvent.pointerUp(divider, { button: 0, pointerId: 1, clientX: left + 10, clientY: top - 30 });
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "146"));
  },
};

export const Collapsible: Story = {
  args: { collapsible: true },
  play: async ({ canvas, canvasElement }) => {
    const divider = canvas.getByRole("separator");
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "200"));
    await expect(divider).toHaveAttribute("aria-valuemin", "0");
    divider.focus();
    await userEvent.keyboard("{Enter}");
    await expect(divider).toHaveAttribute("aria-valuenow", "0");
    const root = canvasElement.querySelector("[data-orientation]");
    await expect(root).toHaveAttribute("data-collapsed");
    const pane = canvasElementById(divider.getAttribute("aria-controls"));
    await expect(pane).toHaveAttribute("inert");
    await userEvent.keyboard("{Enter}");
    await expect(divider).toHaveAttribute("aria-valuenow", "200");

    // Dragging under half the minimum snaps it shut; dragging out again reopens at the minimum.
    const { left, top } = divider.getBoundingClientRect();
    await fireEvent.pointerDown(divider, { button: 0, pointerId: 1, clientX: left, clientY: top });
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left - 150, clientY: top });
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "0"));
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left - 120, clientY: top });
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "120"));
    await fireEvent.pointerMove(divider, { pointerId: 1, clientX: left - 180, clientY: top });
    await fireEvent.pointerUp(divider, { button: 0, pointerId: 1, clientX: left - 180, clientY: top });
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "0"));
    // Restoring from a drag-collapse returns to the last open size.
    await userEvent.keyboard("{Enter}");
    await expect(divider).toHaveAttribute("aria-valuenow", "120");
  },
};

/** The size kept by the caller: the divider reports, the caller decides. */
function ControlledSplit(args: Parameters<typeof SplitPane>[0]) {
  const [size, setSize] = useState(240);
  return (
    <div className="flex flex-col gap-2">
      <div style={{ width: 640, height: 192 }} className="rounded-panel border border-line">
        <SplitPane
          {...args}
          size={size}
          onSizeChange={(next) => {
            setSize(next);
            args.onSizeChange?.(next);
          }}
        />
      </div>
      <span className="font-mono text-xs text-fg-muted">size = {size}px</span>
    </div>
  );
}

export const Controlled: Story = {
  render: (args) => <ControlledSplit {...args} />,
  play: async ({ canvas, args }) => {
    const divider = canvas.getByRole("separator");
    await expect(divider).toHaveAttribute("aria-valuenow", "240");
    divider.focus();
    await userEvent.keyboard("{ArrowLeft}");
    await expect(args.onSizeChange).toHaveBeenLastCalledWith(224);
    await expect(canvas.getByText("size = 224px")).toBeInTheDocument();
  },
};

const STORAGE_KEY = "workbench-story-split";

export const Persisted: Story = {
  args: { storageKey: STORAGE_KEY },
  beforeEach: () => {
    window.localStorage.setItem(STORAGE_KEY, "300");
    return () => window.localStorage.removeItem(STORAGE_KEY);
  },
  play: async ({ canvas }) => {
    const divider = canvas.getByRole("separator");
    // Starts from the remembered size rather than `defaultSize`.
    await waitFor(() => expect(divider).toHaveAttribute("aria-valuenow", "300"));
    divider.focus();
    await userEvent.keyboard("{ArrowRight}");
    await expect(window.localStorage.getItem(STORAGE_KEY)).toBe("316");
  },
};

export const Nested: Story = {
  render: (args) => (
    <div style={{ width: 720, height: 288 }} className="rounded-panel border border-line">
      <SplitPane {...args} aria-label="Resize navigator">
        <Fill>Navigator</Fill>
        <SplitPane orientation="vertical" sizedPane="end" defaultSize={80} minSize={40} aria-label="Resize timeline">
          <SplitPane sizedPane="end" defaultSize={180} minSize={120} aria-label="Resize inspector">
            <Fill>Viewport</Fill>
            <Fill>Inspector</Fill>
          </SplitPane>
          <Fill>Timeline</Fill>
        </SplitPane>
      </SplitPane>
    </div>
  ),
  play: async ({ canvas }) => {
    await expect(canvas.getAllByRole("separator")).toHaveLength(3);
    const inspector = canvas.getByRole("separator", { name: "Resize inspector" });
    await waitFor(() => expect(inspector).toHaveAttribute("aria-valuenow", "180"));
    inspector.focus();
    await userEvent.keyboard("{ArrowLeft}");
    await expect(inspector).toHaveAttribute("aria-valuenow", "196");
    await expect(canvas.getByText("Viewport")).toBeVisible();
  },
};
