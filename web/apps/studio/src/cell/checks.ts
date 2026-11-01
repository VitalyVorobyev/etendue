/**
 * What the kernel says about a cell being built, on a throwaway session of its scene at the rest
 * pose: whether the scene holds together, what each camera sees of the target, and joints that
 * put the cameras (eye-in-hand) or the target (eye-to-hand) at the working distance.
 */

import type { BakedScenario, IkAnswer, RobotSource, SceneSpec } from "@etendue/wasm";
import { type Iso3, composeIso3, invertIso3 } from "@vitavision/three";

import { describeFailure, withScene } from "../kernel/etendue";

/** What one camera sees of the target at the start pose. */
export interface View {
  camera: string;
  /** Target points in its image, of `total`. */
  seen: number;
  total: number;
  /** The target's origin along the camera's axis, metres (negative: behind it). */
  distance_m: number | null;
}

export interface CellChecks {
  /** Why the kernel refuses the scene; `null` when it takes it. */
  problem: string | null;
  views: View[];
  /** The target's size `[x, y]`, metres, when its geometry says. */
  extent: [number, number] | null;
}

function poseOf(baked: BakedScenario, frame: string): Iso3 | null {
  const i = baked.frames.indexOf(frame);
  const pose = i < 0 ? undefined : baked.samples[0]?.world_se3_frame[i];
  return pose ? { rotation: [...pose.rotation], translation: [...pose.translation] } : null;
}

function failure(e: unknown): string {
  const f = describeFailure(e);
  const issue = f.issues[0];
  return issue ? `${issue.path}: ${issue.message}` : f.message;
}

/** The checks of `scene` (the target `target`) with `robots`. */
export function checkCell(scene: SceneSpec, robots: readonly RobotSource[], target: string): CellChecks {
  try {
    return withScene(scene, robots, (session, baked) => {
      const world_se3_target = poseOf(baked, target);
      const extent = session.targetExtent(target) ?? null;
      const views = (scene.cameras ?? []).map((c): View => {
        const world_se3_camera = poseOf(baked, c.id);
        if (!world_se3_camera || !world_se3_target) return { camera: c.id, seen: 0, total: 0, distance_m: null };
        const uv = session.projectTarget(c.id, world_se3_camera, target, world_se3_target);
        let seen = 0;
        for (let i = 0; i < uv.length; i += 2) if (!Number.isNaN(uv[i]!)) seen++;
        const camera_se3_target = composeIso3(invertIso3(world_se3_camera), world_se3_target);
        return { camera: c.id, seen, total: uv.length / 2, distance_m: camera_se3_target.translation[2] };
      });
      return { problem: null, views, extent };
    });
  } catch (e) {
    return { problem: failure(e), views: [], extent: null };
  }
}

/** Which robot to aim, and at what. */
export interface Aim {
  robot: string;
  baseLink: string;
  tcpLink: string;
  rig: string;
  target: string;
  /** The rig on the flange (eye-in-hand), or the target on it (eye-to-hand). */
  eyeInHand: boolean;
  /** Rig-to-target distance along the rig's axis, metres. */
  distance: number;
  /** Where the solver starts. */
  seed: readonly number[];
}

/**
 * Joints that face the rig and the target at `aim.distance` along the rig's +Z, the target's
 * printed side (+Z) towards the rig and its x along the rig's x: the robot carries the rig over
 * the target (eye-in-hand) or holds the target in front of the rig (eye-to-hand). Damped least
 * squares from `aim.seed`, with restarts; a pose out of reach gives `converged: false`. A string
 * when the scene does not hold together.
 */
export function aimRobot(scene: SceneSpec, robots: readonly RobotSource[], aim: Aim): IkAnswer | string {
  try {
    return withScene(scene, robots, (session, baked) => {
      const tcp = poseOf(baked, `${aim.robot}/${aim.tcpLink}`);
      const base = poseOf(baked, `${aim.robot}/${aim.baseLink}`);
      const rig = poseOf(baked, aim.rig);
      const target = poseOf(baked, aim.target);
      if (!tcp || !base || !rig || !target) return "The cell lacks the robot, the rig or the target.";
      // Along +Z by the distance, then turned half about x: facing back.
      const facing: Iso3 = { rotation: [1, 0, 0, 0], translation: [0, 0, aim.distance] };
      let world_se3_tcp: Iso3;
      if (aim.eyeInHand) {
        const tcp_se3_rig = composeIso3(invertIso3(tcp), rig);
        world_se3_tcp = composeIso3(composeIso3(target, facing), invertIso3(tcp_se3_rig));
      } else {
        const tcp_se3_target = composeIso3(invertIso3(tcp), target);
        world_se3_tcp = composeIso3(composeIso3(rig, facing), invertIso3(tcp_se3_target));
      }
      return session.robotIk(aim.robot, composeIso3(invertIso3(base), world_se3_tcp), aim.seed);
    });
  } catch (e) {
    return failure(e);
  }
}
