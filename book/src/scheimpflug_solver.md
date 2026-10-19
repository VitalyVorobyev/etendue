# Scheimpflug Solver

The Scheimpflug solver finds the sensor tilt angles `(τ_x, τ_y)` and focus
distance `s_o` that minimise the **worst-case circle of confusion** over a
camera-frame depth window `[d_min, d_max]` around a working distance
`d_opt`.

The solver is part of `etendue-core` (`etendue_core::solver`). It minimises
with a small deterministic Nelder–Mead simplex of its own, restarted from its
best point until a restart stops helping, so it needs no random numbers and
builds for WebAssembly.

## The design problem

A triangulation camera viewing a target at an angle to its optical axis
suffers depth-dependent focus variation: points near the camera are in focus
while far points blur, or vice versa. The Scheimpflug principle says this is
correctable: there exists a sensor tilt that rotates the plane of best focus
until it contains the target plane. The solver finds that tilt and the
matching focus distance.

## Cost function

The solver minimises the worst-case circle of confusion over the laser fan:

$$
C(\tau_x, \tau_y, s_o)
  = \max_{p \,\in\, \text{valid samples}} \text{CoC}(\tau_x, \tau_y, s_o;\, p)
$$

The fan is sampled in its natural `(phi, r)` parameterisation (see
[`SampleGrid`](#samplegrid)). Each sample is transformed into the camera
frame and kept only if it is **valid**: in front of the camera, projecting
inside the sensor rectangle, and at a camera-frame depth inside the
`[d_min, d_max]` window. The cost is the largest CoC over the surviving
samples, or `+∞` when a trial loses the whole stripe, a strong penalty the
optimiser steers away from. Samples outside the depth window are dropped,
not penalised, so the optimiser cannot shrink the working volume to game the
objective. (`d_opt` is carried as design intent; the depth window is what
actually drives the cost.)

The max-CoC objective is the right cost for a "worst-case image quality"
criterion. It is **non-smooth** (the validity predicates flip discretely as a
trial moves), which is why the solver is derivative-free.

The CoC is computed by the Scheimpflug formula `coc_diameter_at_sensor`, the
same physics behind `ThickLens::coc_diameter_px` (see
[Thick Lens and Defocus](thick_lens.md)). It accounts for the sensor tilt,
the inter-principal gap, and the object depth in one formula.

## API

```rust
use etendue_core::solver::{SampleGrid, SolverSpec, SolverResult, SolverError, solve_scheimpflug};

pub fn solve_scheimpflug(
    camera: &CameraEntity,
    laser:  &LaserEntity,
    spec:   &SolverSpec,
) -> Result<SolverResult, SolverError>;

pub struct SolverSpec {
    pub d_opt: f64,                 // working distance, metres (design intent)
    pub depth_range: (f64, f64),    // camera-frame (d_min, d_max), metres
    pub grid: SampleGrid,
}
```

The camera is **not** mutated. To apply a result, write `s_o` into
`camera.optics.focus_distance_m` and set
`camera.params.sensor = SensorParams::Scheimpflug { … }` with
`tilt_x = tau_x` and `tilt_y = tau_y`.

`SolverError` is `#[non_exhaustive]`, with variants `InvalidSpec` (non-finite
numbers, an inverted or negative depth window, a grid smaller than 2 × 2),
`InvalidCamera`, and `SolverFailure` (the optimiser could not make progress,
typically because every trial lost the stripe).

## The solver: Nelder-Mead

The optimisation uses the **Nelder-Mead** simplex method (`solver::nelder_mead`,
with the standard coefficients: reflection 1, expansion 2, contraction ½,
shrink ½). Nelder-Mead is derivative-free, the natural choice because the
worst-case-CoC cost is non-smooth and cheap to evaluate, so a gradient method
would have no smooth gradient to follow.

The initial simplex is anchored at the camera's current `(τ_x, τ_y, s_o)`
with small per-variable perturbations: 1° in each tilt, and 5 % of `d_opt`
(or 5 mm, whichever is larger) in focus. Each run stops when the standard
deviation of the simplex's costs is at most `1e-6`, or after 500 iterations.

The worst-case blur is steep in focus and in one tilt and nearly flat in the
other, and a single run can collapse its simplex before the flat direction is
explored. The solver therefore restarts from its best point with a fresh
simplex until a restart stops improving the cost (at most 8 runs).

The tilts are searched in the defocus physics' convention and returned in
calibration-rs's, as `CameraParams` stores them: the two differ in the sign of
`τ_x` (`optics::coc_tilt`).

There are no hard solver bounds. Instead the cost returns `+∞` for a
physically invalid trial (`s_o ≤ f`, or a tilt magnitude at or beyond
`π/2`), so the simplex retreats from the infeasible region on its own.

## SampleGrid

```rust
pub struct SampleGrid {
    pub rows: usize,   // default 32
    pub cols: usize,   // default 32
}
```

The grid is a `rows × cols` sampling of the **laser fan plane** in its
natural `(phi, r)` parameterisation: `rows` sweeps the fan angle, `cols`
the radius. Both must be at least 2. `SampleGrid::DEFAULT` (also
`SampleGrid::default()`) is 32 × 32. Larger grids give more accurate
worst-case estimates but cost proportionally more cost-function
evaluations; 32 × 32 (1 024 points) solves in well under a frame on a
typical development machine.

## SolverResult

```rust
pub struct SolverResult {
    pub tau_x: f64,          // optimal tilt around x, radians
    pub tau_y: f64,          // optimal tilt around y, radians
    pub s_o:   f64,          // optimal focus distance, metres
    pub max_coc_px: f64,     // worst-case CoC over the sample grid, pixels
    pub n_valid_samples: usize,
    pub converged: bool,
    pub iterations: u32,
}
```

`n_valid_samples` is zero when the design lost the whole stripe; `max_coc_px`
is then `+∞`. `converged = false` means the iteration cap was hit; the result
is still the best-known simplex point.

## Validation

Two convergence tests guard the solver:

- `fronto_parallel_fan_drives_tilt_to_zero_and_focus_to_d_opt`: when the
  laser fan is fronto-parallel (perpendicular to the camera axis) the
  optimal Scheimpflug tilt is zero and the optimal focus is `d_opt`; the
  solver must recover this.
- `solving_lowers_worst_case_coc`: the solver result must strictly reduce
  the worst-case CoC compared with the baseline (zero tilt, current focus).
