//! The robot pose error a calibration's data support when its residuals are view-correlated
//! (P20c2).
//!
//! calibration-rs estimates the robot's pose error from its pose corrections by variance-component
//! estimation (`robot_pose_sigma`): per group (rotation, translation) `σ̂² = Σ δ̂² / Σ r`, with
//! `r = 1 − Var(δ̂)/σ²` each correction's redundancy, `Var(δ̂)` taken from the white-noise
//! covariance. Each view's correction `δ̂ = (I + P)⁻¹ (I δ + g)` (`I` the view's pixel information
//! on it, `P` the prior's, `g` the pixels' score) carries the robot's true error `δ` and the part
//! of the pixel error that looks like a pose. With white pixels `Cov(g) = I`, and the estimate is
//! unbiased. When the pixel error is correlated within a view (calibration-rs's view scatter
//! `k > 1`), `Cov(g) = k I`: the corrections carry `k` times the pixel part the redundancy allows
//! for, and the estimate attributes the excess to the robot,
//! `E[Σ δ̂²] = Σ [M D Mᵀ] + k Σ [(I + P)⁻¹ I (I + P)⁻¹]`, `M = (I + P)⁻¹ I` and `D` the true error's
//! covariance, where calibration-rs's fixed point equates it with `σ̂² Σ r`.
//! [`Prediction::robot_prior_under_view_scatter`] solves that for `D`: the robot error the
//! corrections support once the view-correlated pixel error is taken out. At `k = 1` it returns
//! the estimate itself.

use nalgebra::{Matrix2, Matrix6, Vector2, Vector6};

use super::information::RobotPrior;
use super::predict::Prediction;

/// The lowest robot error returned, as calibration-rs floors its estimate: 1e-5 rad, 1 µm.
const FLOOR: [f64; 2] = [1e-5, 1e-6];

impl Prediction {
    /// The robot pose error the views' pose corrections support when the pixel error is
    /// correlated within views by `view_scatter` (`k`), given that the calibration estimated
    /// [`Self`]'s robot prior from them with white weights (see the module docs). Each group's σ
    /// is floored at 1e-5 rad and 1 µm. `None` for a prediction with exact robot poses.
    #[must_use]
    pub fn robot_prior_under_view_scatter(&self, view_scatter: f64) -> Option<RobotPrior> {
        let (estimate, poses) = self.robot.as_ref()?;
        let p = estimate.information()?;
        let sigma2 = [
            estimate.rot_sigma_rad * estimate.rot_sigma_rad,
            estimate.trans_sigma_m * estimate.trans_sigma_m,
        ];
        let group = |k: usize| k / 3;
        // Per group: Σ r, the pixel part Σ (S⁻¹ I S⁻¹)ₖₖ, and the robot part's coefficients.
        let mut redundancy = [0.0; 2];
        let mut pixel = [0.0; 2];
        let mut robot = Matrix2::<f64>::zeros();
        for i in poses {
            let s = i + Matrix6::from_diagonal(&Vector6::from(p));
            let s_inv = s.cholesky()?.inverse();
            let m = s_inv * i;
            let white = m * s_inv;
            for k in 0..6 {
                redundancy[group(k)] += 1.0 - s_inv[(k, k)] * p[k];
                pixel[group(k)] += white[(k, k)];
                for j in 0..6 {
                    robot[(group(k), group(j))] += m[(k, j)] * m[(k, j)];
                }
            }
        }
        let observed = Vector2::from_fn(|g, _| sigma2[g] * redundancy[g] - view_scatter * pixel[g]);
        let floor = |g: usize| FLOOR[g] * FLOOR[g];
        let mut d = robot.lu().solve(&observed)?;
        // A group below its floor is held there, and the other solved from its own row.
        for g in 0..2 {
            if d[g] < floor(g) {
                let h = 1 - g;
                d[g] = floor(g);
                d[h] = ((observed[h] - robot[(h, g)] * d[g]) / robot[(h, h)]).max(floor(h));
            }
        }
        Some(RobotPrior {
            rot_sigma_rad: d[0].sqrt(),
            trans_sigma_m: d[1].sqrt(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::predict::tests::truth;
    use super::super::{NoiseModel, predict};
    use super::*;

    const ROBOT: RobotPrior = RobotPrior {
        rot_sigma_rad: 1e-3,
        trans_sigma_m: 1e-3,
    };

    fn estimated() -> NoiseModel {
        NoiseModel {
            robot: Some(ROBOT),
            ..NoiseModel::white(0.5)
        }
    }

    #[test]
    fn white_views_leave_the_estimate_as_it_is() {
        let p = predict(&truth(), &estimated()).unwrap();
        let same = p.robot_prior_under_view_scatter(1.0).unwrap();
        assert!((same.rot_sigma_rad / ROBOT.rot_sigma_rad - 1.0).abs() < 1e-9);
        assert!((same.trans_sigma_m / ROBOT.trans_sigma_m - 1.0).abs() < 1e-9);
        let exact = predict(&truth(), &NoiseModel::white(0.5)).unwrap();
        assert!(exact.robot_prior_under_view_scatter(4.0).is_none());
    }

    #[test]
    fn view_correlated_pixel_error_is_taken_out_of_the_robots() {
        let p = predict(&truth(), &estimated()).unwrap();
        let mut last = ROBOT;
        for k in [2.0, 8.0, 32.0, 1e9] {
            let r = p.robot_prior_under_view_scatter(k).unwrap();
            assert!(r.rot_sigma_rad <= last.rot_sigma_rad, "{k}: {r:?}");
            assert!(r.trans_sigma_m <= last.trans_sigma_m, "{k}: {r:?}");
            last = r;
        }
        // Past what the corrections hold, the floor.
        assert_eq!(last.rot_sigma_rad, FLOOR[0]);
        assert_eq!(last.trans_sigma_m, FLOOR[1]);
    }

    #[test]
    fn a_learnt_model_without_view_scatter_is_the_calibrations_own() {
        let gt = truth();
        for k in [None, Some(1.0), Some(0.7)] {
            let m = NoiseModel::learnt(&gt, 0.5, Some(ROBOT), k).unwrap();
            assert_eq!(m, estimated());
        }
        let m = NoiseModel::learnt(&gt, 0.5, Some(ROBOT), Some(4.0)).unwrap();
        assert_eq!(m.view_scatter, 4.0);
        assert!(m.robot.unwrap().rot_sigma_rad < ROBOT.rot_sigma_rad);
        assert!(
            NoiseModel::learnt(&gt, 0.5, None, Some(4.0))
                .unwrap()
                .robot
                .is_none()
        );
        // Every σ grows with the per-view term: by √k with exact robot poses.
        let all: Vec<usize> = (0..gt.captures.len()).collect();
        let sigma = |m: &NoiseModel| predict(&gt, m).unwrap().system_sigma_mm(&all).unwrap();
        let white = NoiseModel::white(0.5);
        let k4 = NoiseModel {
            view_scatter: 4.0,
            ..white
        };
        assert!((sigma(&k4) / sigma(&white) - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_view_scatter_below_one_is_refused() {
        let m = NoiseModel {
            view_scatter: 0.5,
            ..NoiseModel::white(0.5)
        };
        assert!(predict(&truth(), &m).is_err());
    }
}
