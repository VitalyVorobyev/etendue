# Glossary

**Airy disc.** The blur spot diffraction makes at a lens's aperture: its diameter is about
$2.44\,\lambda N$ for wavelength $\lambda$ and working f-number $N$. It sets a floor on blur that
stopping down raises. See [Defocus and the blur budget](models/blur.md).

**Bake.** To compile a scenario into the pose of every frame at every sample, `dt` apart. The
result is a **baked scenario**. See [Scenes and scenarios](scenes.md#the-scenario).

**Base frame.** The robot frame the controller reports tool poses in. Tool poses in scenarios and
datasets (`base_se3_tool`) are given in it. See [Robots](robots.md#base-and-tool-frames).

**Beam waist.** The narrowest point of a laser's Gaussian beam, and its radius there
(`beam_waist_m`, the 1/e² radius). A laser focused at a distance has its waist there
(`waist_distance_m`). The stripe is thinnest at the waist. See [Laser line and stripe](laser.md).

**Cell.** A calibration setup: a scene and its scenario, kept as a folder in a project.

**Circle of confusion (CoC).** The blur spot a point out of focus makes on the sensor: the cone of
light from the lens's aperture, cut by the sensor before or after it converges. See
[Defocus and the blur budget](models/blur.md).

**Closed loop.** Making a dataset from a known truth, calibrating it, and comparing the result
with that truth. See [Calibration](calibration.md) and
[Ground truth and closed-loop calibration](models/closed_loop.md).

**Coverage.** Which points of a volume a rig sees, and how many cameras see each one sharply. See
[Working volume and rig coverage](models/coverage.md).

**Dataset.** A folder holding a cell's ground truth, its calibration-rs manifest, and optionally
its rendered images and detections. See [Datasets](datasets.md).

**Depth of field.** The range of distances over which blur stays below a limit.

**Detection.** Finding the target's points (or a laser stripe) in an image and labelling them.
`etendue detect` checks every label against the ground truth.

**Entrance pupil.** The image of the lens's aperture seen from the front of the lens. Every ray
of a camera passes through its centre, so it is the camera's centre of projection: the point a
pinhole model and a calibration place the camera at. It is not the principal point or the nodal
point. See [Physical cameras and the thick lens](physical_cameras.md).

**Extrinsics.** A camera's pose: on a rig, its pose relative to the rig or to the other cameras.

**Eye-in-hand.** A hand-eye setup with the cameras on the robot's flange, looking at a fixed
target.

**Eye-to-hand.** A hand-eye setup with the cameras fixed in the cell, looking at a target the
robot holds.

**Frame.** A coordinate system: the world, a robot link, a rig, a camera. Every item of a scene
has one, placed in its parent's. See [Frames and poses](models/frames.md).

<a id="gate"></a>
**Gate.** A measured pass/fail criterion of etendue's own development, with an id such as G4.2
or G5.1. Measurements in the repository's `docs/measurements/` and some tool output are labelled
with them, as is the `gate` field of a calibration report. The two you meet as a user: **G5.1** is the exact-data check of a camera calibration
(reprojection, focal length, principal point, hand-eye), and **G5.3** adds the laser plane. Their
tolerances are in [Calibration](calibration.md#the-tolerances).

**Ground truth.** The exact answer etendue computes from a scene: true camera models and poses,
the true hand-eye transform and laser planes, and the pixel of every target point in every view.
Written to `gt.json`.

**Hand-eye calibration.** Finding the fixed transform between a robot's flange and a camera (or
rig) it carries, or between the robot's base and a fixed camera, from images taken at several
robot poses together with the poses the robot reports.

**Head.** See *triangulation head*.

**Intrinsics.** A camera's internal parameters: focal lengths `fx`, `fy` and principal point
`cx`, `cy` in pixels, skew, distortion and sensor tilt. See [Camera models](models/camera_models.md).

**Inverse kinematics (IK).** Finding the joint positions that put the robot's tool at a pose.
Most poses have several solutions (configurations). See [Robot kinematics](models/kinematics.md).

**Jog.** Moving the robot by hand, joint by joint or along the tool's axes. The studio's
**Jog** panel does it.

**Laser plane.** The plane a line laser's fan lies in. Laser-line calibration estimates it in each
camera's frame.

**LIN.** A straight-line move: the tool travels along a straight line between poses. A scenario
step of type `lin`.

**Manipulability.** How freely the tool can move in every direction at a joint configuration. It
falls to zero at a singularity.

**Mechanical frame.** A physical camera's body frame, at the image plane (the sensor's datum),
where the camera is mounted. A scene places a physical camera by it. See
[Physical cameras and the thick lens](physical_cameras.md).

**Observation.** What a calibration is given: target-point pixels, laser-stripe pixels and robot
tool poses. Exact, with noise, or detected. See [Calibration](calibration.md#observations).

**Pinhole camera.** A camera model in which every ray passes through one point. Calibration
estimates a pinhole model with distortion.

**Plane of best focus.** The plane in the scene that a camera images sharply. Square to the axis
for an untilted sensor; tilted for a Scheimpflug sensor.

**Pose.** A position and an orientation: a rigid transform, written
`{"rotation": [qx, qy, qz, qw], "translation": [x, y, z]}`. A pose named `a_se3_b` maps
coordinates in frame `b` to frame `a`. See [File formats](formats.md#poses-on-disk).

**Principal planes.** The two planes of a thick lens (H and H′) from which its focal lengths are
measured. Their separation is the principal-plane gap.

**Project.** A folder holding cells, a component library, datasets and runs. See
[Projects and the library](library.md).

**PTP.** A point-to-point move: every joint moves from its start to its end position, all
starting and stopping together; the tool's path is not a straight line. Scenario steps
`ptp_joints` and `ptp_pose`.

**Reprojection error.** The distance, in pixels, between an observed point and where the
estimated model projects it.

**Rig.** A rigid mount that holds cameras and lasers together.

**Run.** One calibration of a dataset, kept as a folder with its options, report and result. See
[Calibration](calibration.md#runs).

**Scenario.** A cell's robot program: moves and captures. See [Scenes and scenarios](scenes.md).

**Scene.** What a cell contains: robots, rigs, cameras, lasers, lights, targets and parts, as a
tree of frames. See [Scenes and scenarios](scenes.md).

**Scheimpflug.** Tilting the sensor relative to the lens so that the plane of best focus tilts
too. When the sensor plane, the lens plane and the plane of best focus meet in one line, a plane
at an angle to the camera — such as a laser plane — is in focus everywhere. See
[Scheimpflug plane of best focus](scheimpflug.md).

**Singularity.** A joint configuration where the robot loses the ability to move its tool in some
direction, and small tool motions need large joint motions.

**Speed scale.** The fraction of a robot's joint velocity, acceleration and jerk limits a move
uses.

**Stop-and-shoot.** A program that moves to each viewpoint, stops, and captures.

**TCP (tool centre point).** The robot's tool frame: the flange, or a tool on it. etendue's robots
report the flange, `tool0`.

**Triangulation.** Measuring a point's position from where it appears in a camera and the known
geometry of a laser plane (or a second camera).

**Triangulation head.** A camera and a line laser fixed to its body: a laser profile sensor. The
studio's Optics workspace designs one. See [Triangulation heads](models/heads.md).

**Working distance.** The distance from a camera or rig to where it is meant to measure. In a rig
layout, where the camera axes meet the rig axis.

**Working f-number.** The f-number that governs blur and diffraction at a given focus:
$N_w = N\,(1 + |m|/m_p)$ for magnification $m$ and pupil magnification $m_p$. It is larger than
the nominal f-number for close focus. See [Physical cameras and the thick lens](physical_cameras.md).

**Working volume.** The region a camera or rig measures in: in view and sharp enough.
