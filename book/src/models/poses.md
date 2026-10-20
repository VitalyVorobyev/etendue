# Pose generation

A good calibration dataset sees the board from many directions: tilted views constrain the focal
length and the principal point, varied rotations make the hand-eye problem well posed, and views
across the image constrain distortion. Placing 20–50 such views by hand, each one reachable by the
robot and showing the whole board to every camera, is tedious. etendue's **sphere pose
generator** places the sensor on spheres around the target, keeps the placements that work, and
orders them into a short stop-and-shoot program.

## Where the sensor goes

The sensor is any frame the robot's tool carries rigidly: a rig, a camera, or a frame on the tool
(`look`, optionally offset by `look_se3_sensor`). The target defines a sphere centre (by default
the target's centre; a PuzzlePole's axis midpoint) and a **pole** (by default the board's normal,
+Z; for a PuzzlePole, +X, the seam's outward normal). For every combination of

- a radius $r$ (`radii_m`),
- a polar angle $\theta$ from the pole (`theta`: `steps` angles from `min_rad` to `max_rad`),
- an azimuth $\varphi$ about the pole (`phi`; a full turn counts its start once),
- a roll $\psi$ about the sensor's axis (`roll_rad`, default `[0]`),

the sensor is placed at

$$
\mathbf{c} + r\,R_{\text{pole}}
\begin{pmatrix} \sin\theta\cos\varphi \\ \sin\theta\sin\varphi \\ \cos\theta \end{pmatrix}
$$

in the target's frame, with its +Z pointing at the centre and its +Y along the target's
"down" (the board's −Y), so that the images read upright, then rolled by $\psi$ about +Z. The
tool pose that puts the sensor there follows from the tool-to-sensor transform.

## Which candidates are kept

Each placement is a candidate. It is kept when:

1. **the robot reaches it**: inverse kinematics finds a configuration (see [Robot
   kinematics](kinematics.md));
2. **the configuration is comfortable**: every joint is at least `joint_margin_rad` (default 0.05
   rad) from its limits, and the tool Jacobian's smallest singular value is at least `min_sigma`
   (default 0.02);
3. **every required camera sees the target**: at least `min_visible_fraction` of its points
   (default all of them), each in front of the camera, at least `margin_px` (default 2 px) inside
   the image, and on a side of the target facing the camera. These are the ground truth's own
   visibility tests.

A rejected candidate keeps its reason: `unreachable`, `near_limit` (which joint, by how much),
`near_singular` (the singular value), or `not_visible` (which camera, how many points of how
many). `etendue scenario sphere --candidates` writes them all; the studio's generator form shows
the count kept as you edit.

## Staying on one branch

Inverse kinematics visits the candidates in a "snake" order (radius, then $\theta$, then $\varphi$
back and forth), each solve seeded from the previous solution without restarts, so neighbouring
views stay on one IK branch. Only if that fails does it retry from the scene's initial
configuration, and only then with restarts (such a candidate is flagged: it may be on another
branch). Every kept step stores the configuration it was solved to, so compiling the scenario
again never switches branch.

## The tour

The kept placements are ordered for a short program: nearest neighbour from the start
configuration, then 2-opt improvement (at most 50 passes), both measuring the distance between two
configurations as the slowest joint's travel time, $\max_i |\Delta q_i| / v_i$. Each becomes a
`ptp_pose` move (at `speed_scale`, default 0.5) followed by a capture; `return_home` adds a move
back to the start. The generator uses no random numbers: the same scene and spec always give the
same scenario. A scenario keeps its spec in `generator`, so it can be edited and run again
(`etendue scenario sphere <scene> --spec <scenario.json>`).

**Worked example.** The `closed_loop_ur5e` example's scenario is generated from radii 450 and
550 mm, $\theta$ = 0.14 and 0.44 rad (8° and 25°), six azimuths over a full turn, and rolls of
±0.35 rad (±20°), with both cameras required to see the whole 9 × 6 board: 2 × 2 × 6 × 2 = 48
candidates. 24 are kept; each of the other 24 has one or two corners outside one camera's image
(52 or 53 of the 54 points seen). The tour takes 22.4 s of motion. The studio's default spec (two
radii, three polar angles, eight azimuths: 48 candidates) is evaluated in about 10 ms, and 400
candidates for a three-camera rig in about 80 ms.

Try it: change the radius, the cone angle and the number of views, and look at the generated
poses around the board from above and from the side.

<div class="etendue-demo" data-demo="poses"></div>

## Assumptions and limits

- **Eye-in-hand only.** The sensor must ride on the robot's tool; the generator does not move a
  target held by the robot past fixed cameras.
- **No collision checking.** A kept pose can put the robot or the rig through the table or the
  board's stand.
- **Visibility is geometric.** Occlusion by other objects and lighting are not considered; the
  renders show them.
- **A good spread is your choice.** The generator keeps what is feasible; it does not choose
  poses to optimise the calibration's conditioning. Tilted views (larger $\theta$) and rolls are
  what constrain the intrinsics and the hand-eye rotation.

API: `etendue-synth` `poses::generate`; `etendue scenario sphere`; `@etendue/wasm`
`generatePoses`.
