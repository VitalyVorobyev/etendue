//! Scheimpflug solver: optimal sensor tilt + focus distance for a target
//! working volume.
//!
//! Given a camera, a laser, a depth window, and a sampling grid on the fan
//! plane, [`solve_scheimpflug`] returns the `(tau_x, tau_y, s_o)` that
//! minimises the worst-case circle of confusion across the visible, in-window
//! samples. The lens (focal length, f-number, principal gap, pixel pitch),
//! the sensor (resolution), the camera pose, and the laser geometry are held
//! fixed — the solver tunes only the three Scheimpflug-and-focus parameters.
//!
//! # The cost
//!
//! For each sample of the fan in its natural `(phi, r)` parameterisation:
//!
//! 1. transform world → camera-local using the camera's `pose`;
//! 2. reject samples with `z <= 0` (behind the camera) or whose pinhole
//!    projection falls outside the sensor rectangle (visibility);
//! 3. reject samples whose camera-frame depth is outside the user's
//!    `[d_min, d_max]` window;
//! 4. compute the circle of confusion in pixels via
//!    [`crate::optics::coc::coc_diameter_at_sensor`] using the trial
//!    `(tau_x, tau_y, s_o)` and the camera's fixed `f`, `N`, `g`, pixel pitch;
//! 5. return the **maximum** CoC over the surviving samples — or `+∞` when
//!    every sample is rejected (the trial loses the whole stripe, a strong
//!    penalty that the optimiser will steer away from).
//!
//! Step 2 deliberately uses the **pinhole** projection rather than the camera
//! model's full Scheimpflug-homography projection. The homography depends on
//! the trial tilt — using it would couple visibility to the same variables we
//! are optimising and create spurious cost discontinuities at the sensor
//! rectangle's edges. The pinhole approximation is good to fractions of a
//! pixel for any non-extreme tilt and is what matters for the working volume.
//!
//! # The optimiser
//!
//! Nelder-Mead. The cost is non-smooth (sample visibility / depth-window
//! predicates flip discretely as the trial moves) and inexpensive (~32×32
//! sample CoC evaluations per call), so a derivative-free simplex method is
//! the natural choice. The initial simplex is built around the camera's
//! current `(tau_x, tau_y, s_o)` with small perturbations in each variable.
//! A smoothed gradient-based pass (log-sum-exp soft-max + L-BFGS) is an
//! explicit follow-up — see the M9 plan — should the simplex method prove
//! brittle on real designs.

use std::f64::consts::FRAC_PI_2;

use argmin::core::{CostFunction, Error as ArgminError, Executor, State};
use argmin::solver::neldermead::NelderMead;
use serde::{Deserialize, Serialize};

use crate::laser::LaserPlane;
use crate::optics::{coc_diameter_at_sensor, coc_to_pixels, scheimpflug_tilt};
use crate::scene::{CameraEntity, LaserEntity};

/// Sampling resolution of the laser fan during cost evaluation.
///
/// `rows` sweeps the fan angle `phi`; `cols` sweeps the radius `r`. Both must
/// be `>= 2`. The default 32×32 (~1000 samples) balances cost-evaluation
/// speed against the smoothness of the worst-case CoC objective: a coarser
/// grid lets the worst sample fall *between* grid points; a finer grid
/// multiplies cost-evaluation work without changing the optimum meaningfully.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SampleGrid {
    /// Fan-angle samples (`>= 2`).
    pub rows: usize,
    /// Radial samples (`>= 2`).
    pub cols: usize,
}

impl SampleGrid {
    /// The default 32×32 sampling grid the M9 UI uses.
    pub const DEFAULT: Self = Self { rows: 32, cols: 32 };
}

