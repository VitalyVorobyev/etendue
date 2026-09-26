//! Damped-least-squares inverse kinematics (Levenberg–Marquardt).
//!
//! Minimises the 6-D TCP pose error `e = [p* − p; log(R* Rᵀ)]` (base frame,
//! metres and radians, unweighted) with the update
//! `Δq = Jᵀ (J Jᵀ + λ² I)⁻¹ e`. The damping `λ` adapts: an accepted step
//! shrinks it towards Gauss–Newton, a rejected step grows it towards
//! gradient descent. Iterates are clamped to the joint limits, so a solution
//! always respects them.
//!
//! A local method can stall in a local minimum, typically when the joint
//! limits block the way to the solution. [`IkOptions::restarts`] then retries
//! from quasi-random seeds drawn from a fixed-seed generator. The solver is
//! deterministic: same model, target, seed, and options give bit-identical
//! results.

use nalgebra::{Isometry3, Matrix6, Vector6};

use crate::error::{Error, Result};
use crate::urdf::RobotModel;

/// Tuning of [`RobotModel::ik`].
#[derive(Clone, Copy, Debug)]
pub struct IkOptions {
    /// Iteration cap (accepted plus rejected steps).
    pub max_iterations: usize,
    /// Converged when the TCP position error is at most this (metres) …
    pub tol_translation: f64,
    /// … and the TCP rotation error angle at most this (radians).
    pub tol_rotation: f64,
    /// Starting damping `λ`.
    pub initial_damping: f64,
    /// Largest joint change per step, ∞-norm (rad or m).
    pub max_step: f64,
    /// Extra attempts from deterministic quasi-random seeds (uniform within
    /// the joint limits) when the attempt from the caller's seed fails. `0`
    /// keeps the solver strictly local, which a continuous path needs.
    pub restarts: usize,
}

impl Default for IkOptions {
    fn default() -> Self {
        Self {
            max_iterations: 500,
            tol_translation: 1e-10,
            tol_rotation: 1e-10,
            initial_damping: 1e-2,
            max_step: 0.3,
            restarts: 16,
        }
    }
}

/// A converged inverse-kinematics solution.
#[derive(Clone, Debug)]
pub struct IkSolution {
    /// Joint vector, inside the joint limits.
    pub q: Vec<f64>,
    /// Iterations used, summed over all attempts.
    pub iterations: usize,
    /// Attempt that converged: `0` = from the caller's seed, `k` = restart `k`.
    pub attempt: usize,
    /// Final TCP position error, metres.
    pub translation_error: f64,
    /// Final TCP rotation error angle, radians.
    pub rotation_error: f64,
}

/// SplitMix64, for deterministic restart seeds.
struct SplitMix64(u64);

impl SplitMix64 {
    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

fn pose_error(target: &Isometry3<f64>, current: &Isometry3<f64>) -> Vector6<f64> {
    let dp = target.translation.vector - current.translation.vector;
    let dr = (target.rotation * current.rotation.inverse()).scaled_axis();
    Vector6::new(dp.x, dp.y, dp.z, dr.x, dr.y, dr.z)
}

impl RobotModel {
    fn clamp_to_limits(&self, q: &mut [f64]) {
        for (v, j) in q.iter_mut().zip(&self.active) {
            *v = v.clamp(j.lower, j.upper);
        }
    }

