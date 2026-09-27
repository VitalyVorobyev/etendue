# @vitavision/three

Framework-agnostic three.js building blocks for robot-cell scenes: the frame and pose
conventions, a runtime that plays baked scenarios, robot visuals, and gizmos for cameras,
lasers, targets, and lights. No React; see `@vitavision/three-react` for the R3F layer.

> **Incubating in etendue** (`web/packages/three`, `private`). It moves to lab-ui by PR
> (lab-ui PLAN L8-1) once the etendue studio has settled its API.

```ts
import { FrameTreeRuntime, CameraFrustum, imageBorderPixels } from "@vitavision/three";

const runtime = new FrameTreeRuntime(baked); // an etendue BakedScenario
scene.add(runtime.root);
runtime.frame("cam_left")!.add(new CameraFrustum({ borderRays, depth: 0.12, color }));
runtime.apply(k); // per animation frame: poses only, no kinematics
```

**No kinematics and no camera math here.** Poses arrive baked (etendue ADR 0003), and a
camera's field of view arrives as back-projected border rays from the host, e.g.
`@etendue/wasm`'s `backprojectPixels(imageBorderPixels(w, h))` — so distortion shows and
nothing re-implements a camera model.

| Module | What |
|---|---|
| `conventions` | `Iso3Wire` (`{rotation: [qx,qy,qz,qw], translation}`), `matrixFromIso3`, `composeIso3`, `invertIso3`, `CV_TO_GL` (Rx(π)), `glCameraMatrix`, `Z_UP`, URDF roll-pitch-yaw |
| `FrameTreeRuntime` | one `Object3D` per baked frame, `apply(k)`, `pose(frame, k)`, capture markers |
| `robot` | `loadRobotVisuals` (per-link GLB, failures reported not thrown), `attachRobotVisuals`, `applyRobotMaterial` |
| primitives | `CameraFrustum`, `LaserFan`, `TargetBoard`, `LightGizmo`, `Axes` |
| `theme` | `readSceneColors` / `observeSceneColors`: scene colours from the `@vitavision/ui` tokens |

Frames: world +Z up, metres; camera frames are OpenCV (+Z forward, +Y down); lasers fan in
their `x = 0` plane about +Z; targets lie in `z = 0` facing +Z; lights emit along +Z.