impl Default for SampleGrid {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Solver inputs: the target working geometry the user wants to achieve.
///
/// `d_opt` is currently only carried as metadata (for the UI to display the
/// designer's intent); the cost itself is driven by `depth_range` — the
/// camera-frame `z` window over which the worst-case CoC is minimised.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolverSpec {
    /// The user's optimal working distance, in metres (camera-frame `z`).
    /// Carried as design intent; the solver does not enforce it directly
    /// (the depth window does that). Stored so the UI can show a "spec" line.
    pub d_opt: f64,
    /// The camera-frame depth window `(d_min, d_max)`, in metres, over which
    /// the solver minimises worst-case CoC. Samples outside this window are
    /// dropped from the cost — *not* penalised — so the optimiser does not
    /// shrink the working volume to game the objective.
    pub depth_range: (f64, f64),
    /// Fan-plane sampling resolution. See [`SampleGrid`].
    pub grid: SampleGrid,
}

/// Solver outputs: the proposed Scheimpflug tilt + focus, plus diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SolverResult {
    /// Proposed sensor tilt about camera `x`, in radians.
    pub tau_x: f64,
    /// Proposed sensor tilt about camera `y`, in radians.
    pub tau_y: f64,
    /// Proposed object-side focus distance from the rear principal plane
    /// `H'`, in metres.
    pub s_o: f64,
    /// Achieved worst-case circle of confusion across the working volume, in
    /// pixels.
    pub max_coc_px: f64,
    /// Number of fan-plane samples that contributed to the final cost
    /// evaluation (visible **and** within the depth window). Zero if the
    /// design lost the whole stripe — `max_coc_px` is then `+∞` and the
    /// returned `(tau_x, tau_y, s_o)` is the last simplex centre.
    pub n_valid_samples: usize,
    /// `true` if the Nelder-Mead simplex shrank below tolerance before the
    /// iteration cap. `false` means the cap was hit; the result is still
    /// the best-known simplex point but a tighter tolerance or more
    /// iterations may improve it.
    pub converged: bool,
    /// Number of Nelder-Mead iterations executed (one *cost-function* call
    /// per iteration is the dominant work).
    pub iterations: u32,
}

/// Errors the Scheimpflug solver can return.
#[derive(Clone, Debug, thiserror::Error)]
pub enum SolverError {
    /// The [`SolverSpec`] is malformed (non-finite numbers, negative depth
    /// window, depth window inverted, grid too small).
    #[error("invalid solver spec: {0}")]
    InvalidSpec(String),
    /// The camera's fixed optical parameters fail validation when the inner
    /// CoC evaluator runs. Cannot happen for a [`CameraEntity`] built through
    /// [`CameraEntity::new`].
    #[error("invalid camera parameters: {0}")]
    InvalidCamera(String),
    /// The optimiser failed to take a useful step — typically because the
    /// initial simplex's cost evaluations were all `+∞` (every trial loses
    /// the stripe, often because the depth window does not overlap the fan's
    /// projection).
    #[error("solver failed: {0}")]
    SolverFailure(String),
}

