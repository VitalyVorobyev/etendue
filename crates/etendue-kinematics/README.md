# etendue-kinematics

Robot kinematics and scenario baking for [etendue](https://github.com/VitalyVorobyev/etendue).

- `RobotModel`: a URDF bound to an `etendue-scene` robot manifest, with forward kinematics, the
  geometric Jacobian and damped-least-squares inverse kinematics with deterministic restarts.
- Analytic OPW inverse kinematics behind the `opw` feature.
- `compile` / `bake`: a scenario becomes a trajectory (synchronised trapezoidal point-to-point
  moves, spline-timed straight-line moves within the joint limits, stop-and-shoot captures) and a
  baked scenario.

Documentation: <https://vitalyvorobyev.github.io/etendue>.
