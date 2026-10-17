# G1.1 — Forward kinematics vs Pinocchio

- Gate: **G1.1** (`docs/pivot/PLAN.md`, P1-3)
- Criterion: on 10 000 random configurations per robot, `etendue-kinematics`
  FK matches a fixture generated once with Pinocchio to ≤ 1e-9 m and
  ≤ 1e-9 rad.
- Result: **PASS**. The worst error is 8.5e-16 m / 1.3e-15 rad, six orders of
  magnitude inside the gate.
- Measured: 2026-09-26, on etendue commit `4333b1c` (clean tree). The result is
  identical to the pre-commit run.
- Re-measured 2026-10-17 (P7-1) with the igus ReBeL and the local-only FAIRINO
  FR5 added, on `f05a157` plus the uncommitted P7 work (branch
  `p7/authoring`): **PASS**, worst 1.3e-15 m / 1.8e-15 rad (FR5). The FR5
  fixture is generated with `--include-local` into the git-ignored
  `tools/fixtures/fk-local/` (no redistribution licence); the gate tests read
  it when it exists.

## Fixtures

`tools/fixtures/fk/<robot>.json` is written by `tools/fixtures/fk_fixture.py`.

**How the fixture is generated**
- Library: Pinocchio (`pin`) 4.1.0 with numpy 2.5.3 on Python 3.12, via
  `pinocchio.buildModelFromUrdf`.
- Inputs: 10 000 configurations `q`, drawn uniformly within the `robot.json`
  joint limits with seed 20260926.
- Outputs:
  - `base_se3_tcp` for each configuration;
  - `base_se3_link` for every URDF link for the first 100 configurations.
- The base is the manifest's REP-199 `base` link (ADR 0002).

**Script self-checks**
- An independent numpy URDF chain agrees with Pinocchio to 6.1e-16.
- `oMi · placement` equals `oMf` exactly.
- `urdf_sha256` matches the committed `robot.urdf`.

| Robot | Fixture | Size | sha256 |
|---|---|---|---|
| `ur5e` | `tools/fixtures/fk/ur5e.json` | 3 271 610 B | `88f93957…ff9a2b86` |
| `abb_irb1200_5_90` | `tools/fixtures/fk/abb_irb1200_5_90.json` | 3 242 447 B | `118c8a39…d7ce8ce522` |
| `igus_rebel_6dof` | `tools/fixtures/fk/igus_rebel_6dof.json` | 3 271 981 B | `fbdf5a78…ebb214f2` |

## Result

`cargo test -p etendue-kinematics --test gates g1_1 -- --nocapture`:

| Robot | TCP, 10 000 configs: max m / rad | Every link, 100 configs: max m / rad (pose count) |
|---|---|---|
| `abb_irb1200_5_90` | 8.087e-16 / 1.013e-15 | 5.796e-16 / 7.407e-16 (1000) |
| `ur5e` | 8.528e-16 / 1.336e-15 | 6.974e-16 / 9.037e-16 (1300) |
| `igus_rebel_6dof` | 7.134e-16 / 1.199e-15 | 3.999e-16 / 7.713e-16 (1100) |
| `fairino_fr5` (local only) | 1.296e-15 / 1.794e-15 | 6.004e-16 / 1.176e-15 (800) |

Rotation error is the angle of `R_etendue⁻¹ · R_pinocchio`, computed with
`atan2(|v|, |w|)` on the relative quaternion rather than `acos`. `acos` would
inflate a 1e-16 difference to about 1e-8.

## Reproduce

```bash
uv run tools/fixtures/fk_fixture.py                         # regenerate fixtures (deterministic)
cargo test -p etendue-kinematics --test gates -- --nocapture
```