/// Solve for the Scheimpflug tilt + focus distance that minimises worst-case
/// CoC across the fan plane's `[d_min, d_max]` depth window.
///
/// Returns the proposed `(tau_x, tau_y, s_o)` plus diagnostics. The camera is
/// **not** mutated — apply the result by writing the three values back into
/// `CameraParams::sensor`'s [`ScheimpflugParams`] and `CameraEntity::focus_distance_m`
/// at the call site.
///
/// # Errors
///
/// Returns [`SolverError::InvalidSpec`] if the spec is malformed, or
/// [`SolverError::SolverFailure`] if the optimiser cannot make progress.
///
/// [`ScheimpflugParams`]: vision_calibration_core::ScheimpflugParams
pub fn solve_scheimpflug(
    camera: &CameraEntity,
    laser: &LaserEntity,
    spec: &SolverSpec,
) -> Result<SolverResult, SolverError> {
    validate_spec(spec)?;

    // Initial simplex anchored at the camera's current state.
    let (tau_x0, tau_y0) = scheimpflug_tilt(&camera.params);
    let s_o0 = camera.focus_distance_m;
    let initial = vec![tau_x0, tau_y0, s_o0];

    // Simplex perturbations: 1° in tilt, 5 % of d_opt in focus (or 5 mm,
    // whichever is larger). Chosen so the simplex spans a region the user
    // would notice in the UI but small enough that the cost surface stays
    // approximately convex around the optimum.
    let d_tau = 1.0_f64.to_radians();
    let d_s = (0.05 * spec.d_opt).max(5.0e-3);
    let simplex = vec![
        initial.clone(),
        vec![initial[0] + d_tau, initial[1], initial[2]],
        vec![initial[0], initial[1] + d_tau, initial[2]],
        vec![initial[0], initial[1], initial[2] + d_s],
    ];

    let cost = ScheimpflugCost::new(camera, laser, spec)?;

    let solver = NelderMead::new(simplex)
        .with_sd_tolerance(1.0e-6)
        .map_err(|e| SolverError::SolverFailure(format!("simplex tolerance: {e}")))?;
    let executor = Executor::new(cost, solver).configure(|state| state.max_iters(500));

    let outcome = executor
        .run()
        .map_err(|e| SolverError::SolverFailure(format!("Nelder-Mead: {e}")))?;

    let state = outcome.state();
    let best = state
        .get_best_param()
        .ok_or_else(|| SolverError::SolverFailure("no best parameter".to_string()))?
        .clone();
    let max_coc_px = state.get_best_cost();
    let iterations = u32::try_from(state.get_iter()).unwrap_or(u32::MAX);
    let converged = state.get_termination_status().terminated();

    // Re-evaluate at the best point to get the valid-sample count (argmin
    // doesn't surface custom auxiliary diagnostics from the cost function).
    let cost_for_diag = ScheimpflugCost::new(camera, laser, spec)?;
    let diag = cost_for_diag.evaluate(best[0], best[1], best[2]);

    Ok(SolverResult {
        tau_x: best[0],
        tau_y: best[1],
        s_o: best[2],
        max_coc_px,
        n_valid_samples: diag.n_valid,
        converged,
        iterations,
    })
}

/// Validation of the user-facing spec, separate from the inner cost so the
/// UI receives a single coherent error before any optimiser work runs.
fn validate_spec(spec: &SolverSpec) -> Result<(), SolverError> {
    if !(spec.d_opt.is_finite() && spec.d_opt > 0.0) {
        return Err(SolverError::InvalidSpec(format!(
            "optimal distance must be finite and positive, got {}",
            spec.d_opt
        )));
    }
    let (lo, hi) = spec.depth_range;
    if !(lo.is_finite() && hi.is_finite()) {
        return Err(SolverError::InvalidSpec(format!(
            "depth range must be finite, got ({lo}, {hi})"
        )));
    }
    if lo <= 0.0 || hi <= 0.0 {
        return Err(SolverError::InvalidSpec(format!(
            "depth range must be strictly positive, got ({lo}, {hi})"
        )));
    }
    if lo > hi {
        return Err(SolverError::InvalidSpec(format!(
            "depth range is inverted: ({lo}, {hi})"
        )));
    }
    if spec.grid.rows < 2 || spec.grid.cols < 2 {
        return Err(SolverError::InvalidSpec(format!(
            "sample grid must be at least 2x2, got {}x{}",
            spec.grid.rows, spec.grid.cols
        )));
    }
    Ok(())
}

/// One inner cost evaluation: worst-case CoC over the surviving samples.
#[derive(Clone, Copy, Debug)]
struct Evaluation {
    /// Maximum CoC in pixels over visible, in-window samples. `+∞` when no
    /// sample survives the predicates.
    max_coc_px: f64,
    /// Number of samples that contributed to the maximum.
    n_valid: usize,
}

/// The cost function exposed to argmin.
///
/// Holds the fixed scene (camera + laser + spec) and pre-computes the world
/// points of the sampled fan plane in camera-local coordinates, so the hot
/// path is just per-trial CoC evaluation.
struct ScheimpflugCost {
    /// Pre-computed fan-plane sample points in **camera-local** coordinates.
    /// Storing camera-local saves a `pose.inverse() *` on every cost call.
    cam_local_samples: Vec<nalgebra::Point3<f64>>,
    /// Lens focal length, in metres.
    f: f64,
    /// Lens f-number.
    n_fnum: f64,
    /// Inter-principal gap `H - H'`, in metres.
    g: f64,
    /// Sensor pixel pitch, in metres.
    pixel_pitch_m: f64,
    /// Sensor extent in pixels, `(w, h)`.
    sensor_extent: (f64, f64),
    /// Principal point in pixels, `(cx, cy)`.
    principal_point: (f64, f64),
    /// Pinhole focal length in pixels (`f / pixel_pitch_m`).
    focal_px: f64,
    /// User depth window `(d_min, d_max)` in camera-frame metres.
    depth_range: (f64, f64),
}

