# Motion

A calibration program is a robot program: move, stop, take a picture, move on. How long it takes
decides a cell's throughput, and whether it respects the robot's limits decides whether the real
controller will run it as planned. etendue compiles a scenario into a timed trajectory that stays
within every joint's velocity, acceleration and jerk limits, stops the robot for every capture,
and reports how close each move comes to the limits, the joint ranges and singularities.

## The timing model

A scenario's steps run in order. One robot moves at a time, and every move is **rest to rest**:
it starts and ends with zero velocity and acceleration. A move's `speed_scale` (in $(0, 1]$) scales
all three of the robot's limits. The trajectory is sampled every `dt` seconds:

- every move's duration is rounded **up** to a whole number of samples (the profile is slowed to
  fit, within the same limits), so step boundaries fall on samples;
- a `capture` is one sample with the robot at rest, flagged with its id (**stop and shoot**);
- a `wait` holds for $\lceil \text{duration} / dt \rceil$ samples;
- a final sample holds the end state.

## Point-to-point moves

A `ptp_joints` or `ptp_pose` move goes in a **straight line in joint space**,

$$
\mathbf{q}(t) = \mathbf{q}_0 + s(t)\,\Delta\mathbf{q}, \qquad s: 0 \to 1,
$$

so every joint starts and stops together. The path parameter $s$ follows a **time-optimal,
jerk-limited** profile computed by Ruckig, with limits taken from the tightest joint for each
quantity separately:

$$
|\dot s| \le \min_i \frac{v_i}{|\Delta q_i|}, \qquad
|\ddot s| \le \min_i \frac{a_i}{|\Delta q_i|}, \qquad
|s^{(3)}| \le \min_i \frac{j_i}{|\Delta q_i|},
$$

with $v_i, a_i, j_i$ joint $i$'s limits times the speed scale. A jerk-limited profile ramps the
acceleration up and down instead of switching it on: up to seven phases (jerk up, constant
acceleration, jerk down, cruise, and the mirror image). A short move never reaches its cruise
velocity.

A `ptp_pose` move first needs a joint configuration: the stored `q` if the step has one (checked
to reach the pose within $10^{-6}$), else inverse kinematics from the current configuration (see
[Robot kinematics](kinematics.md#branches)).

**Worked example.** The UR5e's joints are limited to π rad/s, 5 rad/s² and 50 rad/s³. Turning the
shoulder pan by 90° at full speed scale:

- the acceleration ramps take $a/j = 0.1$ s;
- reaching π rad/s would take 2.29 rad of travel, more than the 1.57 rad of the move, so the
  profile peaks at 81 % of the velocity limit;
- the move takes **1.226 s** (at `dt` = 2 ms), with acceleration and jerk at 100 % of their
  limits.

The same robot moving its shoulder lift and elbow by 0.37 rad each at speed scale 0.5 takes
0.878 s; both joints peak at 50 % of their acceleration and jerk limits and 27 % of their velocity
limit.

Joint-space straight lines are a choice. Ruckig, synchronising several joints itself, could
sometimes arrive sooner by letting the joints leave the line; etendue's moves are therefore never
faster than Ruckig's own multi-joint solution, and exactly as fast when one joint binds velocity,
acceleration and jerk alike.

## Straight-line moves

A `lin` move takes the TCP along a straight line, rotating it by spherical interpolation (slerp)
between the two orientations. etendue:

1. places waypoints along the line at most 1 mm and 0.5° apart;
2. solves inverse kinematics at each, seeded from the previous one, with no restarts, so the arm
   stays on its branch;
3. joins the joint values with a natural cubic spline per joint;
4. times the path parameter with a jerk-limited profile whose limits come from the spline's exact
   derivative bounds $d_1, d_2, d_3$, so that every joint's velocity, acceleration
   ($q''\dot s^2 + q'\ddot s$) and jerk ($q'''\dot s^3 + 3q''\dot s\ddot s + q's^{(3)}$) stay
   within its limits:

$$
\dot s \le \min\!\left(\frac{v}{d_1},\ \sqrt{\frac{a}{2d_2}},\ \sqrt[3]{\frac{j}{3d_3}}\right), \qquad
\ddot s \le \min\!\left(\frac{a}{2d_1},\ \frac{j}{9 d_2 \dot s_{\max}}\right), \qquad
s^{(3)} \le \frac{j}{3d_1} .
$$

A `lin` move is refused if a waypoint is out of reach, if any joint changes by more than 0.2 rad
between neighbouring waypoints (the line passes through a singularity or forces a branch switch),
or if the spline leaves a joint's range.

## Checked against the limits

On random programs (point-to-point and straight-line moves, random speed scales and sample
periods, a capture after each move) for every library robot, the analytic velocity, acceleration
and jerk never exceed the scaled limits (to a relative $10^{-9}$), finite differences of the
sampled positions agree, every capture is at rest, and every point-to-point move stays on its
joint-space line. Acceleration and jerk peak at exactly their limits: the profiles are
time-optimal.

Vendors publish velocity and acceleration limits, not jerk limits. The library's manifests set
each joint's jerk limit to 10/s times its acceleration limit, so an acceleration ramp takes
0.1 s.

Try it: set two joint configurations and watch each joint's position, velocity, acceleration and
jerk over the move, and how close each comes to its limit.

<div class="etendue-demo" data-demo="motion"></div>

## The motion report

`etendue export --report` (and the studio's step list) reports on every step:

| Field | Meaning |
|---|---|
| `kind`, `robot`, `t0`, `duration_s` | the step, the robot it moves, when it starts and how long it takes |
| `peaks` | per joint, the largest velocity, acceleration and jerk over the step, as fractions of the joint's limits |
| `binding` | the joint and quantity closest to its limit: what sets the step's duration |
| `min_limit_margin` | the smallest distance of any joint from its position limits over the step |
| `min_sigma` | the smallest singular value of the TCP Jacobian over the step: near zero at a singularity |
| `max_joint_jump` | for a `lin`, the largest joint change between waypoints |
| `ik` | for a `ptp_pose`, whether its configuration was stored or solved, and on which attempt |
| `warnings` | what deserves a look (below) |

| Warning | When |
|---|---|
| `ik_restart_used` | IK converged only from a restart seed: the configuration may be on another branch, and the arm may swing far. Store `q` to pin it. |
| `near_limit` | a joint comes within 2° of a position limit |
| `near_singular` | $\sigma_{\min}$ drops below 0.02 |
| `large_joint_jump` | a `lin` move's joints change by more than 0.05 rad between waypoints |

The report also gives the program's total duration and number of captures.

## Assumptions and limits

- **Joint limits only.** No dynamics: torques, payload and the TCP's Cartesian speed are not
  limited, and a real controller may be slower or faster than the limits allow.
- **No blending.** Every move stops; a controller blending moves would be faster.
- **One robot at a time.** Moves of several robots are not overlapped.
- **No collision checking** (see [Robot kinematics](kinematics.md)).

API: `etendue-kinematics` `compile`, `bake`, `report`; `etendue export`; `@etendue/wasm`
`motionReport`.
