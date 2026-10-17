//! Motion primitives: rest-to-rest, jerk-limited profiles of a path
//! parameter (Ruckig) and natural cubic splines with exact derivative
//! bounds.

use rsruckig::prelude::{DataArrayOrVec, InputParameter, Ruckig, ThrowErrorHandler, Trajectory};

/// A rest-to-rest profile moving a path parameter `s` from 0 to 1, with
/// `|ṡ| ≤ v`, `|s̈| ≤ a` and `|s⃛| ≤ j` — time-optimal (Ruckig), or slowed
/// down to a longer duration with the same limits.
///
/// An infinite `j` gives the second-order (trapezoidal) profile.
#[derive(Clone, Debug)]
pub(crate) struct PathProfile {
    limits: [f64; 3],
    trajectory: Trajectory<1>,
}

impl PathProfile {
    fn solve(limits: [f64; 3], minimum_duration: Option<f64>) -> std::result::Result<Self, String> {
        let [v, a, j] = limits;
        // The control cycle only matters to `update`; `calculate` is continuous.
        let mut otg = Ruckig::<1, ThrowErrorHandler>::new(None, 1e-3);
        let mut input = InputParameter::<1>::new(None);
        input.current_position[0] = 0.0;
        input.target_position[0] = 1.0;
        input.max_velocity[0] = v;
        input.max_acceleration[0] = a;
        input.max_jerk[0] = j;
        input.minimum_duration = minimum_duration;
        let mut trajectory = Trajectory::<1>::new(None);
        otg.calculate(&input, &mut trajectory)
            .map_err(|e| format!("motion profile (v {v}, a {a}, j {j}): {e}"))?;
        Ok(Self { limits, trajectory })
    }

    /// The fastest profile (all limits `> 0`; `j` may be infinite).
    pub(crate) fn fastest(v: f64, a: f64, j: f64) -> std::result::Result<Self, String> {
        Self::solve([v, a, j], None)
    }

    /// Duration, seconds.
    pub(crate) fn duration(&self) -> f64 {
        self.trajectory.get_duration()
    }

    /// The same move taking `duration ≥ self.duration()`, within the same
    /// limits.
    pub(crate) fn stretched(&self, duration: f64) -> std::result::Result<Self, String> {
        debug_assert!(duration >= self.duration());
        Self::solve(self.limits, Some(duration))
    }

    /// `[s, ṡ, s̈, s⃛]` at time `t` (clamped to `[0, duration]`).
    pub(crate) fn eval(&self, t: f64) -> [f64; 4] {
        let t = t.clamp(0.0, self.duration());
        let mut state = [0.0; 4].map(|v| DataArrayOrVec::Stack([v]));
        let [p, v, a, j] = &mut state;
        self.trajectory.at_time(
            t,
            &mut Some(p),
            &mut Some(v),
            &mut Some(a),
            &mut Some(j),
            &mut None,
        );
        state.map(|x| x[0])
    }
}

/// A natural cubic spline through `values` at uniform knots
/// `s_k = k / (n − 1)` on `[0, 1]`.
#[derive(Clone, Debug)]
pub(crate) struct Spline {
    h: f64,
    y: Vec<f64>,
    /// Second derivatives at the knots (`0` at both ends).
    m: Vec<f64>,
}

impl Spline {
    /// Interpolate `values` (at least two).
    pub(crate) fn natural(values: Vec<f64>) -> Self {
        let n = values.len();
        assert!(n >= 2, "a spline needs at least two knots");
        let h = 1.0 / (n - 1) as f64;
        let mut m = vec![0.0; n];
        if n > 2 {
            // Thomas algorithm for M_{k-1} + 4 M_k + M_{k+1} = 6/h² Δ²y_k,
            // k = 1..n-2, with M_0 = M_{n-1} = 0.
            let rhs: Vec<f64> = (1..n - 1)
                .map(|k| 6.0 / (h * h) * (values[k + 1] - 2.0 * values[k] + values[k - 1]))
                .collect();
            let len = rhs.len();
            let mut c = vec![0.0; len];
            let mut d = vec![0.0; len];
            c[0] = 1.0 / 4.0;
            d[0] = rhs[0] / 4.0;
            for i in 1..len {
                let denom = 4.0 - c[i - 1];
                c[i] = 1.0 / denom;
                d[i] = (rhs[i] - d[i - 1]) / denom;
            }
            m[len] = d[len - 1];
            for i in (0..len - 1).rev() {
                m[i + 1] = d[i] - c[i] * m[i + 2];
            }
        }
        Self { h, y: values, m }
    }