    /// Solve `tcp_pose(q) = base_se3_tcp` starting from `seed`.
    ///
    /// The seed is clamped to the joint limits first. The first attempt
    /// returns the solution the damped iteration reaches from the seed
    /// (typically the nearest one); only if it fails are up to
    /// [`IkOptions::restarts`] further seeds tried.
    ///
    /// # Errors
    ///
    /// - [`Error::InvalidJoints`] if `seed` has the wrong length or a
    ///   non-finite entry;
    /// - [`Error::IkFailed`] if the tolerances are not met within
    ///   `max_iterations` (unreachable pose, joint limits, or a poor seed).
    pub fn ik(
        &self,
        base_se3_tcp: &Isometry3<f64>,
        seed: &[f64],
        opts: &IkOptions,
    ) -> Result<IkSolution> {
        if seed.len() != self.dof() || !seed.iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidJoints(format!(
                "IK seed must have {} finite values",
                self.dof()
            )));
        }
        let mut iterations = 0;
        let mut rng = SplitMix64(0x0e7e_4d0e_5eed_0001);
        let mut last_err = None;
        for attempt in 0..=opts.restarts {
            let start: Vec<f64> = if attempt == 0 {
                seed.to_vec()
            } else {
                self.active
                    .iter()
                    .map(|j| j.lower + (j.upper - j.lower) * rng.unit())
                    .collect()
            };
            match self.ik_attempt(base_se3_tcp, start, opts) {
                (Ok(mut sol), used) => {
                    sol.iterations = iterations + used;
                    sol.attempt = attempt;
                    return Ok(sol);
                }
                (Err(e), used) => {
                    iterations += used;
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.expect("at least one attempt"))
    }

    /// One damped-least-squares run; returns the result and the iterations
    /// used.
    fn ik_attempt(
        &self,
        base_se3_tcp: &Isometry3<f64>,
        mut q: Vec<f64>,
        opts: &IkOptions,
    ) -> (Result<IkSolution>, usize) {
        self.clamp_to_limits(&mut q);
        let (pose, mut jac) = self.tcp_jacobian(&q);
        let mut err = pose_error(base_se3_tcp, &pose);
        let mut lambda = opts.initial_damping;
        let converged = |e: &Vector6<f64>| {
            e.fixed_rows::<3>(0).norm() <= opts.tol_translation
                && e.fixed_rows::<3>(3).norm() <= opts.tol_rotation
        };

        let mut used = opts.max_iterations;
        for iteration in 0..opts.max_iterations {
            if converged(&err) {
                return (
                    Ok(IkSolution {
                        q,
                        iterations: iteration,
                        attempt: 0,
                        translation_error: err.fixed_rows::<3>(0).norm(),
                        rotation_error: err.fixed_rows::<3>(3).norm(),
                    }),
                    iteration,
                );
            }
            let jjt: Matrix6<f64> = (&jac * jac.transpose())
                .fixed_view::<6, 6>(0, 0)
                .into_owned()
                + Matrix6::identity() * (lambda * lambda);
            let Some(y) = jjt.cholesky().map(|c| c.solve(&err)) else {
                lambda *= 10.0;
                continue;
            };
            let mut dq = jac.transpose() * y;
            let step = dq.amax();
            if step > opts.max_step {
                dq *= opts.max_step / step;
            }
            let mut trial: Vec<f64> = q.iter().zip(dq.iter()).map(|(a, b)| a + b).collect();
            self.clamp_to_limits(&mut trial);
            let (trial_pose, trial_jac) = self.tcp_jacobian(&trial);
            let trial_err = pose_error(base_se3_tcp, &trial_pose);
            if trial_err.norm_squared() < err.norm_squared() {
                q = trial;
                jac = trial_jac;
                err = trial_err;
                lambda = (lambda * 0.3).max(1e-12);
            } else {
                lambda *= 5.0;
                if lambda > 1e6 {
                    used = iteration + 1;
                    break;
                }
            }
        }
        if converged(&err) {
            return (
                Ok(IkSolution {
                    q,
                    iterations: used,
                    attempt: 0,
                    translation_error: err.fixed_rows::<3>(0).norm(),
                    rotation_error: err.fixed_rows::<3>(3).norm(),
                }),
                used,
            );
        }
        (
            Err(Error::IkFailed(format!(
                "robot `{}`: residual {:.3e} m / {:.3e} rad after {} restart(s)",
                self.id,
                err.fixed_rows::<3>(0).norm(),
                err.fixed_rows::<3>(3).norm(),
                opts.restarts
            ))),
            used,
        )
    }
}
