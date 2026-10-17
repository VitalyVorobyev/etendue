# G1.3 — Robot asset mesh round-trip

- Date: 2026-10-17
- etendue commit: `egui-ui-final-21-gf05a157-dirty` (`git describe --always --dirty` at measurement time)
- Result: **PASS** — overall max abs vertex error 2.980e-08 m (gate ≤ 1e-06 m)

## Criterion

Ticket P1-2 (docs/pivot/PLAN.md §5): per-mesh vertex positions round-trip within ≤ 1e-6 m of
the source mesh. For every link GLB written by `tools/robot-assets/build.py`, the GLB is
reloaded from disk with trimesh (`process=False`) and each face corner's position (GLB node
transform applied) is compared with the same corner of the source mesh, freshly reloaded and
transformed by the URDF `<visual><origin>`, mesh `scale` and COLLADA node transform / `<unit>`.
The expected positions use a separate code path from the builder (explicit Rz·Ry·Rx matrices,
unit read with pycollada instead of lxml). Face counts and corner order must match exactly;
the value reported is the max absolute coordinate difference in metres.

Source precision: pycollada hard-codes float32 when parsing `<float_array>` and node
transforms, which would quantise the source before the comparison and hide that error. Both
the builder and the gate therefore parse COLLADA in float64 (see `collada_float64` in
`build.py`), and the gate additionally asserts that every loaded source vertex is an exact
row of a POSITION `<float_array>` parsed from the XML text with lxml. The only remaining
error is the single float32 rounding of glTF positions (half-ULP ≈ 3e-8 m at 0.5 m).

## Pinned sources

| Robot | Repository | Revision | URDF root | base_link -> tcp_link |
|---|---|---|---|---|
| `ur5e` (Universal Robots UR5e) | https://github.com/UniversalRobots/Universal_Robots_ROS2_Description | `ae333289875f9ba5a9ea6649a54036efb5ccabee` | `world` | `base` -> `tool0` |
| `abb_irb1200_5_90` (ABB IRB 1200-5/0.90) | https://github.com/ros-industrial/abb | `45f4769d826cf3ac62a65495f2db67b78b0c81df` | `base_link` | `base` -> `tool0` |
| `igus_rebel_6dof` (igus ReBeL 6DOF) | https://github.com/CommonplaceRobotics/iRC_ROS | `41701e6df3398087d217f0f478622e60f694de41` | `world` | `base_link` -> `tool0` |
| `fairino_fr5` (FAIRINO FR5) — **local only** | https://github.com/FAIR-INNOVATION/frcobot_ros2 | `fcf0c7f0d60d949d8a9a4238f929a44d07f60379` | `base_link` | `base_link` -> `tool0` |

Base frame: robot.json `base_link` is the REP-199 `base` link, the frame the controller
reports TCP poses in (ADR 0002). For UR5e it is `base_link`·Rz(π), a fixed-joint sibling of
the arm; for the ABB arm it is identical to `base_link`. The igus ReBeL and the FAIRINO FR5
descriptions have no `base` link; their `base_link` is the controller frame. The GLBs are in
each link's own frame, so this choice does not affect G1.3.

The meshes are git-ignored (licence permits redistribution, but they are regenerated
byte-identically from the pinned SHAs instead of being committed). A **local only** robot
has no redistribution licence: nothing of it is committed or bundled; it is built with
`--include-local` into the git-ignored `assets/robots-local/`, and its rows here are the
gate run on the measuring machine.

## Per-mesh results

### `ur5e` — max 1.490e-08 m (URDF root link: `world`)

| Link | Source mesh (repo-relative) | Source vertices | GLB vertices (welded) | Max abs error (m) |
|---|---|---:|---:|---:|
| `base_link_inertia` | `meshes/ur5e/visual/base.dae` | 14514 | 14314 | 3.724e-09 |
| `shoulder_link` | `meshes/ur5e/visual/shoulder.dae` | 69870 | 69803 | 3.722e-09 |
| `upper_arm_link` | `meshes/ur5e/visual/upperarm.dae` | 120372 | 120174 | 1.489e-08 |
| `forearm_link` | `meshes/ur5e/visual/forearm.dae` | 45174 | 44875 | 1.490e-08 |
| `wrist_1_link` | `meshes/ur5e/visual/wrist1.dae` | 51864 | 51656 | 3.717e-09 |
| `wrist_2_link` | `meshes/ur5e/visual/wrist2.dae` | 60840 | 60420 | 1.863e-09 |
| `wrist_3_link` | `meshes/ur5e/visual/wrist3.dae` | 2733 | 2663 | 1.860e-09 |

