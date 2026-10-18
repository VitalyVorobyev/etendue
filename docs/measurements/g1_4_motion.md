# G1.4 — Feasible motion

- Gate: **G1.4** (`docs/pivot/PLAN.md`, P7-2)
- Criterion: on random programs of PTP and LIN moves, for every library robot:
  - the analytic joint velocity, acceleration and jerk stay within
    `speed_scale ×` the joint's limits, to a relative 1e-9;
  - finite differences of the sampled positions agree (velocity ≤ limit,
    second difference ≤ acceleration limit × (1 + 1e-6));
  - every capture is at rest — zero velocity, acceleration and jerk — and the
    robot does not move during the capture sample;
  - every PTP move is a straight line in joint space (off-line residual
    ≤ 1e-12 × |Δq|), never faster than an n-DoF Ruckig solution with phase
    synchronisation, and as fast to within one sample when one joint binds
    velocity, acceleration and jerk alike.
- Result: **PASS** on all four robots. Acceleration and jerk peak at exactly
  their limits (time-optimal profiles); velocity at 0.9996–0.99999.
- Measured: 2026-10-17, on etendue `f05a157` plus the uncommitted P7 work
  (branch `p7/authoring`).

## Setup

`crates/etendue-kinematics/tests/motion.rs`, test
`g1_4_motion_is_feasible_on_every_robot`. Per robot, 10 programs, each from a
random start configuration (joint limits shrunk by 0.4 rad), with a random
sample period `dt ∈ [0.002, 0.02)` s, of 12 moves with a capture after each:

- `ptp_joints` to a random configuration;
- `ptp_pose` to the tool pose of another random configuration (IK from the
  current one);
- `lin` by a random offset in the tool frame (±30 mm, ±0.08 rad per axis).

Every move gets a random `speed_scale ∈ [0.1, 1)`. Seed 20261017
(SplitMix64). A program whose LIN cannot be compiled (a waypoint out of
reach, or a joint jump above 0.2 rad between waypoints: a singularity) is a
reported error, not a limit violation, and is counted and skipped.

The jerk limits are the manifests' `max_jerk`: derived as 10/s × the
acceleration limit for every robot (`tools/robot-assets/robots.toml`), since
no vendor publishes one.

## Result

`cargo test --release -p etendue-kinematics --test motion -- --nocapture`:

| Robot | Programs | Moves | Peak velocity / limit | Peak acceleration / limit | Peak jerk / limit | PTP durations equal to Ruckig's | Programs with an infeasible LIN |
|---|---:|---:|---:|---:|---:|---:|---:|
| `abb_irb1200_5_90` | 7 | 84 | 0.999913953 | 1.000000000 | 1.000000000 | 41 | 3 |
| `igus_rebel_6dof` | 5 | 60 | 0.999986651 | 1.000000000 | 1.000000000 | 40 | 5 |
| `ur5e` | 5 | 60 | 0.999995973 | 1.000000000 | 1.000000000 | 40 | 5 |
| `fairino_fr5` (local only) | 8 | 96 | 0.999568480 | 1.000000000 | 1.000000000 | 64 | 2 |

Every capture of every program was at rest and stationary; every PTP stayed
on its line.

## Why Ruckig's duration is a bound, not an equality

The compiler moves a path parameter `s` from 0 to 1 with a 1-DoF Ruckig
profile whose limits are the tightest `limit_i / |Δq_i|` for velocity,
acceleration and jerk separately; `q = q₀ + s Δq` is then straight in joint
space by construction. Ruckig's own phase synchronisation keeps the slowest
joint's profile shape only where that shape fits every other joint; where it
does not, it falls back to time synchronisation, which arrives sooner but
leaves the line. Its duration therefore bounds ours from below, and equals it
when a single joint binds all three limits — the cases counted above.

## Also checked

- `stored_configurations_pin_the_ik_branch`: a scenario with every
  `ptp_pose`'s `q` stored (`resolve`) compiles to bit-identical joint
  positions, uses the stored configuration (`IkReport::Stored`), and a `q`
  moved by 1 mrad is refused.
- `the_report_names_the_binding_limit`: the motion report's binding limit is
  at 100 % on a time-optimal move.

## Reproduce

```bash
cargo test --release -p etendue-kinematics --test motion -- --nocapture
```
