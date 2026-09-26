# Scheimpflug Solver (M9)

The M9 Scheimpflug solver finds the sensor tilt angles `(τ_x, τ_y)` and
focus distance `s_o` that minimise the **worst-case circle-of-confusion**
over a user-defined depth window `[d_min, d_max]` at an optimal working
distance `d_opt`.

## The design problem

A triangulation camera viewing a target at an angle to its optical axis
suffers depth-dependent focus variation: points near the camera are in focus
while far points blur, or vice versa. The Scheimpflug principle says this is
correctable — there exists a sensor tilt that rotates the plane of best
focus until it contains the target plane. The solver finds that tilt and the
matching focus distance automatically.

## Cost function

The solver minimises the worst-case circle of confusion over the laser fan:

$$
C(\tau_x, \tau_y, s_o)
  = \max_{p \,\in\, \text{valid samples}} \text{CoC}(\tau_x, \tau_y, s_o;\, p)
$$

The fan is sampled in its natural `(phi, r)` parameterisation (see
[`SampleGrid`](#samplegrid)). Each sample is transformed into the camera
frame and kept only if it is **valid** — in front of the camera, projecting
inside the sensor rectangle, and at a camera-frame depth inside the user's
`[d_min, d_max]` window. The cost is the largest CoC over the surviving
samples, or `+∞` when a trial loses the whole stripe — a strong penalty the
optimiser steers away from. (`d_opt` is carried as design intent for the UI
readout; the depth window is what actually drives the cost.)

The max-CoC objective is the right cost for a "worst-case image quality"
criterion. It is **non-smooth** — the validity predicates flip discretely as a
trial moves — which is exactly why the solver below is derivative-free.

The CoC is computed by the M4 Scheimpflug formula (`coc_diameter_at_sensor`,
the same physics behind `ThickLens::coc_diameter_px`), which accounts for the
sensor tilt, the inter-principal gap, and the object depth in one formula.

## The solver: `argmin` Nelder-Mead

The optimisation uses [`argmin`](https://argmin-rs.github.io/argmin/)'s
**Nelder-Mead** simplex method, called from
`etendue_core::solver::scheimpflug::solve_scheimpflug`. Nelder-Mead is
derivative-free — the natural choice because the worst-case-CoC cost is
non-smooth and cheap to evaluate, so a gradient method would have no smooth
gradient to follow.

The initial simplex is anchored at the camera's current `(τ_x, τ_y, s_o)`
with small per-variable perturbations: 1° in each tilt, and 5 % of `d_opt`
(or 5 mm, whichever is larger) in focus. The solver runs with a
standard-deviation tolerance of `1e-6` and an iteration cap of 500.

There are no hard solver bounds. Instead the cost returns `+∞` for a
physically invalid trial — `s_o ≤ f`, or a tilt magnitude at or beyond
`π/2` — so the simplex retreats from the infeasible region on its own. A
smoothed-objective gradient pass is a possible future follow-up should the
simplex method prove brittle on real designs.

## SampleGrid

```rust
pub struct SampleGrid {
    pub rows: usize,   // default 32
    pub cols: usize,   // default 32
}
```

The grid is a `rows × cols` sampling of the **laser fan plane** in its
natural `(phi, r)` parameterisation — `rows` sweeps the fan angle, `cols`
the radius. Larger grids give more accurate worst-case estimates but cost
proportionally more cost-function evaluations. 32 × 32 (1 024 points)
completes in well under one frame on a typical development machine; larger
grids are straightforward but move the solver toward a background-thread
model.

## UI section

The parameter panel's **Scheimpflug solver (M9)** collapsible:

1. Three drag-value inputs: **Optimal distance**, **Depth min**, **Depth max**
   (all in metres; initialised from the camera's current focus on first
   open).
2. A **Solve** button — runs the solver synchronously and stores the result.
3. A result readout: proposed `τ_x`, `τ_y`, `s_o`, worst-case CoC, sample
   count, and iteration count.
4. **Apply** — writes the proposed tilt + focus into the camera and triggers
   a viewport rebuild. **Discard** — clears the pending result.

Applying calls `apply_solver_result`, which writes `camera.optics.focus_distance_m`
and sets `camera.params.sensor = SensorParams::Scheimpflug { … }` — the
same path the Scheimpflug-tilt sliders use, so the defocus heatmap updates
on the same frame.

## SolverResult

```rust
pub struct SolverResult {
    pub tau_x: f64,          // optimal tilt around x, radians
    pub tau_y: f64,          // optimal tilt around y, radians
    pub s_o:   f64,          // optimal focus distance, metres
    pub max_coc_px: f64,     // worst-case CoC over the sample grid, pixels
    pub n_valid_samples: usize,
    pub iterations: u32,
    pub converged: bool,
}
```

## Validation

Two convergence tests guard the solver:

- `fronto_parallel_fan_drives_tilt_to_zero_and_focus_to_d_opt` — when the
  laser fan is fronto-parallel (perpendicular to the camera axis) the
  optimal Scheimpflug tilt is zero and the optimal focus is `d_opt`; the
  solver must recover this.
- `solving_lowers_worst_case_coc` — the solver result must strictly reduce
  the worst-case CoC compared with the baseline (zero tilt, current focus).
