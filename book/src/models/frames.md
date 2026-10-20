# Frames and poses

A calibration cell is a chain of rigid transforms: the robot's base in the world, the flange on
the base, the camera on the flange, the board in the world. A calibration estimates some links of
that chain from images. Ground truth is only useful if every link means exactly what the
calibration tool thinks it means. etendue therefore uses one convention everywhere, and it is
calibration-rs's.

## Axes and units

- **World:** right-handed, +Z up. Lengths are in metres, angles in radians, unless a field's name
  says otherwise (`wavelength_nm`, `square_size_mm`).
- **Camera:** the OpenCV convention. +Z looks along the optical axis toward the scene, +X runs
  along the image rows (to the right in the image), +Y runs down the image.
- **Laser:** its fan lies in the laser's local $x = 0$ plane. The central ray is local +Z, and
  the line spreads toward ±Y. The plane's normal is local +X.
- **Planar target:** centred on its local origin, facing +Z (the printed side), its top edge
  toward +Y. Reading the print from the top, "down the page" is −Y.
- **PuzzlePole:** a cylinder about local +Z, the axis starting at the first corner column, the
  seam at +X, the print facing outward.
- **Robot base:** the URDF's `base` link (the REP-199 convention). It is the frame the robot
  controller reports tool poses in. The **TCP** is the link the manifest names `tcp_link`
  (`tool0` for the UR5e).

## Transforms: `a_se3_b`

A pose is a rigid transform written `a_se3_b`. It maps coordinates of a point in frame `b` into
frame `a`:

$$
\mathbf{p}_a = {}^{a}T_{b}\,\mathbf{p}_b = R\,\mathbf{p}_b + \mathbf{t}.
$$

Read the name right to left: `world_se3_camera` takes a point from camera coordinates to world
coordinates, so its translation is the camera's position in the world. Names chain like
fractions cancel:

$$
{}^{\text{world}}T_{\text{cam}} =
{}^{\text{world}}T_{\text{base}}\;
{}^{\text{base}}T_{\text{tcp}}\;
{}^{\text{tcp}}T_{\text{rig}}\;
{}^{\text{rig}}T_{\text{cam}},
\qquad
{}^{b}T_{a} = \left({}^{a}T_{b}\right)^{-1}.
$$

On the wire, every pose is a unit quaternion (scalar last) and a translation in metres:

```json
{ "rotation": [qx, qy, qz, qw], "translation": [tx, ty, tz] }
```

This is the serialisation calibration-rs reads and writes, so a pose copied from a dataset into a
scene (or back) needs no conversion.

## The frame tree

Every entity in a scene (camera, rig, laser, light, target, part, robot) has a `parent` and its
pose in that parent, `parent_se3_self`. A parent is `"world"`, another entity's id, or a robot
link, `"<robot>/<link>"`. Together they form a tree rooted at the world, and `etendue validate`
refuses a cycle or a dangling parent.

<svg viewBox="0 0 560 210" role="img" aria-label="Frame tree of an eye-in-hand cell" style="max-width:560px;width:100%;color:currentColor">
  <g fill="none" stroke="currentColor" stroke-width="1.5">
    <rect x="230" y="10" width="100" height="30" rx="4"/>
    <rect x="60" y="85" width="120" height="30" rx="4"/>
    <rect x="380" y="85" width="120" height="30" rx="4"/>
    <rect x="60" y="165" width="120" height="30" rx="4"/>
    <rect x="230" y="165" width="100" height="30" rx="4"/>
    <rect x="380" y="165" width="120" height="30" rx="4"/>
    <path d="M280 40 L120 85 M280 40 L440 85 M120 115 L120 165 M120 115 L280 165 M120 115 L440 165"/>
  </g>
  <g fill="currentColor" font-size="13" text-anchor="middle" font-family="sans-serif">
    <text x="280" y="30">world</text>
    <text x="120" y="105">ur5e (base)</text>
    <text x="440" y="105">board</text>
    <text x="120" y="185">ur5e/tool0</text>
    <text x="280" y="185">rig</text>
    <text x="440" y="185">cam_left, cam_right</text>
  </g>
  <g fill="currentColor" font-size="11" font-family="sans-serif">
    <text x="300" y="140">tool0 → rig</text>
    <text x="400" y="140">rig → cams</text>
  </g>
</svg>

The example eye-in-hand cell: the robot and the board hang on the world, the rig on the robot's
TCP link, the cameras on the rig. The joint angles move `tool0`; everything below it follows. A
scenario's baked form (see [Motion](motion.md)) stores `world_se3_frame` for every frame at every
sample.

## Hand-eye in this vocabulary

The tree needs no special case for the two hand-eye topologies. Where things hang decides which
transforms a calibration estimates:

| Topology | Scene | What calibration-rs estimates |
|---|---|---|
| Eye-in-hand | the rig on `"<robot>/<tcp_link>"`, the target on the world | `gripper_se3_rig` (the rig in the TCP frame) and `base_se3_target` |
| Eye-to-hand | the rig on the world (or a fixture), the target on the TCP link | `rig_se3_base` and `gripper_se3_target` |
| Rig cameras | each camera on its rig | `cam_se3_rig` per camera (the inverse of the scene's `rig_se3_cam`) |

The robot poses a dataset carries are `base_se3_<tcp_link>`, computed by the same forward
kinematics as the scene. If the rig hangs on a frame fixed to the TCP link (a tool plate, say),
the fixed offset is folded into `gripper_se3_rig`. When etendue compares a calibration with the truth, it
re-expresses the truth in calibration-rs's rig frame, which is the frame of the rig's reference
(first) camera; see [Ground truth and closed-loop calibration](closed_loop.md).

## Where a camera is

For a plain pinhole camera, the camera's frame is its centre of projection. A physical camera
(one with optics) is placed by its **body** instead: the origin is where the lens axis meets the
image plane, with the camera axes above. The centre of projection, the entrance pupil, sits some
tens of millimetres in front of it and moves when the lens refocuses.
[Physical cameras](../physical_cameras.md) explains the difference. Everything that projects
(ground truth, renders, the studio's camera views) uses the projection frame, and `gt.json`
records where each physical camera's projection frame sits in its body (`mech_se3_projection`),
so an estimate can be compared in either frame.

## Pitfalls

- **Inverting by accident.** `camera_se3_target` and `target_se3_camera` differ by an inverse.
  A pose whose translation looks like "where the camera is" is a `world_se3_camera`, not a
  `camera_se3_world`.
- **The wrong link.** Hang an eye-in-hand rig on the TCP link, or on a frame fixed to it. A rig
  on a link before the last joint (the UR5e's `wrist_2_link`, say) moves relative to the TCP, so
  no single hand-eye transform exists, and the ground truth would record the one of the first
  capture. etendue does not check this for you.
- **Euler angles.** etendue's documents never store Euler angles. URDF `rpy` is fixed-axis X-Y-Z,
  $R = R_z(\text{yaw})\,R_y(\text{pitch})\,R_x(\text{roll})$, and only the URDF reader uses it.

API: `etendue-scene` (`FrameGraph`, the scene documents); see also [Scenes and
scenarios](../scenes.md) and [File formats](../formats.md).
