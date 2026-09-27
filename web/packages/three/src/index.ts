/**
 * Framework-agnostic three.js building blocks for robot-cell scenes (etendue PLAN P2-2):
 * conventions, a runtime that applies baked scenarios, robot visuals, and gizmos for
 * cameras, lasers, targets, and lights. No React — see `@vitavision/three-react`.
 *
 * No kinematics and no camera-model math live here: poses come baked, and camera fields of
 * view come as back-projected rays from the host (e.g. `@etendue/wasm`).
 *
 * @packageDocumentation
 */

export {
  CV_TO_GL,
  IDENTITY_ISO3,
  type Iso3,
  type Iso3Wire,
  Z_UP,
  composeIso3,
  glCameraMatrix,
  invertIso3,
  iso3FromMatrix,
  matrixFromIso3,
  quaternionFromRpy,
  rpyFromQuaternion,
} from "./conventions";
export { disposeObject } from "./dispose";
export {
  type BakedSampleLike,
  type BakedScenarioLike,
  type CaptureMarker,
  FrameTreeRuntime,
  WORLD,
} from "./frameTree";
export { Axes, type AxesColors } from "./primitives/axes";
export { CameraFrustum, type CameraFrustumOptions, imageBorderPixels } from "./primitives/frustum";
export { LaserFan, type LaserFanOptions } from "./primitives/laserFan";
export { LightGizmo, type LightShapeLike } from "./primitives/lightGizmo";
export { TargetBoard, type TargetBoardOptions } from "./primitives/targetBoard";
export {
  type MeshLoader,
  type RobotVisual,
  type RobotVisuals,
  applyRobotMaterial,
  attachRobotVisuals,
  gltfMeshLoader,
  loadRobotVisuals,
} from "./robot";
export { type SceneColors, observeSceneColors, readSceneColors } from "./theme";
