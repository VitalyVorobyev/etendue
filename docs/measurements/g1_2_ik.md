# G1.2 — Inverse kinematics round trips

- Gate: **G1.2** (`docs/pivot/PLAN.md`, P1-4)
- Criterion: on 10 000 reachable poses per robot, `FK(IK(T))` matches `T` to
  ≤ 1e-9 m / 1e-9 rad for analytic OPW IK, and to ≤ 1e-6 m / 1e-6 rad for DLS.
  Report the DLS failure rate and the 99th-percentile iteration count.
- Result: **PASS** for every robot (UR5e, ABB IRB 1200, igus ReBeL, and the
  local-only FAIRINO FR5).
  - OPW: 0 failures, worst 5.5e-10 m / 5.1e-11 rad.
  - DLS: every success has a residual ≤ 1.0e-10.
  - DLS failure rate: 0 % from nearby seeds; 0.11 % (UR5e) and 0.35 % (ABB)
    from random seeds.
- Measured: 2026-09-26, on etendue commit `4333b1c` (clean tree). The result is
  identical to the pre-commit run.

## Setup

**Target poses.** `T` is the fixture's `base_se3_tcp` for each of its 10 000
configurations (`tools/fixtures/fk/`, see G1.1). Every target is therefore
reachable inside the joint limits.

**DLS solver (`RobotModel::ik`, default `IkOptions`).** Damped least squares
(Levenberg–Marquardt) with these settings:
- λ₀ = 1e-2, adapted ×0.3 on an accepted step and ×5 on a rejected one;
- iterates clamped to the joint limits;
- step ∞-norm ≤ 0.3 rad;
- convergence at 1e-10 m and 1e-10 rad;
- 500 iterations per attempt;
- 16 deterministic restarts from SplitMix64 seeds uniform within the limits.

Iteration counts are summed over all attempts.

**Seed policies.**
- `near`: the true configuration plus U(−0.1, 0.1) rad per joint.
- `random`: uniform within the joint limits.

**OPW (`RobotModel::ik_opw`, feature `opw`).** Uses `rs-opw-kinematics` 3.0.0
preset `irb1200_5_90`, seeded with the true configuration. Each analytic
solution is then:
- shifted by ±2π where the joint limits allow;
- filtered to the joint limits;
- **checked against etendue's own URDF FK** to 1e-9.

The solution nearest the seed is returned. UR5e is not an OPW arm (its wrist
is offset), so it uses DLS only.

## DLS results

`cargo run --release -p etendue-kinematics --example g1_2_ik`:

| robot | seeds | poses | failures | failure rate | iters p50 | iters p99 | iters max | worst residual (m / rad) |
|---|---|---|---|---|---|---|---|---|
| abb_irb1200_5_90 | near | 10000 | 0 | 0.00 % | 4 | 7 | 258 | 9.99e-11 / 9.93e-11 |
| abb_irb1200_5_90 | random | 10000 | 35 | 0.35 % | 510 | 6510 | 8051 | 9.98e-11 / 1.00e-10 |
| ur5e | near | 10000 | 0 | 0.00 % | 4 | 9 | 2018 | 1.00e-10 / 9.99e-11 |
| ur5e | random | 10000 | 11 | 0.11 % | 130 | 2046 | 6384 | 9.96e-11 / 1.00e-10 |
| igus_rebel_6dof | near | 10000 | 0 | 0.00 % | 4 | 8 | 1053 | 9.99e-11 / 9.98e-11 |
| igus_rebel_6dof | random | 10000 | 145 | 1.45 % | 515 | 6513 | 8425 | 9.88e-11 / 9.99e-11 |
| fairino_fr5 (local only) | near | 10000 | 0 | 0.00 % | 4 | 8 | 2522 | 1.00e-10 / 9.66e-11 |
| fairino_fr5 (local only) | random | 10000 | 214 | 2.14 % | 524 | 6141 | 8055 | 9.96e-11 / 9.97e-11 |

The igus ReBeL and the FAIRINO FR5 rows were added on 2026-10-17 (P7-1, `f05a157`
plus the uncommitted P7 work); the ABB and UR5e rows are unchanged. Their
random-seed failure rates are higher (1.45 % and 2.14 %); the cause was not
investigated. From nearby seeds — what the scenario compiler and the pose
generator use — neither fails.

A single DLS attempt from a random seed stalls in a local minimum often. A
hand-written test arm with ±π limits shows the effect: without restarts, 68 %
of random-seed solves fail with residuals of 3–60 cm. The limits block the
path to the solution branch. Deterministic restarts reduce this to the rates
above.

How the scenario compiler uses the solver:
- `lin` moves set `restarts: 0`, because each waypoint must continue the
  previous waypoint's branch.
- `ptp_pose` keeps the default restarts. It first tries a local solve seeded
  from the current configuration, and only on failure falls back to the
  restarts, which may land on any valid branch.

## OPW result

`cargo test -p etendue-kinematics --features opw --test gates g1_2_opw -- --nocapture`:

| robot | poses | failures | max translation | max rotation |
|---|---|---|---|---|
| abb_irb1200_5_90 | 10000 | 0 | 5.459e-10 m | 5.067e-11 rad |

The ≤ 1e-9 gate holds, but with only about a 2× margin in translation. The
residual comes from the closed-form solution's own conditioning; the DLS
residuals, at 1e-10, are smaller. The CI-sized assertions run in
`tests/gates.rs`.
