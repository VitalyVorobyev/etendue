# Robots

A robot in etendue is three files in a folder of the robot library:

- `robot.urdf`, the kinematic description;
- `robot.json`, the manifest: its base and tool links, its joints in controller order with their
  limits, and its link meshes;
- `meshes/<link>.glb`, one mesh per link, in the link's frame.

## The library

| Robot | Id | Source | Licence |
|---|---|---|---|
| Universal Robots UR5e | `ur5e` | Universal_Robots_ROS2_Description | BSD-3-Clause |
| ABB IRB 1200-5/0.90 | `abb_irb1200_5_90` | ros-industrial/abb | Apache-2.0 |
| igus ReBeL 6DOF | `igus_rebel_6dof` | CommonplaceRobotics/iRC_ROS | Apache-2.0 |
| FAIRINO FR5 | `fairino_fr5` | FAIR-INNOVATION/frcobot_ros2 | none: **local only** |

The FAIRINO description has no licence, so it may not be redistributed. It is built only on your
machine (`--include-local`, below), into `assets/robots-local/`, which is never committed or
shipped in the desktop app. A development build of the studio lists it with "(local only)". Its
description stops at the wrist centre; the tool flange `tool0` is added 0.1 m along the last
link's axis, which has not been checked against FAIRINO's controller. Check it before sending
poses to a real FR5.

In the studio, **+** in the scene tree adds a library robot, attached to the selected frame.

## Limits

Every joint has position, velocity, acceleration and jerk limits, and the manifest records where
each number comes from (`limit_source`). Position and velocity come from the URDF. Acceleration
comes from the vendor's or the community's MoveIt configuration where one exists, else from a
documented default. No vendor publishes jerk limits, so jerk is derived: 10/s times the
acceleration limit, so an acceleration ramp takes 0.1 s. Change a limit in
`tools/robot-assets/robots.toml`, never in `robot.json`.

## Adding a robot

Robots are built from a pinned upstream ROS description by `tools/robot-assets/build.py` (Python,
run with [uv](https://docs.astral.sh/uv/)). To add one:

1. **Find the description.** You need a URDF or xacro, its meshes (COLLADA `.dae` or STL), and its
   licence. Note the commit you use: the build is pinned to a full commit SHA and refuses anything
   else.
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
3. **Build it and check the meshes** (gate G1.3: every mesh vertex within 1e-6 m of the source):

   ```bash
   uv run --locked --project tools/robot-assets tools/robot-assets/build.py --robot my_arm \
       [--include-local] --report docs/measurements/g1_3_robot_assets.md
   ```

4. **Check the kinematics** against Pinocchio (G1.1) and the inverse kinematics (G1.2):

   ```bash
   uv run tools/fixtures/fk_fixture.py --robot my_arm [--include-local]
   cargo test -p etendue-kinematics --all-features --test gates -- --nocapture
   ```

5. **Try it**: add it in the studio's scene tree, or name it in a scene:
   `{"id": "arm", "parent": "world", "manifest": "../../assets/robots/my_arm/robot.json", …}`.

The base link matters: it is the frame tool poses are given in (`ptp_pose`, `lin`, exported
captures). Use the frame your controller reports poses in — for a ROS-Industrial description,
the REP-199 `base` link where there is one.