    fn segment(&self, s: f64) -> (usize, f64) {
        let last = self.y.len() - 2;
        let k = ((s / self.h).floor().max(0.0) as usize).min(last);
        (k, s - k as f64 * self.h)
    }

    /// `(y, y', y'')` at `s ∈ [0, 1]`.
    pub(crate) fn eval(&self, s: f64) -> (f64, f64, f64) {
        let (k, u) = self.segment(s.clamp(0.0, 1.0));
        let h = self.h;
        let w = h - u;
        let (m0, m1, y0, y1) = (self.m[k], self.m[k + 1], self.y[k], self.y[k + 1]);
        let a0 = y0 / h - m0 * h / 6.0;
        let a1 = y1 / h - m1 * h / 6.0;
        // Exactly on a knot, return the knot value itself: the cubic formula
        // reproduces it only to an ULP, and a motion must start exactly where
        // the previous step (or capture) left the robot.
        let y = if u == 0.0 {
            y0
        } else {
            m0 * w * w * w / (6.0 * h) + m1 * u * u * u / (6.0 * h) + a0 * w + a1 * u
        };
        let yd = -m0 * w * w / (2.0 * h) + m1 * u * u / (2.0 * h) - a0 + a1;
        let ydd = (m0 * w + m1 * u) / h;
        (y, yd, ydd)
    }

    /// The third derivative at `s ∈ [0, 1]`: constant on each segment.
    pub(crate) fn third(&self, s: f64) -> f64 {
        let (k, _) = self.segment(s.clamp(0.0, 1.0));
        (self.m[k + 1] - self.m[k]) / self.h
    }

    /// Exact max of the third derivative's magnitude over `[0, 1]`.
    pub(crate) fn third_bound(&self) -> f64 {
        self.m
            .windows(2)
            .fold(0.0_f64, |acc, w| acc.max((w[1] - w[0]).abs() / self.h))
    }

    /// Exact `max |y'|` and `max |y''|` over `[0, 1]`.
    pub(crate) fn derivative_bounds(&self) -> (f64, f64) {
        let h = self.h;
        let max2 = self.m.iter().fold(0.0_f64, |acc, v| acc.max(v.abs()));
        let mut max1 = 0.0_f64;
        for k in 0..self.y.len() - 1 {
            let s0 = k as f64 * h;
            let (_, d0, _) = self.eval(s0);
            let (_, d1, _) = self.eval(s0 + h);
            max1 = max1.max(d0.abs()).max(d1.abs());
            // y'' is linear on the segment; y' has an extremum where it
            // crosses zero.
            let (m0, m1) = (self.m[k], self.m[k + 1]);
            if m0 * m1 < 0.0 {
                let u = h * m0 / (m0 - m1);
                let (_, d, _) = self.eval(s0 + u);
                max1 = max1.max(d.abs());
            }
        }
        (max1, max2)
    }

