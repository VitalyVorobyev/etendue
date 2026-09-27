import type { Meta, StoryObj } from "@storybook/react-vite";
import { Axis3d, Bot, Camera, Crosshair, Globe, Grid3x3, Link2, Zap } from "lucide-react";
import { useState } from "react";
import { expect, fn, userEvent } from "storybook/test";

import { DensityProvider } from "@vitavision/ui";

import { TreeView, type TreeViewProps } from "./TreeView";
import type { TreeNode } from "./treeModel";

/** A robot cell's frame tree: world → robot → links → rig → sensors, plus a table and a target. */
const CELL: TreeNode[] = [
  {
    id: "world",
    label: "world",
    icon: <Globe />,
    children: [
      {
        id: "ur5e",
        label: "ur5e",
        icon: <Bot />,
        meta: "robot",
        children: [
          { id: "base", label: "base", icon: <Link2 />, meta: "link" },
          { id: "shoulder", label: "shoulder_link", icon: <Link2 />, meta: "link" },
          {
            id: "tool0",
            label: "tool0",
            icon: <Link2 />,
            meta: "link",
            children: [
              {
                id: "rig",
                label: "eye_in_hand_rig",
                icon: <Axis3d />,
                meta: "rig",
                children: [
                  { id: "cam0", label: "cam0", icon: <Camera />, meta: "camera" },
                  { id: "laser0", label: "laser0", icon: <Zap />, meta: "laser" },
                ],
              },
            ],
          },
        ],
      },
      { id: "table", label: "table", icon: <Grid3x3 />, meta: "part", children: [] },
      { id: "board", label: "charuco_board", icon: <Crosshair />, meta: "target" },
      { id: "spare", label: "spare_camera", icon: <Camera />, meta: "offline", disabled: true },
    ],
  },
];

/** Selection is controlled; this keeps it in state and reports each selection. */
function StatefulTree(props: TreeViewProps) {
  const [selected, setSelected] = useState(props.selectedId ?? null);
  return (
    <div style={{ width: 280 }} className="rounded-panel border border-line bg-surface">
      <TreeView
        {...props}
        selectedId={selected}
        onSelect={(id, node) => {
          setSelected(id);
          props.onSelect?.(id, node);
        }}
      />
    </div>
  );
}

const meta = {
  title: "workbench/TreeView",
  component: TreeView,
  parameters: {
    docs: {
      description: {
        component: `A data-driven tree with single selection: \`nodes\` in, \`selectedId\`/\`onSelect\` controlled, expansion
controlled (\`expanded\`/\`onExpandedChange\`) or not (\`defaultExpanded\`, \`defaultExpandAll\`). Icons and a
mono \`meta\` annotation come from the data.

**Use** it for a navigator over a hierarchy the app already has — a robot cell's frame tree, a scene graph.
When \`selectedId\` changes to a hidden node (a pick in the viewport), its ancestors open and it scrolls into view.

**Don't** use it for a flat list (that is a \`Table\` or a list box), for navigation between pages, or for
multi-selection (not supported).

**Accessibility**: the WAI-ARIA tree pattern in its flat form — every visible row is a \`treeitem\` with
\`aria-level\`, \`aria-posinset\`, \`aria-setsize\`, \`aria-selected\` and (parents) \`aria-expanded\`. One Tab
stop; ↑/↓, →/← (expand, enter, collapse, up to parent), Home/End, Enter/Space to select, \`*\` to expand
siblings, and type-ahead by first letter. Icons are decorative.`,
      },
    },
  },
  args: {
    nodes: CELL,
    "aria-label": "Cell frames",
    defaultExpanded: ["world", "ur5e"],
    onSelect: fn(),
    onExpandedChange: fn(),
  },
  render: (args) => <StatefulTree {...args} />,
} satisfies Meta<typeof TreeView>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  play: async ({ canvas, args }) => {
    const tree = canvas.getByRole("tree", { name: "Cell frames" });
    await expect(tree).toBeInTheDocument();
    const world = canvas.getByRole("treeitem", { name: /^world/ });
    await expect(world).toHaveAttribute("aria-expanded", "true");
    await expect(world).toHaveAttribute("aria-level", "1");
    await expect(world).toHaveAttribute("tabindex", "0");
    const tool0 = canvas.getByRole("treeitem", { name: /^tool0/ });
    await expect(tool0).toHaveAttribute("aria-expanded", "false");
    await expect(tool0).toHaveAttribute("aria-level", "3");
    await expect(tool0).toHaveAttribute("aria-posinset", "3");
    await expect(tool0).toHaveAttribute("aria-setsize", "3");
    await expect(canvas.queryByRole("treeitem", { name: /^cam0/ })).toBeNull();

    await userEvent.click(canvas.getByRole("treeitem", { name: /^charuco_board/ }));
    await expect(args.onSelect).toHaveBeenCalledWith("board", expect.objectContaining({ id: "board" }));
    await expect(canvas.getByRole("treeitem", { name: /^charuco_board/ })).toHaveAttribute("aria-selected", "true");
    await expect(world).toHaveAttribute("aria-selected", "false");
  },
};