### `abb_irb1200_5_90` — max 2.980e-08 m (URDF root link: `base_link`)

| Link | Source mesh (repo-relative) | Source vertices | GLB vertices (welded) | Max abs error (m) |
|---|---|---:|---:|---:|
| `base_link` | `abb_irb1200_support/meshes/irb1200_5_90/visual/base_link.dae` | 42858 | 9925 | 7.444e-09 |
| `link_1` | `abb_irb1200_support/meshes/irb1200_5_90/visual/link_1.dae` | 54480 | 14774 | 7.446e-09 |
| `link_2` | `abb_irb1200_support/meshes/irb1200_5_90/visual/link_2.dae` | 60375 | 13840 | 2.974e-08 |
| `link_3` | `abb_irb1200_support/meshes/irb1200_5_90/visual/link_3.dae` | 31080 | 7484 | 1.483e-08 |
| `link_4` | `abb_irb1200_support/meshes/irb1200_5_90/visual/link_4.dae` | 53172 | 11948 | 2.980e-08 |
| `link_5` | `abb_irb1200_support/meshes/irb1200_5_90/visual/link_5.dae` | 33834 | 7851 | 3.717e-09 |
| `link_6` | `abb_irb1200_support/meshes/irb1200_5_90/visual/link_6.dae` | 2880 | 850 | 9.300e-10 |

### `igus_rebel_6dof` — max 1.490e-08 m (URDF root link: `world`)

| Link | Source mesh (repo-relative) | Source vertices | GLB vertices (welded) | Max abs error (m) |
|---|---|---:|---:|---:|
| `base_link` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint0.dae` | 5406 | 1482 | 3.714e-09 |
| `link_1` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint1.dae` | 32880 | 6179 | 7.445e-09 |
| `link_2` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint2.dae` | 65628 | 12102 | 1.490e-08 |
| `link_5` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint3.dae` | 32136 | 5968 | 4.768e-09 |
| `link_6` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint4.dae` | 45540 | 8277 | 7.449e-09 |
| `link_7` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint5.dae` | 43824 | 7929 | 3.409e-09 |
| `link_8` | `irc_ros_description/meshes/igus_rebel_6dof_00/Joint6.dae` | 372 | 130 | 8.902e-10 |

### `fairino_fr5` — max 0.000e+00 m (URDF root link: `base_link`)

| Link | Source mesh (repo-relative) | Source vertices | GLB vertices (welded) | Max abs error (m) |
|---|---|---:|---:|---:|
| `base_link` | `fairino_description/meshes/fairino5_v6/base_link.STL` | 7194 | 1218 | 0.000e+00 |
| `shoulder_link` | `fairino_description/meshes/fairino5_v6/shoulder_link.STL` | 26130 | 4359 | 0.000e+00 |
| `upperarm_link` | `fairino_description/meshes/fairino5_v6/upperarm_link.STL` | 32076 | 5364 | 0.000e+00 |
| `forearm_link` | `fairino_description/meshes/fairino5_v6/forearm_link.STL` | 27042 | 4503 | 0.000e+00 |
| `wrist1_link` | `fairino_description/meshes/fairino5_v6/wrist1_link.STL` | 27144 | 4520 | 0.000e+00 |
| `wrist2_link` | `fairino_description/meshes/fairino5_v6/wrist2_link.STL` | 27534 | 4585 | 0.000e+00 |
| `wrist3_link` | `fairino_description/meshes/fairino5_v6/wrist3_link.STL` | 28326 | 4723 | 0.000e+00 |

Source vertices are counted as trimesh loads them (`process=False`; COLLADA triangles are
unshared, three vertices per face). GLB vertices are after exact welding of identical
position+normal+UV rows.

## Tool versions

- python: 3.12.12
- trimesh: 5.1.0
- pycollada: 0.9.3
- numpy: 2.5.3
- lxml: 6.1.3
- xacro: 2.1.1
- pyyaml: 6.0.3
- git: 2.54.0 (Apple Git-157)

## Reproduce

```bash
uv run --locked --project tools/robot-assets tools/robot-assets/build.py --report docs/measurements/g1_3_robot_assets.md
```
