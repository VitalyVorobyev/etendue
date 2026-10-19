/**
 * Jogging a robot at the playhead (P12): a joint vector the scenario does not hold, shown without
 * a bake. Forward kinematics (the kernel's) gives every link's new pose in the robot's base; each
 * link, and whatever hangs on it, moves by one correction in the world, as an edit's pose
 * override does (`kernel/posed.ts`).
 */

import type { EtendueScene, Iso3Schema, RobotSource, ScenarioSpec, SceneSpec, Step } from "@etendue/wasm";
import { type FrameTreeRuntime, type Iso3, composeIso3, invertIso3 } from "@vitavision/three";

import type { PoseOverride } from "../kernel/posed";
import { hangingOn } from "../scene/effect";

/** A robot held at joints `q` while the playhead stays at sample `k`. */
export interface Jog {
  robot: string;
  q: number[];
  k: number;
  /** The last IK target was out of reach (the arm stayed where it was). */
  unreachable?: boolean;
}

/** The robot's frames: its base (where FK is expressed) and its TCP link. */
export interface RobotFrames {
  base: string;
  tcp: string;
}

export function robotFrames(robots: readonly RobotSource[], robot: string): RobotFrames | null {
  const m = robots.find((r) => r.id === robot)?.manifest;
  return m ? { base: `${robot}/${m.base_link}`, tcp: `${robot}/${m.tcp_link}` } : null;
}

const isoAt = (poses: Float64Array, i: number): Iso3 => ({
  rotation: [poses[7 * i]!, poses[7 * i + 1]!, poses[7 * i + 2]!, poses[7 * i + 3]!],
  translation: [poses[7 * i + 4]!, poses[7 * i + 5]!, poses[7 * i + 6]!],
});

/**
 * The overrides that show `jog` on `runtime` at its sample: one per link of the robot that the
 * bake has, each moving the link and what hangs on it (not its child links, which have their
 * own).
 */
export function jogOverrides(
  session: EtendueScene,
  runtime: FrameTreeRuntime,
  scene: SceneSpec,
  frames: readonly string[],
  base: string,
  jog: Jog,
): PoseOverride[] {
  const links = session.robotLinks(jog.robot);
  const poses = session.robotLinkPoses(jog.robot, jog.q);
  const worldBase = runtime.pose(base, jog.k);
  if (!worldBase) return [];
  const known = new Set(frames);
  const out: PoseOverride[] = [];
  links.forEach((link, i) => {
    const name = `${jog.robot}/${link}`;
    const baked = known.has(name) ? runtime.pose(name, jog.k) : undefined;
    if (!baked) return;
    const now = composeIso3(worldBase, isoAt(poses, i));
    out.push({
      parent: "world",
      local: composeIso3(now, invertIso3(baked)),
      moves: new Set([name, ...hangingOn(scene, frames, name)]),
    });
  });
  return out;
}

/** The robot's joints at sample `k` of the bake. */
export function bakedQ(
  baked: { robots: { id: string }[]; samples: { joint_positions: number[][] }[] },
  robot: string,
  k: number,
): number[] | null {
  const r = baked.robots.findIndex((x) => x.id === robot);
  const s = baked.samples[Math.min(k, baked.samples.length - 1)];
  return r < 0 || !s ? null : [...s.joint_positions[r]!];
}

/** `scenario` with a PTP move to the jogged pose (its joints stored) and, if asked, a capture. */
export function teach(
  scenario: ScenarioSpec | null,
  robot: string,
  q: number[],
  baseSe3Tool: Iso3Schema,
  capture: boolean,
): ScenarioSpec {
  const steps: Step[] = [
    ...(scenario?.steps ?? []),
    { type: "ptp_pose", robot, base_se3_tool: baseSe3Tool, q },
    ...(capture ? [{ type: "capture" } as const] : []),
  ];
  return { ...(scenario ?? { version: 1, dt: 0.01 }), steps };
}

/**
 * A Cartesian jog of `tcp` (in the base frame): `delta` translates along, and rotates about,
 * the base's axes (`base`) or the tool's own (`tool`), about the TCP's origin either way.
 */
export function jogged(tcp: Iso3, delta: Iso3, frame: "base" | "tool"): Iso3 {
  if (frame === "tool") return composeIso3(tcp, delta);
  // Rotate in base axes about the TCP: R' = ΔR·R, t' = t + Δt.
  const rotated = composeIso3(
    { rotation: delta.rotation, translation: [0, 0, 0] },
    { rotation: tcp.rotation, translation: [0, 0, 0] },
  );
  return {
    rotation: rotated.rotation,
    translation: [
      tcp.translation[0] + delta.translation[0],
      tcp.translation[1] + delta.translation[1],
      tcp.translation[2] + delta.translation[2],
    ],
  };
}

/** The steps that move `robot` (PTP and LIN), by index, for people. */
export function motionSteps(scenario: ScenarioSpec | null, robot: string): { index: number; label: string }[] {
  return (scenario?.steps ?? []).flatMap((s, index) =>
    (s.type === "ptp_pose" || s.type === "ptp_joints" || s.type === "lin") && s.robot === robot
      ? [{ index, label: `${index + 1}: ${s.type === "lin" ? "LIN" : "PTP"}` }]
      : [],
  );
}

/**
 * Step `step` re-taught at the jogged pose: a PTP keeps the joints (and a pose target its tool
 * pose too), a LIN takes the tool pose. Other steps do not move a robot: `null`.
 */
export function reteach(step: Step, q: number[], baseSe3Tool: Iso3Schema): Step | null {
  switch (step.type) {
    case "ptp_pose":
      return { ...step, base_se3_tool: baseSe3Tool, q };
    case "ptp_joints":
      return { ...step, q };
    case "lin":
      return { ...step, base_se3_tool: baseSe3Tool };
    default:
      return null;
  }
}
