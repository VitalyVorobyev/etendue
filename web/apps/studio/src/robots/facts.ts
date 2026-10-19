/**
 * What a robot of the library offers, for choosing one: its joints and their limits, and its
 * reach, from the kernel's forward kinematics over joint vectors spread through the limits.
 */

import { EtendueScene } from "@etendue/wasm";

import type { LoadedRobot } from "../io/load";
import { IDENTITY } from "../scene/edit";

/** Joint vectors the reach is taken over. */
const SAMPLES = 4096;

export interface RobotFacts {
  dof: number;
  joints: { name: string; lower: number; upper: number; max_velocity: number }[];
  /** The farthest the TCP gets from the base over the samples (m). */
  reach_m: number;
  /** How far the TCP is from the base with every joint at 0 or its nearest limit (m). */
  home_m: number;
  meshes_redistributable: boolean;
}

/** A deterministic generator in [0, 1) (mulberry32), so the reach is the same every time. */
function uniform(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export function robotFacts(robot: LoadedRobot): RobotFacts {
  const joints = robot.manifest.joints.map(({ name, lower, upper, max_velocity }) => ({
    name,
    lower,
    upper,
    max_velocity,
  }));
  const session = new EtendueScene(
    {
      version: 1,
      robots: [{ id: robot.id, parent: "world", parent_se3_self: IDENTITY, manifest: "robot.json" }],
    },
    [{ id: robot.id, manifest: robot.manifest, urdf: robot.urdf }],
  );
  try {
    const distance = (q: number[]) => {
      const t = session.robotTcp(robot.id, q).base_se3_tcp.translation;
      return Math.hypot(t[0], t[1], t[2]);
    };
    const home = joints.map((j) => Math.min(Math.max(0, j.lower), j.upper));
    const next = uniform(0x5eed);
    let reach = distance(home);
    for (let s = 0; s < SAMPLES; s++) {
      reach = Math.max(reach, distance(joints.map((j) => j.lower + next() * (j.upper - j.lower))));
    }
    return {
      dof: joints.length,
      joints,
      reach_m: reach,
      home_m: distance(home),
      meshes_redistributable: robot.manifest.license.meshes_redistributable,
    };
  } finally {
    session.free();
  }
}
