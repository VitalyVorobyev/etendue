# G1.3 — Robot asset mesh round-trip

- Date: 2026-09-26
- etendue commit: `4333b1c` (`git describe --always --dirty` at measurement time)
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

Base frame: robot.json `base_link` is the REP-199 `base` link, the frame the controller
reports TCP poses in (ADR 0002). For UR5e it is `base_link`·Rz(π); for the ABB arm it is
identical to `base_link`. It is a fixed-joint sibling of the arm, not an ancestor of the TCP.
The GLBs are in each link's own frame, so this choice does not affect G1.3.

The meshes are git-ignored (licence permits redistribution, but they are regenerated
byte-identically from the pinned SHAs instead of being committed).

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
