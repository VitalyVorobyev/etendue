/**
 * The studio's view of a scene: one node per selectable thing (robots, links, entities,
 * named frames) arranged by attachment, as the frame tree shows it.
 */

import type { BakedScenario, SceneSpec } from "@etendue/wasm";

/** What a node is. */
export type NodeKind =
  | "world"
  | "frame"
  | "robot"
  | "links"
  | "link"
  | "rig"
  | "camera"
  | "laser"
  | "light"
  | "target"
  | "part";

/** One node of the scene tree. */
export interface SceneNode {
  /**
   * Unique: an entity id, a robot id, `"<robot>/<link>"`, `"world"`, or `"<robot>/"` for the
   * group of a robot's bare links.
   */
  id: string;
  kind: NodeKind;
  /** The baked frame that places it (a robot is placed by its base link). */
  frame: string;
  children: SceneNode[];
}

const ENTITY_KINDS = [
  ["frames", "frame"],
  ["rigs", "rig"],
  ["cameras", "camera"],
  ["lasers", "laser"],
  ["lights", "light"],
  ["targets", "target"],
  ["parts", "part"],
] as const;

/**
 * Build the tree. Robots hang under their parent frame with their links (in baked order)
 * as children; every entity hangs under the node of its parent frame.
 */
export function sceneTree(scene: SceneSpec, baked: BakedScenario): SceneNode {
  const world: SceneNode = { id: "world", kind: "world", frame: "world", children: [] };
  const byFrame = new Map<string, SceneNode>([["world", world]]);
  const pending: { node: SceneNode; parent: string }[] = [];

  for (const robot of scene.robots ?? []) {
    const links = baked.frames.filter((f) => f.startsWith(`${robot.id}/`));
    const node: SceneNode = {
      id: robot.id,
      kind: "robot",
      frame: links[0] ?? "world",
      children: links.map((f) => ({ id: f, kind: "link", frame: f, children: [] })),
    };
    for (const link of node.children) byFrame.set(link.frame, link);
    pending.push({ node, parent: robot.parent });
  }
  for (const [field, kind] of ENTITY_KINDS) {
    for (const e of scene[field] ?? []) {
      const node: SceneNode = { id: e.id, kind, frame: e.id, children: [] };
      byFrame.set(e.id, node);
      pending.push({ node, parent: e.parent });
    }
  }
  for (const { node, parent } of pending) (byFrame.get(parent) ?? world).children.push(node);

  // A robot shows the links something is attached to; the other links fold into one group.
  const robots = [...walk(world)].filter((n) => n.kind === "robot");
  for (const robot of robots) {
    const carrying = robot.children.filter((l) => l.children.length > 0);
    const bare = robot.children.filter((l) => l.children.length === 0);
    robot.children = [
      ...carrying,
      ...(bare.length > 0 ? [{ id: `${robot.id}/`, kind: "links" as const, frame: robot.frame, children: bare }] : []),
    ];
  }
  return world;
}

/** Every node, depth first. */
export function* walk(node: SceneNode): Generator<SceneNode> {
  yield node;
  for (const c of node.children) yield* walk(c);
}

/** The node with `id`, if any. */
export function findNode(root: SceneNode, id: string): SceneNode | undefined {
  for (const n of walk(root)) if (n.id === id) return n;
  return undefined;
}