export const Keyboard: Story = {
  play: async ({ canvas, args }) => {
    const row = (name: string) => canvas.getByRole("treeitem", { name: new RegExp(`^${name}`) });
    row("world").focus();
    await userEvent.keyboard("{ArrowDown}");
    await expect(row("ur5e")).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}");
    await expect(row("tool0")).toHaveFocus();

    // → opens tool0, → again enters it, ← goes back up and ← again closes it.
    await userEvent.keyboard("{ArrowRight}");
    await expect(row("tool0")).toHaveAttribute("aria-expanded", "true");
    await expect(args.onExpandedChange).toHaveBeenLastCalledWith(["world", "ur5e", "tool0"]);
    await userEvent.keyboard("{ArrowRight}");
    await expect(row("eye_in_hand_rig")).toHaveFocus();
    await expect(row("eye_in_hand_rig")).toHaveAttribute("tabindex", "0");
    await expect(row("tool0")).toHaveAttribute("tabindex", "-1");
    await userEvent.keyboard("{ArrowLeft}");
    await expect(row("tool0")).toHaveFocus();
    await userEvent.keyboard("{ArrowLeft}");
    await expect(row("tool0")).toHaveAttribute("aria-expanded", "false");

    await userEvent.keyboard("{End}");
    await expect(row("spare_camera")).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await expect(args.onSelect).not.toHaveBeenCalled(); // disabled
    await userEvent.keyboard("{Home}");
    await expect(row("world")).toHaveFocus();

    // Type-ahead: the next label starting with "c" (charuco_board), then Space selects it.
    await userEvent.keyboard("c");
    await expect(row("charuco_board")).toHaveFocus();
    await userEvent.keyboard(" ");
    await expect(args.onSelect).toHaveBeenCalledWith("board", expect.objectContaining({ id: "board" }));

    // * opens every closed sibling of the focused row.
    row("shoulder_link").focus();
    await userEvent.keyboard("*");
    await expect(row("tool0")).toHaveAttribute("aria-expanded", "true");
    // Modified keys are left to the browser.
    await userEvent.keyboard("{Control>}{ArrowUp}{/Control}");
    await expect(row("shoulder_link")).toHaveFocus();
  },
};

export const ChevronToggles: Story = {
  play: async ({ canvas, canvasElement, args }) => {
    const tool0 = canvas.getByRole("treeitem", { name: /^tool0/ });
    const chevron = tool0.querySelector("span[aria-hidden]");
    if (!chevron) throw new Error("tool0 has no chevron");
    await userEvent.click(chevron);
    await expect(tool0).toHaveAttribute("aria-expanded", "true");
    await expect(tool0).toHaveAttribute("data-state", "open");
    // Toggling is not selecting.
    await expect(args.onSelect).not.toHaveBeenCalled();
    await userEvent.click(chevron);
    await expect(tool0).toHaveAttribute("data-state", "closed");
    await expect(canvasElement.querySelectorAll("[role=treeitem]")).toHaveLength(8);
  },
};

export const ExpandAll: Story = {
  args: { defaultExpandAll: true },
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("treeitem", { name: /^cam0/ })).toHaveAttribute("aria-level", "5");
    await expect(canvas.getAllByRole("treeitem")).toHaveLength(11);
  },
};

/** A viewport pick, simulated: selecting a node that is inside collapsed parents. */
function RevealHarness(props: TreeViewProps) {
  const [selected, setSelected] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<string[]>(["world"]);
  return (
    <div className="flex flex-col gap-2" style={{ width: 280 }}>
      <button type="button" onClick={() => setSelected("laser0")}>
        Pick laser0 in the viewport
      </button>
      <TreeView
        {...props}
        defaultExpanded={undefined}
        expanded={expanded}
        onExpandedChange={(next) => {
          setExpanded(next);
          props.onExpandedChange?.(next);
        }}
        selectedId={selected}
        onSelect={setSelected}
      />
    </div>
  );
}

export const RevealSelection: Story = {
  render: (args) => <RevealHarness {...args} />,
  play: async ({ canvas, args }) => {
    await expect(canvas.queryByRole("treeitem", { name: /^laser0/ })).toBeNull();
    await userEvent.click(canvas.getByRole("button", { name: "Pick laser0 in the viewport" }));
    const laser = await canvas.findByRole("treeitem", { name: /^laser0/ });
    await expect(laser).toHaveAttribute("aria-selected", "true");
    await expect(laser).toHaveAttribute("tabindex", "0");
    await expect(args.onExpandedChange).toHaveBeenCalledWith(["world", "ur5e", "tool0", "rig"]);
  },
};

export const Compact: Story = {
  args: { defaultExpandAll: true },
  render: (args) => (
    <DensityProvider value="compact">
      <StatefulTree {...args} />
    </DensityProvider>
  ),
  play: async ({ canvas }) => {
    await expect(canvas.getByRole("treeitem", { name: /^world/ }).className).toContain("h-6");
  },
};
