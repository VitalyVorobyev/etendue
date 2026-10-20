# Robots

A robot in etendue is three things in a folder:

- `robot.urdf`, the kinematic description;
- `robot.json`, the manifest: its base and tool links, its joints in controller order with their
  limits, and its link meshes;
- `meshes/<link>.glb`, one mesh per link, in the link's frame.

The kinematics — forward and inverse, the Jacobian and manipulability — are described in
[Robot kinematics](models/kinematics.md), and the timing of moves in [Motion](models/motion.md).

## The library

| Robot | Id | Description from | Licence |
|---|---|---|---|
| Universal Robots UR5e | `ur5e` | Universal_Robots_ROS2_Description | BSD-3-Clause |
| ABB IRB 1200-5/0.90 | `abb_irb1200_5_90` | ros-industrial/abb | Apache-2.0 |
| igus ReBeL 6DOF | `igus_rebel_6dof` | CommonplaceRobotics/iRC_ROS | Apache-2.0 |
| FAIRINO FR5 | `fairino_fr5` | FAIR-INNOVATION/frcobot_ros2 | none: **local only** |

The desktop app and the command-line archives carry the first three, with their meshes. In the repository, `robot.json` and
`robot.urdf` are in `assets/robots/<id>/`, and the meshes are rebuilt from the upstream
descriptions (see [Installation](installation.md#the-robot-meshes)).

The FAIRINO description has no licence, so it may not be redistributed. It is built only on your
own machine, into `assets/robots-local/`, and never shipped in the desktop app. A studio built
from source lists it with "(local only)". Its description stops at the wrist; the tool flange
`tool0` is added 0.1 m along the last link's axis, which has not been checked against FAIRINO's
controller. Check it before sending poses to a real FR5.

In the studio, **+** in the scene tree adds a library robot, attached to the selected frame, and
**+ › Robot library…** compares the robots' joints, limits and reach.

## Base and tool frames

The **base link** is the frame tool poses are given in: `ptp_pose` and `lin` targets, the
exported capture poses, and `robot_poses.json` in a dataset. It is the frame your controller
reports poses in. For the UR5e and the ABB it is the `base` link (on the UR5e it is turned half
a turn about Z from `base_link`; on the ABB the two coincide); for the igus ReBeL it is
`base_link`.

The **tool link** is the flange, `tool0` for every robot in the library. A tool or a sensor
mounted on the flange is a frame, rig or camera attached to `<robot>/tool0` with its offset as
the pose.

## Limits

Every joint has position, velocity, acceleration and jerk limits, and the manifest records
where each number comes from (`limit_source`):
- position and velocity come from the robot's URDF;
- acceleration comes from the vendor's or the community's MoveIt configuration where one exists,
  else from a documented default;
- no vendor publishes jerk limits, so jerk is derived as 10/s times the acceleration limit: an
  acceleration ramp takes 0.1 s.

Moves never exceed these limits; `speed_scale` uses a fraction of them. Check them against your
controller before relying on cycle times.

## Adding a robot

Robots are built from a pinned upstream ROS description by `tools/robot-assets/build.py`
(Python, run with [uv](https://docs.astral.sh/uv/)), in a source checkout
([Installation](installation.md#from-source)). To add one:

1. **Find the description.** You need a URDF or xacro, its meshes (COLLADA `.dae` or STL), and
   its licence. Note the commit you use: the build is pinned to a full commit SHA and refuses
   anything else.
2. **Add a `[[robot]]` entry to `tools/robot-assets/robots.toml`.** The file's header documents
   every field. The essential ones:

   ```toml
   [[robot]]
   id = "my_arm"                       # robot.json id, [A-Za-z0-9_-]+
   name = "Vendor Arm 7"
   repository = "https://github.com/vendor/arm_description"
   revision = "<40-hex commit SHA>"
   entry = "arm_description/urdf/arm.urdf.xacro"   # or a plain .urdf
   packages = { arm_description = "arm_description" }   # $(find …) and package:// roots
   base_link = "base_link"             # the frame the controller reports tool poses in
   tcp_link = "tool0"                  # the flange
   license_files = ["LICENSE"]
   distribution = "redistributable"    # or "local" without a redistribution licence

   [robot.xacro_args]                  # xacro name:=value, if any

   [robot.acceleration]                # where acceleration limits come from
   kind = "yaml"
   repository = "https://github.com/vendor/arm_moveit_config"
   revision = "<40-hex commit SHA>"
   file = "config/joint_limits.yaml"
   key = "joint_limits.<joint>.max_acceleration"
   # or: kind = "default", value = 5.0, reason = "why there is no upstream number"

   [robot.jerk]
   kind = "derived"
   factor = 10.0
   reason = "no published jerk limit"

   [robot.license]
   urdf = "Apache-2.0"
   meshes = "Apache-2.0"
   meshes_redistributable = true
   notes = "where the licence evidence is, at the pinned commit"
   ```

   If the description lacks the controller's tool frame, add it with `[[robot.frames]]`
   (`name`, `parent`, `xyz`, `rpy`, `reason`).
3. **Build it:**

   ```bash
   uv run --locked --project tools/robot-assets tools/robot-assets/build.py --robot my_arm
   ```

   This writes `assets/robots/my_arm/` (with `distribution = "local"`, add `--include-local`;
   it then goes to `assets/robots-local/my_arm/`).
4. **Try it.** Name it in a scene, by the path to its manifest relative to the scene:
   `{"id": "arm", "parent": "world", "manifest": "../../assets/robots/my_arm/robot.json", …}`,
   and run `etendue validate` on the scene and a scenario. A studio built from source (`cargo app`) also
   offers it under **+** in the scene tree.
5. **Check it against the real robot.** Move it to a few joint configurations and compare the
   tool pose the studio's **Jog** panel shows with your controller's. A wrong base or tool frame
   shows up here first.

Use the base link your controller reports poses in: for a ROS-Industrial description, the
`base` link where there is one. Changes to limits belong in `robots.toml`, never in a built
`robot.json`, which the next build overwrites.

Contributing a robot to etendue's library also takes the kinematics checks described in the
repository's `CONTRIBUTING.md`.