    /// Exact `(min y, max y)` over `[0, 1]`.
    pub(crate) fn range(&self) -> (f64, f64) {
        let h = self.h;
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for k in 0..self.y.len() - 1 {
            let s0 = k as f64 * h;
            let mut consider = |s: f64| {
                let (y, _, _) = self.eval(s);
                lo = lo.min(y);
                hi = hi.max(y);
            };
            consider(s0);
            consider(s0 + h);
            // y'(u) = A u² + B u + C on the segment, u ∈ (0, h).
            let (m0, m1) = (self.m[k], self.m[k + 1]);
            let c = (self.y[k + 1] - self.y[k]) / h - (m1 - m0) * h / 6.0;
            let (qa, qb, qc) = ((m1 - m0) / (2.0 * h), m0, c - m0 * h / 2.0);
            let mut roots = [f64::NAN; 2];
            if qa.abs() < 1e-300 {
                if qb != 0.0 {
                    roots[0] = -qc / qb;
                }
            } else {
                let disc = qb * qb - 4.0 * qa * qc;
                if disc >= 0.0 {
                    let sq = disc.sqrt();
                    let q = -0.5 * (qb + qb.signum() * sq);
                    roots[0] = q / qa;
                    if q != 0.0 {
                        roots[1] = qc / q;
                    }
                }
            }
            for u in roots {
                if u > 0.0 && u < h {
                    consider(s0 + u);
                }
            }
        }
        (lo, hi)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every profile reaches 1 at rest, within its limits, also when
    /// stretched; an infinite jerk gives the trapezoid.
    #[test]
    fn path_profiles_reach_one_at_rest_within_limits() {
        for (v, a, j) in [
            (1.0, 1.0, 10.0),
            (0.5, 4.0, 40.0),
            (3.0, 0.7, 7.0),
            (10.0, 100.0, 1000.0),
            (1.0, 2.0, f64::INFINITY),
            (1e6, 1e7, 1e8),
        ] {
            let fast = PathProfile::fastest(v, a, j).unwrap();
            for p in [fast.clone(), fast.stretched(1.7 * fast.duration()).unwrap()] {
                let [s, sd, sdd, _] = p.eval(p.duration());
                assert!(
                    (s - 1.0).abs() < 1e-9 && sd.abs() < 1e-9 && sdd.abs() < 1e-9,
                    "{v} {a} {j}"
                );
                assert_eq!(p.eval(0.0)[0], 0.0);
                let n = 10_000;
                for i in 0..=n {
                    let [_, sd, sdd, sddd] = p.eval(p.duration() * i as f64 / n as f64);
                    assert!(sd <= v * (1.0 + 1e-9), "v {sd} > {v}");
                    assert!(sdd.abs() <= a * (1.0 + 1e-9), "a {sdd} > {a}");
                    assert!(sddd.abs() <= j * (1.0 + 1e-9), "j {sddd} > {j}");
                }
            }
        }
        // The trapezoid's duration: v²/a < 1 here, so 1/v + v/a.
        let t = PathProfile::fastest(1.0, 2.0, f64::INFINITY)
            .unwrap()
            .duration();
        assert!((t - 1.5).abs() < 1e-9, "{t}");
    }

    #[test]
    fn spline_interpolates_and_bounds_are_exact() {
        let ys: Vec<f64> = (0..=20).map(|k| (k as f64 * 0.3).sin()).collect();
        let sp = Spline::natural(ys.clone());
        for (k, y) in ys.iter().enumerate() {
            let (v, _, _) = sp.eval(k as f64 / 20.0);
            assert!((v - y).abs() < 1e-12);
        }
        // Bit-exact at the start knot (where motions begin).
        assert_eq!(sp.eval(0.0).0, ys[0]);
        let (d1, d2) = sp.derivative_bounds();
        let (lo, hi) = sp.range();
        let n = 200_000;
        for i in 0..=n {
            let (y, yd, ydd) = sp.eval(i as f64 / n as f64);
            assert!(yd.abs() <= d1 * (1.0 + 1e-9) + 1e-12);
            assert!(ydd.abs() <= d2 * (1.0 + 1e-9) + 1e-12);
            assert!(y >= lo - 1e-12 && y <= hi + 1e-12);
        }
    }

    #[test]
    fn two_knot_spline_is_linear() {
        let sp = Spline::natural(vec![1.0, 3.0]);
        let (y, yd, ydd) = sp.eval(0.25);
        assert!((y - 1.5).abs() < 1e-15 && (yd - 2.0).abs() < 1e-15 && ydd == 0.0);
    }
}
