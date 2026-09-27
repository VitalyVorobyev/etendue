/**
 * React Three Fiber components over `@vitavision/three` (etendue PLAN P2-3). Per-frame
 * updates go through `useFrame` and refs, never React state: playback is read from a
 * {@link PlayheadSource} once per rendered frame.
 *
 * @packageDocumentation
 */

export { SceneColorsProvider, type SceneColorsProviderProps, useSceneColors } from "./colors";
export {
  AtFrame,
  type AtFrameProps,
  FrameTree,
  type FrameTreeProps,
  type PlayheadSource,
  useFrameTree,
} from "./FrameTree";
export {
  CameraFrustum,
  type CameraFrustumProps,
  FrameAxes,
  type FrameAxesProps,
  LaserFan,
  type LaserFanProps,
  LightGizmo,
  type LightGizmoProps,
  TargetBoard,
  type TargetBoardProps,
} from "./gizmos";
export { Robot, type RobotProps } from "./Robot";
export { SensorImage, type SensorImageProps } from "./SensorImage";
export { SceneCanvas, type SceneCanvasProps } from "./SceneCanvas";
