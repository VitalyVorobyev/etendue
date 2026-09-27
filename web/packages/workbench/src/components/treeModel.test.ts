import { describe, expect, it } from "vitest";

import { type TreeNode, ancestorIds, parentIds, treeKeyAction, visibleRows } from "./treeModel";

/** world → { ur5e → { base, tool0 → { rig → { cam0, laser } } }, table, target } */
const TREE: TreeNode[] = [
  {
    id: "world",
    label: "world",
    children: [
      {
        id: "ur5e",
        label: "ur5e",
        children: [
          { id: "base", label: "base" },
          {
            id: "tool0",
            label: "tool0",
            children: [
              {
                id: "rig",
                label: "rig",
                children: [
                  { id: "cam0", label: "cam0" },
                  { id: "laser", label: "laser", disabled: true },
                ],
              },
            ],
          },
        ],
      },
      { id: "table", label: "Table", children: [] },
      { id: "target", label: "target" },
    ],
  },
];

const ids = (open: string[]) => visibleRows(TREE, new Set(open)).map((row) => row.node.id);

describe("visibleRows", () => {
  it("shows the roots, and the children of expanded parents only", () => {
    expect(ids([])).toEqual(["world"]);
    expect(ids(["world"])).toEqual(["world", "ur5e", "table", "target"]);
    // An expanded parent inside a collapsed one stays hidden.
    expect(ids(["world", "tool0"])).toEqual(["world", "ur5e", "table", "target"]);
    expect(ids(["world", "ur5e", "tool0"])).toEqual(["world", "ur5e", "base", "tool0", "rig", "table", "target"]);
  });

  it("gives each row its level, position, set size and parent", () => {
    const rows = visibleRows(TREE, new Set(["world", "ur5e"]));
    const tool0 = rows.find((row) => row.node.id === "tool0");
    expect(tool0).toMatchObject({ level: 3, posInSet: 2, setSize: 2, parentId: "ur5e", isParent: true, isExpanded: false });
    const target = rows.find((row) => row.node.id === "target");
    expect(target).toMatchObject({ level: 2, posInSet: 3, setSize: 3, isParent: false });
  });

  it("treats an empty children array as a parent", () => {
    const table = visibleRows(TREE, new Set(["world"])).find((row) => row.node.id === "table");
    expect(table?.isParent).toBe(true);
  });
});

describe("parentIds and ancestorIds", () => {
  it("lists every parent", () => {
    expect(parentIds(TREE)).toEqual(["world", "ur5e", "tool0", "rig", "table"]);
  });

  it("finds the path to a node, root first", () => {
    expect(ancestorIds(TREE, "cam0")).toEqual(["world", "ur5e", "tool0", "rig"]);
    expect(ancestorIds(TREE, "world")).toEqual([]);
    expect(ancestorIds(TREE, "nope")).toEqual([]);
  });
});

describe("treeKeyAction", () => {
  const open = visibleRows(TREE, new Set(["world", "ur5e", "tool0", "rig"]));

  it("moves up and down the visible rows, stopping at the ends", () => {
    expect(treeKeyAction(open, "ur5e", "ArrowDown")).toEqual({ type: "focus", id: "base" });
    expect(treeKeyAction(open, "ur5e", "ArrowUp")).toEqual({ type: "focus", id: "world" });
    expect(treeKeyAction(open, "world", "ArrowUp")).toBeNull();
    expect(treeKeyAction(open, "target", "ArrowDown")).toBeNull();
  });

  it("jumps to the first and last rows", () => {
    expect(treeKeyAction(open, "rig", "Home")).toEqual({ type: "focus", id: "world" });
    expect(treeKeyAction(open, "rig", "End")).toEqual({ type: "focus", id: "target" });
    expect(treeKeyAction(open, "world", "Home")).toBeNull();
  });

  it("expands a closed parent, enters an open one, and ignores a leaf on →", () => {
    const closed = visibleRows(TREE, new Set(["world"]));
    expect(treeKeyAction(closed, "ur5e", "ArrowRight")).toEqual({ type: "expand", id: "ur5e" });
    expect(treeKeyAction(open, "ur5e", "ArrowRight")).toEqual({ type: "focus", id: "base" });
    expect(treeKeyAction(open, "cam0", "ArrowRight")).toBeNull();
    // An open parent with no children has nowhere to go.
    const withTable = visibleRows(TREE, new Set(["world", "table"]));
    expect(treeKeyAction(withTable, "table", "ArrowRight")).toBeNull();
  });

  it("collapses an open parent, else goes to the parent, on ←", () => {
    expect(treeKeyAction(open, "rig", "ArrowLeft")).toEqual({ type: "collapse", id: "rig" });
    expect(treeKeyAction(open, "cam0", "ArrowLeft")).toEqual({ type: "focus", id: "rig" });
    const closed = visibleRows(TREE, new Set([]));
    expect(treeKeyAction(closed, "world", "ArrowLeft")).toBeNull();
  });

  it("selects on Enter and Space, except a disabled node", () => {
    expect(treeKeyAction(open, "cam0", "Enter")).toEqual({ type: "select", id: "cam0" });
    expect(treeKeyAction(open, "cam0", " ")).toEqual({ type: "select", id: "cam0" });
    expect(treeKeyAction(open, "laser", "Enter")).toBeNull();
  });

  it("expands every closed sibling parent on *", () => {
    const closed = visibleRows(TREE, new Set(["world"]));
    expect(treeKeyAction(closed, "target", "*")).toEqual({ type: "expand-siblings", ids: ["ur5e", "table"] });
    expect(treeKeyAction(open, "cam0", "*")).toBeNull();
  });

  it("jumps to the next label starting with a typed character, wrapping", () => {
    expect(treeKeyAction(open, "world", "t")).toEqual({ type: "focus", id: "tool0" });
    expect(treeKeyAction(open, "tool0", "T")).toEqual({ type: "focus", id: "table" });
    expect(treeKeyAction(open, "target", "w")).toEqual({ type: "focus", id: "world" });
    // The only match is the focused row itself; unmatched and non-printable keys do nothing.
    expect(treeKeyAction(open, "world", "w")).toBeNull();
    expect(treeKeyAction(open, "world", "q")).toBeNull();
    expect(treeKeyAction(open, "world", "Tab")).toBeNull();
  });

  it("does nothing for a row that is not visible", () => {
    expect(treeKeyAction(open, "ghost", "ArrowDown")).toBeNull();
  });
});