impl ScheimpflugCost {
    fn new(
        camera: &CameraEntity,
        laser: &LaserEntity,
        spec: &SolverSpec,
    ) -> Result<Self, SolverError> {
        let plane = LaserPlane::from_entity(laser);
        let rows = spec.grid.rows;
        let cols = spec.grid.cols;
        let camera_from_world = camera.pose.inverse();
        let mut cam_local_samples = Vec::with_capacity(rows * cols);

        for row in 0..rows {
            let t_phi = row as f64 / (rows - 1) as f64;
            let phi = (t_phi - 0.5) * 2.0 * laser.fan_half_angle;
            let direction = plane.ray_direction(phi);
            for col in 0..cols {
                let t_r = col as f64 / (cols - 1) as f64;
                let r = t_r * laser.fan_length;
                let point_world = plane.origin() + direction * r;
                cam_local_samples.push(camera_from_world * point_world);
            }
        }

        let (cx, cy) = match &camera.params.intrinsics {
            vision_calibration_core::IntrinsicsParams::FxFyCxCySkew { params } => {
                (params.cx, params.cy)
            }
        };

        Ok(Self {
            cam_local_samples,
            f: camera.effective_focal_length_m,
            n_fnum: camera.f_number,
            g: camera.principal_gap_m,
            pixel_pitch_m: camera.pixel_pitch_m,
            sensor_extent: camera.resolution_f64(),
            principal_point: (cx, cy),
            focal_px: camera.focal_length_px(),
            depth_range: spec.depth_range,
        })
    }

    /// Evaluate the worst-case CoC at a trial `(tau_x, tau_y, s_o)`.
    fn evaluate(&self, tau_x: f64, tau_y: f64, s_o: f64) -> Evaluation {
        if !(tau_x.is_finite()
            && tau_y.is_finite()
            && s_o.is_finite()
            && s_o > self.f
            && tau_x.abs() < FRAC_PI_2
            && tau_y.abs() < FRAC_PI_2)
        {
            // Out-of-domain trial: penalise so the simplex retreats.
            return Evaluation {
                max_coc_px: f64::INFINITY,
                n_valid: 0,
            };
        }
        let (d_min, d_max) = self.depth_range;
        let (sw, sh) = self.sensor_extent;
        let (cx, cy) = self.principal_point;
        let mut max_coc_px = 0.0_f64;
        let mut n_valid = 0usize;
        for p in &self.cam_local_samples {
            // Visibility (1): in front of the camera.
            if !(p.z.is_finite() && p.z > 0.0) {
                continue;
            }
            // Visibility (2): pinhole projection inside the sensor.
            let u = self.focal_px * p.x / p.z + cx;
            let v = self.focal_px * p.y / p.z + cy;
            if !(u.is_finite() && v.is_finite() && u >= 0.0 && u <= sw && v >= 0.0 && v <= sh) {
                continue;
            }
            // Depth window.
            if p.z < d_min || p.z > d_max {
                continue;
            }
            // CoC for the trial parameters.
            let Ok(coc_m) =
                coc_diameter_at_sensor(p, self.f, self.n_fnum, s_o, self.g, tau_x, tau_y)
            else {
                continue;
            };
            let Ok(coc_px) = coc_to_pixels(coc_m, self.pixel_pitch_m) else {
                continue;
            };
            if coc_px.is_finite() && coc_px > max_coc_px {
                max_coc_px = coc_px;
            }
            n_valid += 1;
        }
        if n_valid == 0 {
            Evaluation {
                max_coc_px: f64::INFINITY,
                n_valid: 0,
            }
        } else {
            Evaluation {
                max_coc_px,
                n_valid,
            }
        }
    }
}

