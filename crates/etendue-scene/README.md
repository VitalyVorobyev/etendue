# etendue-scene

The document types of [etendue](https://github.com/VitalyVorobyev/etendue): versioned JSON
scenes, scenarios, baked scenarios and robot manifests.

- `SceneSpec`: a frame tree of robots, rigs, cameras (calibration-rs `CameraParams`), line lasers,
  lights, calibration boards and parts. `FrameGraph` resolves it; `validate` reports every issue.
- `ScenarioSpec`: robot moves (`ptp_joints`, `ptp_pose`, `lin`), `wait` and `capture` steps.
- `BakedScenario`: every frame's `world_se3_frame` at every sample, with the captures.
- `RobotManifest`: a URDF's base and tool links, limits and meshes (`robot.json`).

No I/O and no kinematics: those are in `etendue-kinematics`. JSON Schemas are derived behind the
`schemars` feature. Documentation: <https://vitalyvorobyev.github.io/etendue>.
