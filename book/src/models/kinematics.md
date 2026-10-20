# Robot kinematics

In a hand-eye calibration the robot is a measuring instrument: each view comes with the pose the
controller reports for its tool, and the calibration trusts it. etendue's synthetic robot must
therefore produce those poses exactly as the real kinematics would (forward kinematics), find the
joint angles that put a camera where you want it (inverse kinematics), and tell you when a pose is
close to a joint limit or a singularity, where the real robot would struggle.

## The robot model

A robot is its URDF plus a manifest (`robot.json`) that names:

- the **base link**, the frame the controller reports tool poses in (for the library robots the
  URDF's REP-199 `base` link);
- the **TCP link**, whose pose the controller reports (`tool0` for the UR5e);
- every joint's limits: position, velocity, acceleration and jerk.

The URDF supplies the chain: for each joint its origin (a fixed transform from the parent link)
and its axis. Revolute, continuous, prismatic and fixed joints are supported; floating, planar and
spherical joints and mimic joints are not. The library's robots (UR5e, ABB IRB 1200-5/0.90, igus
ReBeL 6DOF) are on [Robots](../robots.md).

## Forward kinematics

The pose of the TCP in the base frame is the product, along the chain, of each joint's origin and
its motion:

$$
{}^{\text{base}}T_{\text{tcp}}(\mathbf{q}) =
\prod_{j} \; T_{\text{origin},j}\; M_j(q_j),
$$

where $M_j$ is a rotation by $q_j$ about the joint's axis (revolute) or a translation by $q_j$
along it (prismatic), both in the joint's frame. A URDF `rpy` is the fixed-axis rotation
$R_z(\text{yaw})\,R_y(\text{pitch})\,R_x(\text{roll})$.

Over 10 000 random configurations per robot, etendue's forward kinematics agree with Pinocchio's
(an independent robotics library) to $1.3 \times 10^{-15}$ m and $1.8 \times 10^{-15}$ rad at the
TCP, and as closely for every link. The studio and the command line compute the same poses to the last bit.

## The Jacobian and singularities

The geometric Jacobian $J(\mathbf{q})$ maps joint velocities to the TCP's linear and angular
velocity in the base frame. For a revolute joint $j$ with axis $\hat{\mathbf{z}}_j$ through the
point $\mathbf{p}_j$, its column is

$$
J_j = \begin{pmatrix} \hat{\mathbf{z}}_j \times (\mathbf{p}_{\text{tcp}} - \mathbf{p}_j) \\ \hat{\mathbf{z}}_j \end{pmatrix},
$$

and for a prismatic joint $(\hat{\mathbf{z}}_j, \mathbf{0})$. At a **singularity** $J$ loses rank:
some TCP motion needs infinite joint speed, and near one, small TCP motions need large joint
motions. etendue measures the distance from a singularity in two ways:

- **Yoshikawa's manipulability** $w = \sqrt{\det(J J^\top)}$, zero at a singularity. The Jog
  panel shows it with the TCP and warns below 0.01.
- **The smallest singular value** $\sigma_{\min}$ of $J$. The motion report and the pose
  generator use it, with 0.02 as their default threshold.

Both mix metres and radians, so their scale depends on the robot's size; compare them on one robot,
not across robots.

## Inverse kinematics

etendue solves IK numerically, by **damped least squares** (Levenberg–Marquardt). The error
between the target pose and the current one is a 6-vector, the position difference and the
rotation vector of $R^{*} R^{\top}$, and each step is

$$
\Delta\mathbf{q} = J^\top\left(J J^\top + \lambda^2 I\right)^{-1}\mathbf{e} .
$$

The damping $\lambda$ adapts: it starts at $10^{-2}$, shrinks by 0.3 after a step that reduces the
error (toward Gauss–Newton) and grows by 5 after one that does not (toward gradient descent).
Steps are at most 0.3 rad per joint, every iterate is clamped to the joint limits, and a solve has
converged at $10^{-10}$ m and $10^{-10}$ rad, within 500 iterations.

A local method can stall, typically when a joint limit blocks the way to the solution. If the
attempt from the given seed fails, the solver **restarts** from up to 16 seeds spread over the
joint ranges. The seeds come from a fixed-seed generator, so the solver is deterministic: the same
robot, target, seed and options always give the same answer.

Over 10 000 reachable poses per robot, solving from a seed near the answer never failed. From
random seeds, with the restarts, 0.11 % of UR5e poses and 0.35 % of ABB IRB 1200 poses were not
solved (1.45 % and 2.14 % for the igus ReBeL and the FAIRINO FR5). Without restarts, on a test
arm with ±π limits, 68 % of random-seed solves fail.

### Branches

A 6-axis arm reaches most poses in several joint configurations: shoulder left or right, elbow up
or down, wrist flipped or not. These are the IK **branches**. Damped least squares finds the
solution its seed leads to, usually the nearest; a restart can land on any branch, and then the
arm may swing through a large motion to get there. That is why:

- a `ptp_pose` step can store the configuration it was solved to (`q`), so compiling the scenario
  again always takes the same branch; the pose generator and **Resolve IK** store it;
- straight-line moves and jogging solve with no restarts, each step seeded from the last, so the
  arm moves continuously. Dragging the UR5e's TCP around a 150 mm circle in 1000 steps converges
  every step, in 0.012 ms (95th percentile), with joint steps of at most 3.8 mrad;
- a pose the solver does not reach while jogging is reported as **Out of reach**, and the arm
  stays where it was.

Try it: move the UR5e's joints and watch the TCP pose and the manipulability. Then solve IK to a
TCP pose from different seeds, and see the arm land on different branches of the same pose.

<div class="etendue-demo" data-demo="kinematics"></div>

### Analytic IK for OPW arms

Arms with an ortho-parallel base and a spherical wrist (OPW: the ABB IRB 1200, many industrial
arms; not the UR5e, whose wrist is offset) have closed-form IK. etendue's kinematics library can
solve them analytically, returning every branch, each checked against the URDF's forward
kinematics to $10^{-9}$, and pick the one nearest a seed (worst error $5.5 \times 10^{-10}$ m over
10 000 poses). It is a library option; the studio and the command line use damped least squares
for every robot.

## Assumptions and limits

- **Kinematics only.** No dynamics, payloads or torques; motion limits are the manifest's joint
  limits (see [Motion](motion.md)).
- **No collision checking**, neither with the cell nor of the robot with itself.
- **A perfect robot.** The URDF is taken as exact: no joint offsets, link-length errors or
  compliance. A real robot's reported poses carry such errors; [observation
  noise](closed_loop.md) can add random robot-pose error, but not a systematic one.

API: `etendue-kinematics` `RobotModel` (`tcp_pose`, `tcp_jacobian`, `manipulability`, `ik`,
`ik_opw`); `@etendue/wasm` `robotTcp`, `robotIk`.