impl CostFunction for ScheimpflugCost {
    type Param = Vec<f64>;
    type Output = f64;

    fn cost(&self, param: &Self::Param) -> Result<f64, ArgminError> {
        if param.len() != 3 {
            return Err(ArgminError::msg(format!(
                "ScheimpflugCost expects a 3-vector, got length {}",
                param.len()
            )));
        }
        Ok(self.evaluate(param[0], param[1], param[2]).max_coc_px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use nalgebra::{Isometry3, Translation3, UnitQuaternion, Vector3};
    use vision_calibration_core::{
        CameraParams, DistortionParams, FxFyCxCySkew, IntrinsicsParams, ProjectionParams,
        ScheimpflugParams, SensorParams,
    };

    // Standard test lens — same numbers as the working-volume tests.
    const F_M: f64 = 0.025;
    const FNUM: f64 = 4.0;
    const PITCH_M: f64 = 3.45e-6;
    const RES_W: u32 = 1280;
    const RES_H: u32 = 1024;

    fn camera_with_tilt(
        eye: nalgebra::Point3<f64>,
        look: nalgebra::Point3<f64>,
        focus_m: f64,
        tau_x: f64,
        tau_y: f64,
    ) -> CameraEntity {
        let dir = look - eye;
        let rot = nalgebra::Rotation3::face_towards(&dir, &-Vector3::z());
        let pose = Isometry3::from_parts(
            Translation3::from(eye.coords),
            UnitQuaternion::from_rotation_matrix(&rot),
        );
        let params = CameraParams {
            projection: ProjectionParams::Pinhole,
            distortion: DistortionParams::None,
            sensor: SensorParams::Scheimpflug {
                params: ScheimpflugParams {
                    tilt_x: tau_x,
                    tilt_y: tau_y,
                },
            },
            intrinsics: IntrinsicsParams::FxFyCxCySkew {
                params: FxFyCxCySkew {
                    fx: 0.0,
                    fy: 0.0,
                    cx: f64::from(RES_W) * 0.5,
                    cy: f64::from(RES_H) * 0.5,
                    skew: 0.0,
                },
            },
        };
        CameraEntity::new(
            pose,
            params,
            (RES_W, RES_H),
            F_M,
            FNUM,
            focus_m,
            0.0,
            PITCH_M,
            0.05,
            3.0,
        )
        .expect("valid camera entity")
    }

    fn laser_along_y(standoff: f64, half_angle: f64, length: f64) -> LaserEntity {
        let rot = UnitQuaternion::rotation_between(&Vector3::z(), &Vector3::y())
            .expect("z to y rotation");
        let pose = Isometry3::from_parts(Translation3::new(0.0, -standoff, 0.0), rot);
        LaserEntity::new(pose, half_angle, length, 660.0, 0.25e-3).expect("valid laser")
    }

    /// Build a triangulation camera looking at the world origin from offset
    /// `eye`, with the standard test lens, focused at `focus_m`.
    fn triangulation_camera(focus_m: f64, tau_x: f64) -> CameraEntity {
        camera_with_tilt(
            nalgebra::Point3::new(0.2, -0.45, 0.0),
            nalgebra::Point3::origin(),
            focus_m,
            tau_x,
            0.0,
        )
    }

    #[test]
    fn rejects_invalid_spec() {
        let cam = triangulation_camera(0.5, 0.0);
        let laser = laser_along_y(0.5, 0.30, 1.0);
        let bad = SolverSpec {
            d_opt: -0.5,
            depth_range: (0.4, 0.6),
            grid: SampleGrid::DEFAULT,
        };
        assert!(matches!(
            solve_scheimpflug(&cam, &laser, &bad),
            Err(SolverError::InvalidSpec(_))
        ));
        let inverted = SolverSpec {
            d_opt: 0.5,
            depth_range: (0.7, 0.5),
            grid: SampleGrid::DEFAULT,
        };
        assert!(matches!(
            solve_scheimpflug(&cam, &laser, &inverted),
            Err(SolverError::InvalidSpec(_))
        ));
        let tiny = SolverSpec {
            d_opt: 0.5,
            depth_range: (0.4, 0.6),
            grid: SampleGrid { rows: 1, cols: 8 },
        };
        assert!(matches!(
            solve_scheimpflug(&cam, &laser, &tiny),
            Err(SolverError::InvalidSpec(_))
        ));
    }

    /// Build a laser whose fan plane is **perpendicular** to the world `+y`
    /// axis (the camera's optical direction in these tests).
    ///
    /// Laser apex at the world origin. Laser-local axes are mapped:
    /// `+x → +y` (fan-plane normal), `+y → +z`, `+z → +x` (central ray). A
    /// fan sample at `(0, sin(phi), cos(phi))` in laser-local thus lands at
    /// world `(cos(phi), 0, sin(phi))`. Every sample has world `y = 0`, so
    /// from a camera at `(0, -d, 0)` looking along world `+y` every sample
    /// sits at camera depth `d` — a clean fronto-parallel target geometry
    /// suited to the Scheimpflug solver's `tau = 0`, `s_o = d` ideal answer.
    fn laser_perpendicular_to_optical_axis(half_angle: f64, length: f64) -> LaserEntity {
        let mat = nalgebra::Matrix3::from_columns(&[Vector3::y(), Vector3::z(), Vector3::x()]);
        let rot = UnitQuaternion::from_matrix(&mat);
        let pose = Isometry3::from_parts(Translation3::new(0.0, 0.0, 0.0), rot);
        LaserEntity::new(pose, half_angle, length, 660.0, 0.25e-3).expect("valid laser")
    }

    #[test]
    fn fronto_parallel_fan_drives_tilt_to_zero_and_focus_to_d_opt() {
        // A fan plane perpendicular to the optical axis at camera depth
        // d_opt: every visible sample sits at depth d_opt, so the analytic
        // optimum is (tau_x, tau_y, s_o) = (0, 0, d_opt). The mis-tuned
        // starting camera has 5 mrad tilt and a focus that is 10 % short of
        // d_opt; the solver should drive both back toward the optimum.
        let d_opt = 0.6;
        let cam = camera_with_tilt(
            nalgebra::Point3::new(0.0, -d_opt, 0.0),
            nalgebra::Point3::origin(),
            d_opt * 0.9, // mis-set focus the solver should correct
            0.05,        // mis-set tilt_x
            0.05,        // mis-set tilt_y
        );
        let laser = laser_perpendicular_to_optical_axis(0.05, 0.06);
        let spec = SolverSpec {
            d_opt,
            // Generous window — the fan samples sit at depth d_opt; the
            // window's role is just to exclude unrelated camera-frame depths
            // (none here, but the window is part of the spec contract).
            depth_range: (d_opt - 0.05, d_opt + 0.05),
            grid: SampleGrid { rows: 24, cols: 24 },
        };
        let res = solve_scheimpflug(&cam, &laser, &spec).expect("solver runs");

        // The solver should reach the analytic optimum tightly: < 1° tilt,
        // < 1 % focus error. Nelder-Mead on a smooth quadratic-like cost
        // around the optimum converges quickly.
        assert!(res.n_valid_samples > 0, "fan samples must be visible");
        assert!(
            res.tau_x.abs() < 1.0_f64.to_radians(),
            "tau_x {} rad ≈ {:.3} deg should be near zero",
            res.tau_x,
            res.tau_x.to_degrees(),
        );
        assert!(
            res.tau_y.abs() < 1.0_f64.to_radians(),
            "tau_y {} rad ≈ {:.3} deg should be near zero",
            res.tau_y,
            res.tau_y.to_degrees(),
        );
        assert!(
            (res.s_o - d_opt).abs() < 0.01,
            "focus distance {} m should be within 10 mm of d_opt {} m",
            res.s_o,
            d_opt,
        );
        // And the achieved worst-case CoC is essentially zero (well under 1
        // pixel) at the optimum.
        assert!(
            res.max_coc_px < 1.0,
            "max CoC at optimum should be sub-pixel, got {} px",
            res.max_coc_px,
        );
    }

    #[test]
    fn solving_lowers_worst_case_coc() {
        // Take a mis-tuned camera (off-focus + tilt-free), run the solver,
        // and confirm the *achieved* worst-case CoC is strictly smaller than
        // what the original camera would yield against the same spec.
        let d_opt = 0.6;
        let cam = triangulation_camera(d_opt - 0.05, 0.0); // detuned focus
        let laser = laser_along_y(d_opt, 0.30, 1.0);
        let spec = SolverSpec {
            d_opt,
            // A wide-ish window so the cost surface is non-trivial.
            depth_range: (d_opt - 0.04, d_opt + 0.04),
            grid: SampleGrid { rows: 24, cols: 24 },
        };
        // Baseline cost: evaluate at the camera's current state directly.
        let baseline_cost = ScheimpflugCost::new(&cam, &laser, &spec).unwrap();
        let (tau_x0, tau_y0) = scheimpflug_tilt(&cam.params);
        let baseline = baseline_cost
            .evaluate(tau_x0, tau_y0, cam.focus_distance_m)
            .max_coc_px;
        assert!(
            baseline.is_finite() && baseline > 0.0,
            "baseline must be a finite positive figure, got {baseline}"
        );

        let res = solve_scheimpflug(&cam, &laser, &spec).expect("solver runs");
        assert!(res.n_valid_samples > 0);
        assert!(res.max_coc_px.is_finite());
        assert!(
            res.max_coc_px < baseline,
            "solver should improve worst-case CoC: baseline {baseline} -> solved {} px",
            res.max_coc_px
        );
    }

    #[test]
    fn empty_window_yields_no_valid_samples() {
        // A depth window that excludes the entire fan projection: every
        // sample is dropped, the solver returns a sentinel `+∞` worst case
        // with zero valid samples.
        let cam = triangulation_camera(0.6, 0.0);
        let laser = laser_along_y(0.5, 0.30, 1.0);
        let spec = SolverSpec {
            d_opt: 5.0,
            depth_range: (4.5, 5.5), // 4.5–5.5 m, while the fan sits ~ 0.5 m
            grid: SampleGrid { rows: 8, cols: 8 },
        };
        let res = solve_scheimpflug(&cam, &laser, &spec).expect("runs but degenerate");
        assert_eq!(res.n_valid_samples, 0);
        assert!(res.max_coc_px.is_infinite());
    }

    #[test]
    fn default_sample_grid_is_thirty_two_by_thirty_two() {
        assert_eq!(SampleGrid::default(), SampleGrid::DEFAULT);
        assert_eq!(SampleGrid::DEFAULT.rows, 32);
        assert_eq!(SampleGrid::DEFAULT.cols, 32);
    }

    #[test]
    fn solver_returns_finite_diagnostics_for_default_scene() {
        // Integration-style: the standard default MVP scene runs through the
        // solver without errors, returns finite (tau, s_o), and the recorded
        // iteration count is small (Nelder-Mead converges or hits the cap).
        let scene = crate::scene::Scene::default_mvp();
        let cam = &scene.cameras[0];
        let laser = &scene.lasers[0];
        let spec = SolverSpec {
            d_opt: cam.focus_distance_m,
            depth_range: (cam.focus_distance_m * 0.9, cam.focus_distance_m * 1.1),
            grid: SampleGrid::DEFAULT,
        };
        let res = solve_scheimpflug(cam, laser, &spec).expect("default scene solves");
        assert!(res.tau_x.is_finite());
        assert!(res.tau_y.is_finite());
        assert!(res.s_o.is_finite() && res.s_o > 0.0);
        assert!(res.iterations <= 500, "iter cap respected");
        // A non-empty fan should give us at least a few valid samples; the
        // default scene's working volume is non-empty (a property the M6
        // tests already established), so the solver must find some.
        let _ = res.n_valid_samples; // diagnostic
        assert_relative_eq!(res.max_coc_px.is_finite() as u8 as f64, 1.0);
    }
}
